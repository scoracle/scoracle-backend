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

// The Reader's prompt and contract version live in `prompt.rs` — one file per junction, so a
// change to what this character is asked is a one-file diff. Re-exported here so call sites and
// the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{
    build_article_read_prompt, build_article_read_prompt_parts, ARTICLE_READ_PROMPT_VERSION,
};
pub const ARTICLE_READ_OUTPUT_CONTRACT_VERSION: &str = "article-reading-v3";
const ARTICLE_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const ARTICLE_MIN_WORDS: usize = 80;
pub(crate) const ARTICLE_MAX_MODEL_CHARS: usize = 9_000;
const ARTICLE_MAX_CO_MENTION_CANDIDATES: usize = 24;
const ARTICLE_NUM_PREDICT: i32 = 900;
/// The context size the LOCAL gemma3:4b runner is loaded with. `graph` deliberately sends this
/// same value (see `junctions/graph/mod.rs`): both stages share one local runner, and ollama
/// reloads the runner whenever a request asks for a different `num_ctx`. Two sizes here cost a
/// pair of reloads every rotation. Change both or neither.
pub(crate) const ARTICLE_NUM_CTX: i32 = 8192;
const ARTICLE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; ScoracleBot/1.0; +https://scoracle.com)";
const GOOGLE_NEWS_BATCH_URL: &str =
    "https://news.google.com/_/DotsSplashUi/data/batchexecute?rpcids=Fbv4je";

pub const ARTICLE_READ_SYSTEM_PROMPT: &str = r#"Task: decide whether one fetched sports article is genuinely about the known vetted entities, then compress what it says for The Journalist.

This article reached you because an RSS headline matched an entity name. That match is a GUESS and nothing has checked it. Deciding whether the FULL TEXT is really about the entity is your first job; the evidence card is your second.

Do this by CLASSIFYING the page first, then reading the verdict off that classification. Answer in the order the fields are listed: story_type first, then what the text says (key_facts, relevant_entities, co_mentions), then the card (evidence_blurb), and the `relevant` verdict LAST.

STEP 1 — choose exactly one story_type. Five of these values mean the page is not reporting on the entity:
- score_stub — a result/boxscore/live-score page: a score with lineups, stats tables, possession, cards, attendance, next fixtures. A result table is not reporting.
- broadcast_listing — a "watch"/"how to watch"/TV-times/streaming/replay page, or a schedule blurb.
- opponent_only — the vetted entity appears only as the opposition, or only in passing, while the story is about someone else.
- name_collision — a different club, age level, or competition sharing the name (youth, academy, reserves, women's, flag football, or a same-named club elsewhere). That is NOT the vetted entity.
- off_entity — anything else not materially about a vetted entity: navigation, ads, sidebars, tags, comments, related-link roundups.
The remaining values mean the page IS reporting on the entity: transfer, injury, performance, fixture (a genuine match preview or match report with actual reporting), roster, contract, general.

STEP 2 — the verdict follows mechanically from STEP 1. Do not re-decide it:
- story_type in {score_stub, broadcast_listing, opponent_only, name_collision, off_entity} => relevant=false, ALWAYS.
- any other story_type => relevant=true.
When relevant=false, do not summarize the unrelated story; give a short evidence_blurb explaining the mismatch.

Other rules:
- Use only the article text and the known vetted entities.
- If co-mention candidates are provided, label every candidate exactly once in co_mentions. Mark a candidate relevant only when the full article materially discusses that candidate; reject name collisions, roundup artifacts, and people/teams merely mentioned in navigation, ads, sidebars, tags, comments, or unrelated links.
- Detect the source article language. Translate meaning into English before writing the evidence card.
- evidence_blurb, key_facts, story_type, and caveats must be English.
- Preserve names, teams, dates, injuries, transactions, quotes-as-claims, scores, and reported uncertainty.
- Do not invent context, implications, or sourcing.
- Keep the evidence_blurb dense and neutral: what happened, who is involved, where it stands, and why it matters.
- If the article is mostly boilerplate, say so in caveats.
- Keep proper names in their canonical/source spelling unless an English name is clearly canonical.

Return strict JSON only, with the keys in exactly this order:
{"source_language":"<ISO 639-1 language code or unknown>","story_type":"transfer|injury|performance|fixture|roster|contract|general|score_stub|broadcast_listing|opponent_only|name_collision|off_entity","key_facts":["<English fact>", "..."],"relevant_entities":["<name>", "..."],"co_mentions":[{"candidate":<number>,"relevant":<true|false>}],"caveats":"<short English caveat or empty string>","evidence_blurb":"<2-4 compact English sentences, or short mismatch reason when relevant=false>","relevant":<true|false>}"#;

/// The model budget for one article reading — the stage's options and `bin/eval`'s, from ONE
/// definition (the `graph_opts()` pattern), so a fixture can never be scored under options
/// production does not send. Temperature 0.2 live; the eval overrides it per case.
///
/// `format_schema` is what forces `relevant` to be present in every reply: it is in the schema's
/// `required` list, so the model cannot decline to render a verdict. Worth knowing, because
/// `ArticleEvidence::relevant` defaults to TRUE if the field is ever absent — fail-OPEN on the one
/// field that gates the whole news rail. The schema is the only thing standing in front of that.
pub fn article_read_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(ARTICLE_READ_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: ARTICLE_NUM_PREDICT,
        num_ctx: ARTICLE_NUM_CTX,
        json_mode: false,
        format_schema: Some(article_read_format_schema()),
    }
}

