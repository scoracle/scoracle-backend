//! Article Reader stage — the layer between Candle scrub and Narratives.
//!
//! Scrub admits an RSS hit as relevant enough to read. This stage then tries to resolve/fetch the
//! publisher page, distill the cleaned body into a compact evidence blurb, persists that card, and
//! reopens Narratives for the vetted entities on the article. When full text proves the match was
//! wrong, it clears the article's vetted links and stops the handoff. When the body cannot be
//! read, it records a terminal fallback row and still wakes Narratives so the old
//! title+description path keeps moving.

use crate::harness::{Harness, Parser};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{hash_components, truncate};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;
use tracing::warn;

pub const ARTICLE_READ_PROMPT_VERSION: &str = "ar3";
pub const ARTICLE_READ_OUTPUT_CONTRACT_VERSION: &str = "article-reading-v3";
const ARTICLE_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const ARTICLE_MIN_WORDS: usize = 80;
const ARTICLE_MAX_MODEL_CHARS: usize = 9_000;
const ARTICLE_MAX_CO_MENTION_CANDIDATES: usize = 24;
const ARTICLE_NUM_PREDICT: i32 = 900;
const ARTICLE_NUM_CTX: i32 = 8192;
const ARTICLE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; ScoracleBot/1.0; +https://scoracle.com)";
const GOOGLE_NEWS_BATCH_URL: &str =
    "https://news.google.com/_/DotsSplashUi/data/batchexecute?rpcids=Fbv4je";

pub const ARTICLE_READ_SYSTEM_PROMPT: &str = r#"Task: compress one already-relevance-vetted sports article for The Journalist.

You are not writing the public story. You are preparing a compact evidence card from the article body so The Journalist can group storylines from richer source material.

Rules:
- Use only the article text and the known vetted entities.
- Set relevant=false when the full article is not materially about any known vetted entity, even if the RSS headline/snippet looked like a match. In that case, do not summarize the unrelated story; give a short evidence_blurb explaining the mismatch.
- Set relevant=true only when the full article materially discusses at least one known vetted entity.
- If co-mention candidates are provided, label every candidate exactly once in co_mentions. Set relevant=true only when the full article materially discusses that candidate; reject name collisions, roundup artifacts, and people/teams merely mentioned in navigation, ads, sidebars, tags, comments, or unrelated links.
- Detect the source article language. Translate meaning into English before writing the evidence card.
- evidence_blurb, key_facts, story_type, and caveats must be English.
- Preserve names, teams, dates, injuries, transactions, quotes-as-claims, scores, and reported uncertainty.
- Do not invent context, implications, or sourcing.
- Keep the evidence_blurb dense and neutral: what happened, who is involved, where it stands, and why it matters.
- If the article is mostly boilerplate, say so in caveats.
- Keep proper names in their canonical/source spelling unless an English name is clearly canonical.

Return strict JSON only:
{"relevant":<true|false>,"source_language":"<ISO 639-1 language code or unknown>","evidence_blurb":"<2-4 compact English sentences, or short mismatch reason when relevant=false>","key_facts":["<English fact>", "..."],"relevant_entities":["<name>", "..."],"co_mentions":[{"candidate":<number>,"relevant":<true|false>}],"story_type":"transfer|injury|performance|fixture|roster|contract|general|irrelevant","caveats":"<short English caveat or empty string>"}"#;

pub fn article_read_format_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "relevant": { "type": "boolean" },
            "source_language": { "type": "string" },
            "evidence_blurb": { "type": "string" },
            "key_facts": { "type": "array", "items": { "type": "string" }, "maxItems": 8 },
            "relevant_entities": { "type": "array", "items": { "type": "string" }, "maxItems": 12 },
            "co_mentions": {
                "type": "array",
                "maxItems": 24,
                "items": {
                    "type": "object",
                    "properties": {
                        "candidate": { "type": "integer" },
                        "relevant": { "type": "boolean" }
                    },
                    "required": ["candidate", "relevant"]
                }
            },
            "story_type": { "type": "string" },
            "caveats": { "type": "string" }
        },
        "required": ["relevant", "source_language", "evidence_blurb", "key_facts", "relevant_entities", "co_mentions", "story_type", "caveats"]
    })
}

#[derive(Debug)]
struct ArticleRow {
    url: String,
    source: String,
    title: String,
    description: String,
    duplicate_of: Option<i64>,
    vetted_count: i64,
}

#[derive(Debug)]
struct FetchedArticle {
    final_url: String,
    final_domain: Option<String>,
    text: String,
}

#[derive(Clone, Debug)]
struct ArticleReadEntities {
    vetted_names: Vec<String>,
    co_mentions: Vec<CoMentionCandidate>,
}

