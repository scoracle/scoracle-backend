//! Article Reader stage — the layer between Candle scrub and Narratives.
//!
//! Scrub proves an RSS hit is relevant. This stage then tries to resolve/fetch the publisher page,
//! distill the cleaned body into a compact evidence blurb, persists that card, and reopens
//! Narratives for the vetted entities on the article. When a source is not allowlisted or the body
//! cannot be read, the stage records a terminal fallback row and still wakes Narratives so the old
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
use std::process::Command;
use std::time::Duration;
use tracing::warn;

pub const ARTICLE_READ_PROMPT_VERSION: &str = "ar1";
pub const ARTICLE_READ_OUTPUT_CONTRACT_VERSION: &str = "article-reading-v1";
const ARTICLE_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const ARTICLE_MIN_WORDS: usize = 80;
const ARTICLE_MAX_MODEL_CHARS: usize = 9_000;
const ARTICLE_NUM_PREDICT: i32 = 900;
const ARTICLE_NUM_CTX: i32 = 8192;

pub const ARTICLE_READ_SYSTEM_PROMPT: &str = r#"Task: compress one already-relevance-vetted sports article for The Journalist.

You are not writing the public story. You are preparing a compact evidence card from the article body so The Journalist can group storylines from richer source material.

Rules:
- Use only the article text and the known vetted entities.
- Detect the source article language. Translate meaning into English before writing the evidence card.
- evidence_blurb, key_facts, story_type, and caveats must be English.
- Preserve names, teams, dates, injuries, transactions, quotes-as-claims, scores, and reported uncertainty.
- Do not invent context, implications, or sourcing.
- Keep the evidence_blurb dense and neutral: what happened, who is involved, where it stands, and why it matters.
- If the article is mostly boilerplate, say so in caveats.
- Keep proper names in their canonical/source spelling unless an English name is clearly canonical.

Return strict JSON only:
{"source_language":"<ISO 639-1 language code or unknown>","evidence_blurb":"<2-4 compact English sentences>","key_facts":["<English fact>", "..."],"relevant_entities":["<name>", "..."],"story_type":"transfer|injury|performance|fixture|roster|contract|general","caveats":"<short English caveat or empty string>"}"#;

pub fn article_read_format_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "source_language": { "type": "string" },
            "evidence_blurb": { "type": "string" },
            "key_facts": { "type": "array", "items": { "type": "string" }, "maxItems": 8 },
            "relevant_entities": { "type": "array", "items": { "type": "string" }, "maxItems": 12 },
            "story_type": { "type": "string" },
            "caveats": { "type": "string" }
        },
        "required": ["source_language", "evidence_blurb", "key_facts", "relevant_entities", "story_type", "caveats"]
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
    allowlisted: bool,
}

#[derive(Debug)]
struct FetchedArticle {
    final_url: String,
    final_domain: Option<String>,
    text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArticleEvidence {
    #[serde(default)]
    pub source_language: String,
    pub evidence_blurb: String,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub relevant_entities: Vec<String>,
    #[serde(default)]
    pub story_type: String,
    #[serde(default)]
    pub caveats: String,
}

pub struct ArticleEvidenceParser;

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
        if evidence.evidence_blurb.is_empty() {
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
        if !article.allowlisted {
            persist_terminal(hx, article_id, "not_allowlisted", None, None, 0, None).await?;
            enqueue_narratives_for_article(hx, article_id).await?;
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
        if existing_success_current(&hx.pool, article_id, &content_hash).await? {
            enqueue_narratives_for_article(hx, article_id).await?;
            return Ok(());
        }

        let entities = load_vetted_entity_names(&hx.pool, article_id, &item.sport).await?;
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

        persist_success(
            hx,
            article_id,
            &fetched,
            word_count as i32,
            &content_hash,
            &evidence,
            &extracted.model,
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
               count(nae.*) FILTER (WHERE nae.vetted IS TRUE) AS vetted_count,
               EXISTS (
                   SELECT 1 FROM public.source_tiers st
                   WHERE st.kind = 'news'
                     AND st.tier <= 2
                     AND lower(st.source) = lower(COALESCE(a.source, ''))
               ) AS allowlisted
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
        allowlisted: r.get("allowlisted"),
    }))
}