/// FIELD ORDER IS THE CONTRACT, not a style choice (ar4, 2026-07-26). Constrained decoding emits
/// required properties in the order given, so this ordering is what forces the model to
/// characterize the article — `story_type`, then the facts, then the card — BEFORE it renders the
/// `relevant` verdict, which now comes last.
///
/// Under ar3 `relevant` was FIRST: the model committed to the verdict as its opening token, with
/// nothing written to reason from, and the highest-prior continuation is `true` because most
/// articles genuinely are relevant. mistral:7b could carry the judgment internally and still open
/// correctly; gemma3:4b could not, and rubber-stamped 99.1% of articles. The proof it was ordering
/// rather than capability: gemma labelled boxscores and broadcast listings `story_type:"fixture"`
/// CORRECTLY — at field 7, long after the verdict was already locked in.
///
/// The literal template at the end of `ARTICLE_READ_SYSTEM_PROMPT` must match this order.
pub fn article_read_format_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "source_language": { "type": "string" },
            // GRAMMAR-ENFORCED vocabulary (ar5). Without the enum the model invented values —
            // ar4 measured it answering `story_type:"score"` and `"broadcast"`, adopting the
            // reject list's wording while emitting terms no rule could act on. Constrained
            // decoding now restricts it to exactly the classes the verdict maps from.
            "story_type": { "type": "string", "enum": [
                "transfer", "injury", "performance", "fixture", "roster", "contract", "general",
                "score_stub", "broadcast_listing", "opponent_only", "name_collision", "off_entity"
            ] },
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
            "caveats": { "type": "string" },
            "evidence_blurb": { "type": "string" },
            "relevant": { "type": "boolean" }
        },
        "required": ["source_language", "story_type", "key_facts", "relevant_entities", "co_mentions", "caveats", "evidence_blurb", "relevant"]
    })
}

#[derive(Debug)]
pub struct ArticleRow {
    pub(crate) url: String,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) duplicate_of: Option<i64>,
    pub(crate) vetted_count: i64,
}

#[derive(Debug)]
struct FetchedArticle {
    final_url: String,
    final_domain: Option<String>,
    text: String,
}

#[derive(Clone, Debug)]
pub struct ArticleReadEntities {
    pub(crate) vetted_names: Vec<String>,
    pub(crate) co_mentions: Vec<CoMentionCandidate>,
}

/// One co-mention candidate line in the prompt. Fields are `pub` so a fixture generator outside
/// the crate can render the live prompt via `build_article_read_prompt_parts` — it is prompt data
/// with no invariant, not state.
#[derive(Clone, Debug)]
pub struct CoMentionCandidate {
    pub number: i32,
    pub entity_type: String,
    pub entity_id: i32,
    pub name: String,
    pub nationality: String,
    pub current_club: String,
    pub position: String,
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

/// The `story_type` values that MEAN "not reporting on the vetted entity". `relevant` is derived
/// from these rather than trusted from the model — see [`ArticleEvidenceParser`].
pub const REJECT_STORY_TYPES: &[&str] = &[
    "score_stub",
    "broadcast_listing",
    "opponent_only",
    "name_collision",
    "off_entity",
    "irrelevant",
];

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
        // DERIVE the verdict from the classification instead of trusting the model's boolean.
        //
        // Earned the hard way over three measured prompt revisions (ar3→ar5), each of which fixed
        // a real defect and moved the fixture score by exactly zero. Under ar5 gemma3:4b labels a
        // boxscore `score_stub` and a broadcast page `broadcast_listing` — the precise reject
        // classes — with the mapping stated as a mechanical lookup directly above, and the
        // classification emitted BEFORE the verdict. It still answered `relevant:true` every time.
        // The classification is reliable; the boolean is not; so the boolean stops being an input.
        //
        // Deliberately ONE-WAY. A reject class forces `relevant=false`, but a reporting class
        // never forces `true` — if the model ever does reject on its own judgment, that judgment
        // still stands. We are correcting a measured failure to say no, not overriding a no.
        if REJECT_STORY_TYPES
            .iter()
            .any(|t| evidence.story_type.eq_ignore_ascii_case(t))
        {
            evidence.relevant = false;
        }
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