#[derive(Clone, Debug)]
struct CoMentionCandidate {
    number: i32,
    entity_type: String,
    entity_id: i32,
    name: String,
    nationality: String,
    current_club: String,
    position: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArticleEvidence {
    #[serde(default = "default_relevant")]
    pub relevant: bool,
    #[serde(default)]
    pub source_language: String,
    pub evidence_blurb: String,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub relevant_entities: Vec<String>,
    #[serde(default)]
    pub co_mentions: Vec<ArticleCoMentionVerdict>,
    #[serde(default)]
    pub story_type: String,
    #[serde(default)]
    pub caveats: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArticleCoMentionVerdict {
    #[serde(default)]
    pub candidate: i32,
    #[serde(default)]
    pub relevant: bool,
}

pub struct ArticleEvidenceParser;

fn default_relevant() -> bool {
    true
}

impl Parser<ArticleEvidence> for ArticleEvidenceParser {
    fn parse(&self, raw: &str) -> Result<Option<ArticleEvidence>> {
        let Some(slice) = json_object_slice(raw) else {
            return Ok(None);
        };
        let mut evidence: ArticleEvidence = serde_json::from_str(slice)
            .with_context(|| format!("parse article evidence (raw={:?})", truncate(raw, 200)))?;
        evidence.evidence_blurb = normalize_space(&evidence.evidence_blurb);
        evidence.source_language = normalize_language_code(&evidence.source_language);
        evidence.story_type = normalize_space(&evidence.story_type);
        evidence.caveats = normalize_space(&evidence.caveats);
        evidence.key_facts = evidence
            .key_facts
            .into_iter()
            .map(|s| normalize_space(&s))
            .filter(|s| !s.is_empty())
            .take(8)
            .collect();
        evidence.relevant_entities = evidence
            .relevant_entities
            .into_iter()
            .map(|s| normalize_space(&s))
            .filter(|s| !s.is_empty())
            .take(12)
            .collect();
        evidence.co_mentions = evidence
            .co_mentions
            .into_iter()
            .filter(|c| c.candidate > 0)
            .take(ARTICLE_MAX_CO_MENTION_CANDIDATES)
            .collect();
        if evidence.evidence_blurb.is_empty() {
            if !evidence.relevant {
                evidence.evidence_blurb =
                    "Full text is not materially about the vetted entities.".to_string();
                return Ok(Some(evidence));
            }
            return Ok(None);
        }
        Ok(Some(evidence))
    }
}

pub struct ArticleReadHandler;

impl ArticleReadHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ArticleReadHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for ArticleReadHandler {
    fn stage(&self) -> Stage {
        Stage::ArticleRead
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let article_id = item.entity_id;
        let Some(article) = load_article(&hx.pool, article_id, &item.sport).await? else {
            return Ok(());
        };

        if article.duplicate_of.is_some() {
            persist_terminal(hx, article_id, "duplicate", None, None, 0, None).await?;
            return Ok(());
        }
        if article.vetted_count == 0 {
            persist_terminal(hx, article_id, "no_vetted_entities", None, None, 0, None).await?;
            return Ok(());
        }
        let fetched = match fetch_article(&article.url).await {
            Ok(f) => f,
            Err(e) => {
                persist_terminal(
                    hx,
                    article_id,
                    "fetch_failed",
                    None,
                    None,
                    0,
                    Some(&format!("{e:#}")),
                )
                .await?;
                enqueue_narratives_for_article(hx, article_id).await?;
                return Ok(());
            }
        };

        let word_count = count_words(&fetched.text);
        if word_count < ARTICLE_MIN_WORDS {
            let status = if looks_paywalled(&fetched.text) {
                "paywall"
            } else {
                "empty_body"
            };
            persist_terminal(
                hx,
                article_id,
                status,
                Some(&fetched.final_url),
                fetched.final_domain.as_deref(),
                word_count as i32,
                None,
            )
            .await?;
            enqueue_narratives_for_article(hx, article_id).await?;
            return Ok(());
        }

        let content_hash = content_hash(&fetched.text);
        let entities = load_article_read_entities(&hx.pool, article_id, &item.sport).await?;
        if let Some(status) = existing_model_current(
            &hx.pool,
            article_id,
            &content_hash,
            ARTICLE_READ_PROMPT_VERSION,
        )
        .await?
        {
            if status == "irrelevant" {
                let touched = load_vetted_entity_keys(&hx.pool, article_id, &item.sport).await?;
                clear_vetted_entities_for_article(&hx.pool, article_id, &item.sport).await?;
                reject_unresolved_co_mentions(&hx.pool, article_id, &item.sport).await?;
                enqueue_narratives_for_entities(hx, &touched).await?;
            } else if entities.co_mentions.is_empty() {
                enqueue_narratives_for_article(hx, article_id).await?;
            } else {
                // New co-mention tokens arrived after the cached read; rerun the ar3 prompt so the
                // full-text verdict can promote/reject them.
            }
            if status == "irrelevant" || entities.co_mentions.is_empty() {
                return Ok(());
            }
        }

        let prompt = build_article_read_prompt(&article, &fetched.text, &entities);
        let opts = GenerateOptions {
            system: Some(ARTICLE_READ_SYSTEM_PROMPT.to_string()),
            temperature: Some(0.2),
            num_predict: ARTICLE_NUM_PREDICT,
            num_ctx: ARTICLE_NUM_CTX,
            json_mode: false,
            format_schema: Some(article_read_format_schema()),
        };
        let extracted = hx
            .extract(Role::ArticleReader, &prompt, &opts, &ArticleEvidenceParser)
            .await?;
        let Some(evidence) = extracted.value else {
            persist_terminal(
                hx,
                article_id,
                "parse_failed",
                Some(&fetched.final_url),
                fetched.final_domain.as_deref(),
                word_count as i32,
                Some("article evidence parser returned no committed blurb"),
            )
            .await?;
            enqueue_narratives_for_article(hx, article_id).await?;
            return Ok(());
        };

        if !evidence.relevant {
            let touched = load_vetted_entity_keys(&hx.pool, article_id, &item.sport).await?;
            persist_model_outcome(
                hx,
                article_id,
                "irrelevant",
                &fetched,
                word_count as i32,
                &content_hash,
                &evidence,
                &extracted.model,
            )
            .await?;
            clear_vetted_entities_for_article(&hx.pool, article_id, &item.sport).await?;
            reject_unresolved_co_mentions(&hx.pool, article_id, &item.sport).await?;
            enqueue_narratives_for_entities(hx, &touched).await?;
            return Ok(());
        }

        persist_model_outcome(
            hx,
            article_id,
            "success",
            &fetched,
            word_count as i32,
            &content_hash,
            &evidence,
            &extracted.model,
        )
        .await?;
        apply_co_mention_verdicts(
            &hx.pool,
            article_id,
            &item.sport,
            &entities.co_mentions,
            &evidence.co_mentions,
        )
        .await?;
        enqueue_narratives_for_article(hx, article_id).await?;
        Ok(())
    }
}

async fn load_article(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Option<ArticleRow>> {
    let row = sqlx::query(
        r#"
        SELECT a.url, COALESCE(a.source, '') AS source, a.title,
               COALESCE(a.description, '') AS description, a.duplicate_of,
               count(nae.*) FILTER (WHERE nae.vetted IS TRUE) AS vetted_count
        FROM public.news_articles a
        LEFT JOIN public.news_article_entities nae
          ON nae.article_id = a.id AND nae.sport = $2
        WHERE a.id = $1
        GROUP BY a.id, a.url, a.source, a.title, a.description, a.duplicate_of
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .fetch_optional(pool)
    .await
    .context("load article for article_read")?;

    Ok(row.map(|r| ArticleRow {
        url: r.get("url"),
        source: r.get("source"),
        title: r.get("title"),
        description: r.get("description"),
        duplicate_of: r.get("duplicate_of"),
        vetted_count: r.get("vetted_count"),
    }))
}

async fn existing_model_current(
    pool: &sqlx::PgPool,
    article_id: i64,
    content_hash: &str,
    prompt_version: &str,
) -> Result<Option<String>> {
    let current: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status
        FROM public.news_article_readings
        WHERE article_id = $1
          AND status IN ('success', 'irrelevant')
          AND content_hash = $2
          AND prompt_version = $3
        "#,
    )
    .bind(article_id)
    .bind(content_hash)
    .bind(prompt_version)
    .fetch_optional(pool)
    .await
    .context("check existing article reading")?;
    Ok(current)
}

async fn fetch_article(raw_url: &str) -> Result<FetchedArticle> {
    let client = reqwest::Client::builder()
        .timeout(ARTICLE_FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(ARTICLE_FETCH_USER_AGENT)
        .build()
        .context("build article fetch client")?;

    let fetch_url = match resolve_google_news_article_url(&client, raw_url).await {
        Ok(Some(resolved)) => resolved,
        Ok(None) => raw_url.to_string(),
        Err(e) => {
            warn!(url = raw_url, error = %format!("{e:#}"), "google news url resolution failed");
            raw_url.to_string()
        }
    };

    let resp = client
        .get(&fetch_url)
        .send()
        .await
        .context("fetch article")?;
    let final_url = resp.url().to_string();
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(anyhow!("article HTTP {}", status.as_u16()));
    }
    if !status.is_success() {
        return Err(anyhow!("article HTTP {}", status.as_u16()));
    }
    let html = resp.text().await.context("read article body")?;
    let mut text = clean_html(&html);
    if count_words(&text) < ARTICLE_MIN_WORDS {
        if let Some(rendered) = fetch_with_chrome(&fetch_url) {
            let rendered_text = clean_html(&rendered);
            if count_words(&rendered_text) > count_words(&text) {
                text = rendered_text;
            }
        }
    }
    Ok(FetchedArticle {
        final_domain: domain_of(&final_url),
        final_url,
        text,
    })
}

fn fetch_with_chrome(raw_url: &str) -> Option<String> {
    if std::env::var("ARTICLE_READ_CHROME_ENABLED").ok().as_deref() != Some("1") {
        return None;
    }
    let output = Command::new("timeout")
        .arg("20s")
        .arg("google-chrome-stable")
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--dump-dom")
        .arg(raw_url)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn build_article_read_prompt(
    article: &ArticleRow,
    text: &str,
    entities: &ArticleReadEntities,
) -> String {
    let mut p = String::new();
    p.push_str(&format!("Source: {}\n", article.source));
    p.push_str(&format!("Title: {}\n", article.title));
    if !article.description.trim().is_empty() {
        p.push_str(&format!("RSS description: {}\n", article.description));
    }
    if !entities.vetted_names.is_empty() {
        p.push_str("\nKnown vetted entities:\n");
        for e in &entities.vetted_names {
            p.push_str("- ");
            p.push_str(e);
            p.push('\n');
        }
    }
    if !entities.co_mentions.is_empty() {
        p.push_str("\nCo-mention candidates to verify from full text:\n");
        for c in &entities.co_mentions {
            p.push_str(&format!(
                "{}. {} ({} {}, {})\n",
                c.number,
                c.name,
                c.entity_type,
                c.entity_id,
                co_mention_identity(c)
            ));
        }
    }
    p.push_str("\nArticle text:\n");
    p.push_str(&truncate(&normalize_space(text), ARTICLE_MAX_MODEL_CHARS));
    p.push_str("\n\nReturn the JSON object now.");
    p
}

async fn load_article_read_entities(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<ArticleReadEntities> {
    let sport = sport.to_uppercase();
    let rows: Vec<(String, i32, String)> = sqlx::query_as(
        r#"
        SELECT nae.entity_type, nae.entity_id, COALESCE(p.name, t.name, '') AS name
        FROM public.news_article_entities nae
        LEFT JOIN public.players p
          ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN public.teams t
          ON nae.entity_type = 'team' AND t.id = nae.entity_id AND t.sport = nae.sport
        WHERE nae.article_id = $1 AND nae.sport = $2 AND nae.vetted IS TRUE
        ORDER BY nae.entity_type, nae.entity_id
        "#,
    )
    .bind(article_id)
    .bind(&sport)
    .fetch_all(pool)
    .await
    .context("load article_read vetted entity names")?;

    let vetted_names = rows
        .into_iter()
        .map(|(entity_type, entity_id, name)| format!("{name} ({entity_type} {entity_id})"))
        .collect();

    let rows = sqlx::query(
        r#"
        SELECT nae.entity_type, nae.entity_id,
               COALESCE(p.name, t.name, '')                  AS name,
               COALESCE(p.nationality, '')                   AS nationality,
               COALESCE(ct.name, '')                         AS current_club,
               COALESCE(NULLIF(pci.position, 'Unknown'), '') AS position
        FROM public.news_article_entities nae
        LEFT JOIN public.players p
          ON nae.entity_type = 'player' AND p.id = nae.entity_id AND p.sport = nae.sport
        LEFT JOIN public.teams t
          ON nae.entity_type = 'team' AND t.id = nae.entity_id AND t.sport = nae.sport
        LEFT JOIN public.player_current_identity pci
          ON nae.entity_type = 'player' AND pci.player_id = nae.entity_id AND pci.sport = nae.sport
        LEFT JOIN public.teams ct
          ON ct.id = pci.team_id AND ct.sport = nae.sport
        WHERE nae.article_id = $1
          AND nae.sport = $2
          AND nae.vetted IS NULL
          AND nae.scrubbed_at IS NOT NULL
          AND nae.match_confidence < 0.95
        ORDER BY nae.title_pos NULLS LAST, nae.match_confidence DESC, nae.entity_type, nae.entity_id
        LIMIT $3
        "#,
    )
    .bind(article_id)
    .bind(&sport)
    .bind(ARTICLE_MAX_CO_MENTION_CANDIDATES as i64)
    .fetch_all(pool)
    .await
    .context("load article_read co-mention candidates")?;

    let co_mentions = rows
        .into_iter()
        .enumerate()
        .map(|(idx, r)| CoMentionCandidate {
            number: (idx + 1) as i32,
            entity_type: r.get("entity_type"),
            entity_id: r.get("entity_id"),
            name: r.get("name"),
            nationality: r.get("nationality"),
            current_club: r.get("current_club"),
            position: r.get("position"),
        })
        .collect();

    Ok(ArticleReadEntities {
        vetted_names,
        co_mentions,
    })
}

fn co_mention_identity(c: &CoMentionCandidate) -> String {
    let mut parts = Vec::new();
    if !c.position.is_empty() {
        parts.push(c.position.as_str());
    }
    if !c.current_club.is_empty() {
        parts.push(c.current_club.as_str());
    }
    if !c.nationality.is_empty() {
        parts.push(c.nationality.as_str());
    }
    if parts.is_empty() {
        "no identity card".to_string()
    } else {
        parts.join(", ")
    }
}

async fn persist_terminal(
    hx: &Harness,
    article_id: i64,
    status: &str,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    extracted_words: i32,
    last_error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.news_article_readings (
            article_id, status, final_url, final_domain, extracted_words,
            evidence_blurb, evidence, model_version, prompt_version, parser_outcome,
            last_error, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5,
            NULL, '{}'::jsonb, NULL, $6, 'no_call',
            $7, NOW(), NOW()
        )
        ON CONFLICT (article_id) DO UPDATE SET
            status = EXCLUDED.status,
            final_url = EXCLUDED.final_url,
            final_domain = EXCLUDED.final_domain,
            content_hash = NULL,
            extracted_words = EXCLUDED.extracted_words,
            evidence_blurb = NULL,
            evidence = '{}'::jsonb,
            model_version = NULL,
            prompt_version = EXCLUDED.prompt_version,
            parser_outcome = EXCLUDED.parser_outcome,
            last_error = EXCLUDED.last_error,
            fetched_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(article_id)
    .bind(status)
    .bind(final_url)
    .bind(final_domain)
    .bind(extracted_words)
    .bind(ARTICLE_READ_PROMPT_VERSION)
    .bind(last_error.map(|e| truncate(e, 1000)))
    .execute(&hx.pool)
    .await
    .with_context(|| format!("persist article_read terminal {article_id}"))?;
    insert_data_fetch_ledger_best_effort(
        hx,
        article_id,
        status,
        final_url,
        final_url,
        final_domain,
        None,
        None,
        ARTICLE_READ_PROMPT_VERSION,
        ARTICLE_READ_OUTPUT_CONTRACT_VERSION,
        "no_call",
        last_error,
    )
    .await;
    Ok(())
}

async fn persist_model_outcome(
    hx: &Harness,
    article_id: i64,
    status: &str,
    fetched: &FetchedArticle,
    extracted_words: i32,
    content_hash: &str,
    evidence: &ArticleEvidence,
    model_version: &str,
) -> Result<()> {
    let evidence_json = json!({
        "output_contract_version": ARTICLE_READ_OUTPUT_CONTRACT_VERSION,
        "relevant": evidence.relevant,
        "source_language": evidence.source_language,
        "evidence_blurb": evidence.evidence_blurb,
        "key_facts": evidence.key_facts,
        "relevant_entities": evidence.relevant_entities,
        "co_mentions": evidence.co_mentions.iter().map(|v| json!({
            "candidate": v.candidate,
            "relevant": v.relevant,
        })).collect::<Vec<_>>(),
        "story_type": evidence.story_type,
        "caveats": evidence.caveats,
    });
    sqlx::query(
        r#"
        INSERT INTO public.news_article_readings (
            article_id, status, final_url, final_domain, content_hash, extracted_words,
            evidence_blurb, evidence, model_version, prompt_version, parser_outcome,
            last_error, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8::jsonb, $9, $10, 'parsed',
            NULL, NOW(), NOW()
        )
        ON CONFLICT (article_id) DO UPDATE SET
            status = EXCLUDED.status,
            final_url = EXCLUDED.final_url,
            final_domain = EXCLUDED.final_domain,
            content_hash = EXCLUDED.content_hash,
            extracted_words = EXCLUDED.extracted_words,
            evidence_blurb = EXCLUDED.evidence_blurb,
            evidence = EXCLUDED.evidence,
            model_version = EXCLUDED.model_version,
            prompt_version = EXCLUDED.prompt_version,
            parser_outcome = EXCLUDED.parser_outcome,
            last_error = NULL,
            fetched_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(article_id)
    .bind(status)
    .bind(&fetched.final_url)
    .bind(fetched.final_domain.as_deref())
    .bind(content_hash)
    .bind(extracted_words)
    .bind(&evidence.evidence_blurb)
    .bind(evidence_json)
    .bind(model_version)
    .bind(ARTICLE_READ_PROMPT_VERSION)
    .execute(&hx.pool)
    .await
    .with_context(|| format!("persist article_read {status} {article_id}"))?;
    insert_data_fetch_ledger_best_effort(
        hx,
        article_id,
        status,
        Some(&fetched.final_url),
        Some(&fetched.final_url),
        fetched.final_domain.as_deref(),
        Some(content_hash),
        Some(model_version),
        ARTICLE_READ_PROMPT_VERSION,
        ARTICLE_READ_OUTPUT_CONTRACT_VERSION,
        "parsed",
        None,
    )
    .await;
    Ok(())
}

async fn clear_vetted_entities_for_article(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE public.news_article_entities
           SET vetted = FALSE, scrubbed_at = COALESCE(scrubbed_at, NOW())
         WHERE article_id = $1 AND sport = $2 AND vetted IS TRUE
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .execute(pool)
    .await
    .with_context(|| format!("clear vetted entities for irrelevant article {article_id}"))?;
    Ok(())
}

async fn reject_unresolved_co_mentions(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE public.news_article_entities
           SET vetted = FALSE, scrubbed_at = COALESCE(scrubbed_at, NOW())
         WHERE article_id = $1
           AND sport = $2
           AND vetted IS NULL
           AND scrubbed_at IS NOT NULL
           AND match_confidence < 0.95
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .execute(pool)
    .await
    .with_context(|| format!("reject unresolved co-mentions for article {article_id}"))?;
    Ok(())
}

async fn apply_co_mention_verdicts(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
    candidates: &[CoMentionCandidate],
    verdicts: &[ArticleCoMentionVerdict],
) -> Result<()> {
    if candidates.is_empty() {
        return Ok(());
    }

    let kept: HashSet<i32> = verdicts
        .iter()
        .filter(|v| v.relevant)
        .map(|v| v.candidate)
        .collect();
    let entity_types: Vec<String> = candidates.iter().map(|c| c.entity_type.clone()).collect();
    let entity_ids: Vec<i32> = candidates.iter().map(|c| c.entity_id).collect();
    let relevants: Vec<bool> = candidates
        .iter()
        .map(|c| kept.contains(&c.number))
        .collect();

    sqlx::query(
        r#"
        UPDATE public.news_article_entities n
           SET vetted = v.relevant, scrubbed_at = NOW()
          FROM unnest($2::text[], $3::int[], $4::bool[]) AS v(entity_type, entity_id, relevant)
         WHERE n.article_id = $1
           AND n.sport = $5
           AND n.entity_type = v.entity_type
           AND n.entity_id = v.entity_id
           AND n.vetted IS NULL
        "#,
    )
    .bind(article_id)
    .bind(&entity_types)
    .bind(&entity_ids)
    .bind(&relevants)
    .bind(sport.to_uppercase())
    .execute(pool)
    .await
    .with_context(|| format!("apply co-mention verdicts for article {article_id}"))?;
    Ok(())
}

async fn load_vetted_entity_keys(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Vec<(String, i32, String)>> {
    sqlx::query_as(
        r#"
        SELECT DISTINCT entity_type, entity_id, sport
        FROM public.news_article_entities
        WHERE article_id = $1 AND sport = $2 AND vetted IS TRUE
        "#,
    )
    .bind(article_id)
    .bind(sport.to_uppercase())
    .fetch_all(pool)
    .await
    .with_context(|| format!("load vetted entity keys for article {article_id}"))
}

async fn enqueue_narratives_for_entities(
    hx: &Harness,
    entities: &[(String, i32, String)],
) -> Result<()> {
    for (entity_type, entity_id, sport) in entities {
        let input_version: String = sqlx::query_scalar(
            r#"
            WITH corpus AS (
                SELECT nae.article_id,
                       COALESCE(r.status, 'none') AS read_status,
                       COALESCE(r.content_hash, '') AS content_hash,
                       COALESCE(EXTRACT(EPOCH FROM r.updated_at)::bigint::text, '0') AS read_updated
                FROM public.news_article_entities nae
                JOIN public.news_articles a ON a.id = nae.article_id
                LEFT JOIN public.news_article_readings r ON r.article_id = a.id
                WHERE nae.entity_type = $1
                  AND nae.entity_id = $2
                  AND nae.sport = $3
                  AND nae.vetted IS TRUE
                  AND a.duplicate_of IS NULL
                  AND (
                      a.published_at IS NULL
                      OR a.published_at > NOW() - INTERVAL '72 hours'
                      OR r.updated_at > NOW() - INTERVAL '72 hours'
                  )
            )
            SELECT 'n:' || count(*) || ':' ||
                   md5(COALESCE(string_agg(
                       article_id::text || ':' || read_status || ':' || content_hash || ':' || read_updated,
                       ',' ORDER BY article_id
                   ), ''))
            FROM corpus
            "#,
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport.to_uppercase())
        .fetch_one(&hx.pool)
        .await
        .with_context(|| format!("compute narratives handoff after article rejection {entity_type}/{entity_id}"))?;

        work::enqueue(
            &hx.pool,
            &Item {
                stage: Stage::Narratives,
                entity_type: entity_type.clone(),
                entity_id: i64::from(*entity_id),
                sport: sport.to_uppercase(),
                input_version: Some(input_version),
                attempts: 0,
            },
        )
        .await?;
    }
    Ok(())
}

async fn insert_data_fetch_ledger_best_effort(
    hx: &Harness,
    article_id: i64,
    status: &str,
    source_url: Option<&str>,
    final_url: Option<&str>,
    final_domain: Option<&str>,
    content_hash: Option<&str>,
    model_version: Option<&str>,
    prompt_version: &str,
    output_contract_version: &str,
    parser_outcome: &str,
    error: Option<&str>,
) {
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO public.data_fetch_ledger (
            target_type, target_id, stage, status, source_url, final_url, final_domain,
            content_hash, model_version, prompt_version, output_contract_version,
            parser_outcome, error, generated_at
        ) VALUES (
            'article', $1, 'article_read', $2, $3, $4, $5,
            $6, $7, $8, $9, $10, $11, NOW()
        )
        "#,
    )
    .bind(article_id)
    .bind(status)
    .bind(source_url)
    .bind(final_url)
    .bind(final_domain)
    .bind(content_hash)
    .bind(model_version)
    .bind(prompt_version)
    .bind(output_contract_version)
    .bind(parser_outcome)
    .bind(error.map(|e| truncate(e, 1000)))
    .execute(&hx.pool)
    .await
    {
        warn!(
            article_id,
            status,
            error = %e,
            "article_read: data_fetch_ledger insert failed (continuing)"
        );
    }
}

async fn enqueue_narratives_for_article(hx: &Harness, article_id: i64) -> Result<()> {
    let rows: Vec<(String, i32, String, String)> = sqlx::query_as(
        r#"
        WITH touched AS (
            SELECT DISTINCT entity_type, entity_id, sport
            FROM public.news_article_entities
            WHERE article_id = $1 AND vetted IS TRUE
        )
        SELECT t.entity_type, t.entity_id, t.sport,
               'n:' || count(*) || ':' ||
               md5(string_agg(
                   nae.article_id::text || ':' ||
                   COALESCE(r.status, 'none') || ':' ||
                   COALESCE(r.content_hash, '') || ':' ||
                   COALESCE(EXTRACT(EPOCH FROM r.updated_at)::bigint::text, '0'),
                   ',' ORDER BY nae.article_id
               )) AS input_version
        FROM touched t
        JOIN public.news_article_entities nae
          ON nae.entity_type = t.entity_type
         AND nae.entity_id = t.entity_id
         AND nae.sport = t.sport
         AND nae.vetted IS TRUE
        JOIN public.news_articles a ON a.id = nae.article_id
        LEFT JOIN public.news_article_readings r ON r.article_id = a.id
        WHERE a.duplicate_of IS NULL
          AND (
              a.published_at IS NULL
              OR a.published_at > NOW() - INTERVAL '72 hours'
              OR r.updated_at > NOW() - INTERVAL '72 hours'
          )
        GROUP BY t.entity_type, t.entity_id, t.sport
        HAVING count(*) > 0
        "#,
    )
    .bind(article_id)
    .fetch_all(&hx.pool)
    .await
    .context("compute article_read narratives handoff")?;

    for (entity_type, entity_id, sport, input_version) in rows {
        work::enqueue(
            &hx.pool,
            &Item {
                stage: Stage::Narratives,
                entity_type,
                entity_id: i64::from(entity_id),
                sport,
                input_version: Some(input_version),
                attempts: 0,
            },
        )
        .await?;
    }
    Ok(())
}

fn content_hash(text: &str) -> String {
    let digest = Sha256::digest(normalize_space(text).as_bytes());
    hex::encode(&digest[..16])
}

fn domain_of(raw_url: &str) -> Option<String> {
    reqwest::Url::parse(raw_url).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_lowercase())
    })
}

async fn resolve_google_news_article_url(
    client: &reqwest::Client,
    raw_url: &str,
) -> Result<Option<String>> {
    let Some(article_id) = google_news_article_id(raw_url) else {
        return Ok(None);
    };

    let html = client
        .get(raw_url)
        .send()
        .await
        .context("fetch google news wrapper")?
        .text()
        .await
        .context("read google news wrapper")?;
    let resolved_id = html_attr(&html, "data-n-a-id").unwrap_or(article_id);
    let Some(timestamp) = html_attr(&html, "data-n-a-ts").and_then(|v| v.parse::<i64>().ok())
    else {
        return Ok(None);
    };
    let Some(signature) = html_attr(&html, "data-n-a-sg") else {
        return Ok(None);
    };
    let payload = google_news_resolve_payload(&resolved_id, timestamp, &signature);
    let body = client
        .post(GOOGLE_NEWS_BATCH_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded;charset=utf-8",
        )
        .form(&[("f.req", payload)])
        .send()
        .await
        .context("post google news resolver")?
        .text()
        .await
        .context("read google news resolver")?;
    Ok(parse_google_news_resolver_response(&body))
}

fn google_news_article_id(raw_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw_url).ok()?;
    if url.host_str()? != "news.google.com" {
        return None;
    }
    let mut segments = url.path_segments()?;
    if segments.next()? != "rss" || segments.next()? != "articles" {
        return None;
    }
    let id = segments.next()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn html_attr(html: &str, attr: &str) -> Option<String> {
    let needle = format!(r#"{attr}=""#);
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find('"')?;
    Some(decode_entities(&html[start..start + end]))
}

fn google_news_resolve_payload(article_id: &str, timestamp: i64, signature: &str) -> String {
    let request = json!([
        "garturlreq",
        [
            [
                "en-US",
                "US",
                [
                    "FINANCE_TOP_INDICES",
                    "GENESIS_PUBLISHER_SECTION",
                    "WEB_TEST_1_0_0"
                ],
                null,
                null,
                1,
                1,
                "US:en",
                null,
                null,
                null,
                null,
                null,
                null,
                null,
                0,
                5
            ],
            "en-US",
            "US",
            1,
            [2, 3, 4, 8],
            1,
            0,
            "655000234",
            0,
            0,
            null,
            0
        ],
        article_id,
        timestamp,
        signature
    ])
    .to_string();
    json!([[["Fbv4je", request, null, "generic"]]]).to_string()
}

fn parse_google_news_resolver_response(body: &str) -> Option<String> {
    let start = body.find("[[")?;
    let outer: serde_json::Value = serde_json::from_str(&body[start..]).ok()?;
    for row in outer.as_array()? {
        let row = row.as_array()?;
        if row.first()?.as_str()? != "wrb.fr" || row.get(1)?.as_str()? != "Fbv4je" {
            continue;
        }
        let inner: serde_json::Value = serde_json::from_str(row.get(2)?.as_str()?).ok()?;
        let inner = inner.as_array()?;
        if inner.first()?.as_str()? != "garturlres" {
            continue;
        }
        let url = inner.get(1)?.as_str()?.trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(url.to_string());
        }
    }
    None
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.len() > 1).count()
}

fn looks_paywalled(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("subscribe")
        || lower.contains("subscription")
        || lower.contains("sign in")
        || lower.contains("sign up")
        || lower.contains("register to continue")
}

fn normalize_language_code(raw: &str) -> String {
    let s = raw.trim().to_lowercase();
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_lowercase()) {
        return s;
    }
    "unknown".to_string()
}

fn clean_html(html: &str) -> String {
    let without_scripts = strip_element_blocks(html, "script");
    let without_styles = strip_element_blocks(&without_scripts, "style");
    let mut out = String::with_capacity(without_styles.len());
    let mut in_tag = false;
    for c in without_styles.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&normalize_space(&out))
}