async fn existing_success_current(
    pool: &sqlx::PgPool,
    article_id: i64,
    content_hash: &str,
) -> Result<bool> {
    let current: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM public.news_article_readings
        WHERE article_id = $1 AND status = 'success' AND content_hash = $2
        "#,
    )
    .bind(article_id)
    .bind(content_hash)
    .fetch_optional(pool)
    .await
    .context("check existing article reading")?;
    Ok(current.is_some())
}

async fn fetch_article(raw_url: &str) -> Result<FetchedArticle> {
    let client = reqwest::Client::builder()
        .timeout(ARTICLE_FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Mozilla/5.0 (compatible; ScoracleBot/1.0; +https://scoracle.com)")
        .build()
        .context("build article fetch client")?;

    let resp = client.get(raw_url).send().await.context("fetch article")?;
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
        if let Some(rendered) = fetch_with_chrome(raw_url) {
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

fn build_article_read_prompt(article: &ArticleRow, text: &str, entities: &[String]) -> String {
    let mut p = String::new();
    p.push_str(&format!("Source: {}\n", article.source));
    p.push_str(&format!("Title: {}\n", article.title));
    if !article.description.trim().is_empty() {
        p.push_str(&format!("RSS description: {}\n", article.description));
    }
    if !entities.is_empty() {
        p.push_str("\nKnown vetted entities:\n");
        for e in entities {
            p.push_str("- ");
            p.push_str(e);
            p.push('\n');
        }
    }
    p.push_str("\nArticle text:\n");
    p.push_str(&truncate(&normalize_space(text), ARTICLE_MAX_MODEL_CHARS));
    p.push_str("\n\nReturn the JSON object now.");
    p
}

async fn load_vetted_entity_names(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Vec<String>> {
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
    .bind(sport.to_uppercase())
    .fetch_all(pool)
    .await
    .context("load article_read vetted entity names")?;
    Ok(rows
        .into_iter()
        .map(|(entity_type, entity_id, name)| format!("{name} ({entity_type} {entity_id})"))
        .collect())
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

async fn persist_success(
    hx: &Harness,
    article_id: i64,
    fetched: &FetchedArticle,
    extracted_words: i32,
    content_hash: &str,
    evidence: &ArticleEvidence,
    model_version: &str,
) -> Result<()> {
    let evidence_json = json!({
        "output_contract_version": ARTICLE_READ_OUTPUT_CONTRACT_VERSION,
        "source_language": evidence.source_language,
        "evidence_blurb": evidence.evidence_blurb,
        "key_facts": evidence.key_facts,
        "relevant_entities": evidence.relevant_entities,
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
            $1, 'success', $2, $3, $4, $5,
            $6, $7::jsonb, $8, $9, 'parsed',
            NULL, NOW(), NOW()
        )
        ON CONFLICT (article_id) DO UPDATE SET
            status = 'success',
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
    .with_context(|| format!("persist article_read success {article_id}"))?;
    insert_data_fetch_ledger_best_effort(
        hx,
        article_id,
        "success",
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
          AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours')
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
    fn parser_accepts_compact_evidence_card() {
        let raw = r#"{"source_language":"DE","evidence_blurb":"  Player X returned to training.  ","key_facts":[" one ",""],"relevant_entities":["Club"],"story_type":" injury ","caveats":""}"#;
        let parsed = ArticleEvidenceParser.parse(raw).unwrap().unwrap();
        assert_eq!(parsed.source_language, "de");
        assert_eq!(parsed.evidence_blurb, "Player X returned to training.");
        assert_eq!(parsed.key_facts, vec!["one"]);
        assert_eq!(parsed.story_type, "injury");
    }

    #[test]
    fn parser_fails_closed_on_empty_blurb() {
        let parsed = ArticleEvidenceParser
            .parse(r#"{"evidence_blurb":" ","key_facts":[],"relevant_entities":[],"story_type":"general","caveats":""}"#)
            .unwrap();
        assert!(parsed.is_none());
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
}