    /// A small batch, unlike scrub's 256: this IS a model stage, so a large batch would starve the
    /// product stages behind it in the rotation. But at one item per rotation the Reader spent more
    /// wall clock waiting its turn than working — ~107s per article against ~50s of its own decode
    /// — and a per-junction model choice would pay a cold model load (measured 17.9s) on every
    /// single article. Eight amortizes both while keeping the rotation responsive.
    fn rotation_batch(&self) -> i64 {
        8
    }

    /// Two slots — the other half of Archbox's 4. See the note on graph's `max_in_flight`.
    fn max_in_flight(&self) -> usize {
        2
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
        let opts = article_read_opts();
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
        // graph runs HERE, not off the vetted trigger (mig 193). Reaching this line means the body
        // was fetched, the model read it, and it did NOT come back `irrelevant` — so graph gets the
        // summary as evidence, and never spends a call on a duplicate, an unreadable article, or one
        // the reader overturned. Every other terminal path above returns before this point on
        // purpose.
        enqueue_graph_for_article(hx, article_id, &item.sport).await?;
        enqueue_narratives_for_article(hx, article_id).await?;
        Ok(())
    }
}

/// enqueue_graph_for_article queues typed extraction for an article whose reading succeeded.
/// `input_version` carries the content hash so a re-read of changed content re-triggers extraction,
/// while a re-run over identical content does not.
async fn enqueue_graph_for_article(hx: &Harness, article_id: i64, sport: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'graph', 'article', $1::integer, $2, 'pending',
               'g:' || r.content_hash, NOW(), NOW()
          FROM public.news_article_readings r
         WHERE r.article_id = $1
           AND r.status = 'success'
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status        = 'pending',
            attempts      = 0,
            available_at  = NOW(),
            updated_at    = NOW(),
            last_error    = NULL,
            input_version = EXCLUDED.input_version
        WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
           OR public.pipeline_work.status = 'failed'
        "#,
    )
    .bind(article_id)
    .bind(sport)
    .execute(&hx.pool)
    .await
    .with_context(|| format!("enqueue graph for article {article_id}"))?;
    Ok(())
}

/// build_article_read_prompt_for_eval assembles the EXACT production user prompt for one article
/// — the same DB load, the same fetch, the same builder the stage calls — so `bin/eval` scores the
/// contract that actually runs rather than a reconstruction of it.
///
/// `Ok(None)` mirrors every path where the stage writes a terminal marker WITHOUT a model call
/// (article missing, duplicate, no vetted entities, body too short or paywalled). Those are
/// deterministic bookkeeping, not judgments, so there is nothing for a model to be scored on.
///
/// It fetches over the network, exactly as the stage does. That is deliberate: an eval that read
/// title+description from the DB would grade a prompt the Reader never sends, and the Reader's
/// whole job is judging the FULL text.
pub(crate) async fn build_article_read_prompt_for_eval(
    pool: &sqlx::PgPool,
    article_id: i64,
    sport: &str,
) -> Result<Option<String>> {
    let Some(article) = load_article(pool, article_id, sport).await? else {
        return Ok(None);
    };
    if article.duplicate_of.is_some() || article.vetted_count == 0 {
        return Ok(None);
    }
    let fetched = fetch_article(&article.url).await?;
    if count_words(&fetched.text) < ARTICLE_MIN_WORDS {
        return Ok(None);
    }
    let entities = load_article_read_entities(pool, article_id, sport).await?;
    Ok(Some(build_article_read_prompt(
        &article,
        &fetched.text,
        &entities,
    )))
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
mod tests;