fn strip_element_blocks(html: &str, tag: &str) -> String {
    let mut out = String::new();
    let lower = html.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut pos = 0usize;
    while let Some(start_rel) = lower[pos..].find(&open) {
        let start = pos + start_rel;
        out.push_str(&html[pos..start]);
        if let Some(end_rel) = lower[start..].find(&close) {
            pos = start + end_rel + close.len();
        } else {
            pos = html.len();
            break;
        }
    }
    out.push_str(&html[pos..]);
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_object_slice(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if *b == b'\\' {
                esc = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        match *b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start.map(|s| &raw[s..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn reading_fingerprint(
    status: Option<&str>,
    content_hash: Option<&str>,
    updated_epoch: Option<i64>,
) -> String {
    format!(
        "{}:{}:{}",
        status.unwrap_or("none"),
        content_hash.unwrap_or(""),
        updated_epoch.unwrap_or(0)
    )
}

pub fn build_article_reading_input_components(items: &[(i64, String)]) -> String {
    let mut pairs = items.to_vec();
    pairs.sort_by_key(|(id, _)| *id);
    let mut out = String::from("[");
    for (i, (id, fp)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        out.push_str(&id.to_string());
        out.push(',');
        out.push_str(&crate::util::go_json_string(fp));
        out.push(']');
    }
    out.push(']');
    hash_components(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_html_removes_tags_scripts_and_normalizes_space() {
        let html = "<html><script>bad()</script><body><h1>Title</h1><p>A&nbsp;B &amp; C.</p></body></html>";
        assert_eq!(clean_html(html), "Title A B & C.");
    }

    #[test]
    fn google_news_article_id_extracts_rss_token() {
        let url = "https://news.google.com/rss/articles/CBMiabc123?oc=5&hl=en-US";
        assert_eq!(google_news_article_id(url).as_deref(), Some("CBMiabc123"));
        assert!(google_news_article_id("https://example.com/rss/articles/CBMiabc123").is_none());
    }

    #[test]
    fn html_attr_extracts_google_news_tokens() {
        let html =
            r#"<div data-n-a-id="CBMiabc" data-n-a-ts="1784915408" data-n-a-sg="A&amp;B"></div>"#;
        assert_eq!(html_attr(html, "data-n-a-id").as_deref(), Some("CBMiabc"));
        assert_eq!(
            html_attr(html, "data-n-a-ts").as_deref(),
            Some("1784915408")
        );
        assert_eq!(html_attr(html, "data-n-a-sg").as_deref(), Some("A&B"));
    }

    #[test]
    fn google_news_resolver_response_extracts_publisher_url() {
        let body = r#")]}'

[["wrb.fr","Fbv4je","[\"garturlres\",\"https://www.goal.com/en/news/example\",1]",null,null,null,"generic"],["di",23]]"#;
        assert_eq!(
            parse_google_news_resolver_response(body).as_deref(),
            Some("https://www.goal.com/en/news/example")
        );
    }

    #[test]
    fn parser_accepts_compact_evidence_card() {
        let raw = r#"{"source_language":"DE","evidence_blurb":"  Player X returned to training.  ","key_facts":[" one ",""],"relevant_entities":["Club"],"co_mentions":[],"story_type":" injury ","caveats":""}"#;
        let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
        assert_eq!(parsed.source_language, "de");
        assert_eq!(parsed.evidence_blurb, "Player X returned to training.");
        assert_eq!(parsed.key_facts, vec!["one"]);
        assert_eq!(parsed.story_type, "injury");
    }

    #[test]
    fn parser_accepts_co_mention_verdicts() {
        let raw = r#"{"relevant":true,"source_language":"en","evidence_blurb":"A filed item.","key_facts":[],"relevant_entities":["Club"],"co_mentions":[{"candidate":2,"relevant":true},{"candidate":0,"relevant":true},{"candidate":3,"relevant":false}],"story_type":"general","caveats":""}"#;
        let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
        assert_eq!(parsed.co_mentions.len(), 2);
        assert_eq!(parsed.co_mentions[0].candidate, 2);
        assert!(parsed.co_mentions[0].relevant);
        assert_eq!(parsed.co_mentions[1].candidate, 3);
        assert!(!parsed.co_mentions[1].relevant);
    }

    #[test]
    fn parser_fails_closed_on_empty_blurb() {
        let parsed = ArticleEvidenceParser
            .parse(r#"{"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"general","caveats":""}"#)
            .unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn parser_accepts_irrelevant_without_blurb() {
        let parsed = ArticleEvidenceParser
            .parse(r#"{"relevant":false,"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"irrelevant","caveats":""}"#)
            .unwrap()
            .unwrap();
        assert!(!parsed.relevant);
        assert_eq!(
            parsed.evidence_blurb,
            "Full text is not materially about the vetted entities."
        );
    }

    #[test]
    fn reading_fingerprint_distinguishes_body_changes() {
        assert_ne!(
            reading_fingerprint(Some("success"), Some("a"), Some(1)),
            reading_fingerprint(Some("success"), Some("b"), Some(1)),
        );
        assert_eq!(reading_fingerprint(None, None, None), "none::0");
    }

    #[test]
    fn article_reading_input_hash_is_order_stable() {
        let a = build_article_reading_input_components(&[
            (2, "success:b:9".to_string()),
            (1, "none::0".to_string()),
        ]);
        let b = build_article_reading_input_components(&[
            (1, "none::0".to_string()),
            (2, "success:b:9".to_string()),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn prompt_renders_co_mention_candidates_by_number() {
        let article = ArticleRow {
            url: "https://example.test/a".to_string(),
            source: "Example".to_string(),
            title: "Club tracks midfielder".to_string(),
            description: String::new(),
            duplicate_of: None,
            vetted_count: 1,
        };
        let entities = ArticleReadEntities {
            vetted_names: vec!["Manchester United (team 14)".to_string()],
            co_mentions: vec![CoMentionCandidate {
                number: 1,
                entity_type: "player".to_string(),
                entity_id: 70,
                name: "Example Midfielder".to_string(),
                nationality: "England".to_string(),
                current_club: "Leeds".to_string(),
                position: "Midfielder".to_string(),
            }],
        };

        let prompt = build_article_read_prompt(&article, "The body text.", &entities);
        assert!(prompt.contains("Known vetted entities"));
        assert!(prompt.contains("Co-mention candidates"));
        assert!(prompt.contains("1. Example Midfielder (player 70, Midfielder, Leeds, England)"));
    }
}
