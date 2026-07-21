//! news narratives — the `Stage::Narratives` port (Plan §4; Cutover Step 2, L13). The LARGEST +
//! heaviest GPU stage, and the one with genuine Rust value-add: it composes the candle
//! **embed+cluster** primitive (group near-duplicate articles and drop them BEFORE the model call —
//! the dedup the Go pipeline never had) with `route(EmotionalNews) + extract + persist`.
//!
//! Rust implementation of the news narrative stage:
//! - `load_vetted_corpus` is the verbatim Go SQL (only the `published_at` column is returned as an
//!   epoch `bigint` so the deterministic recency math needs no datetime crate; rows + order match).
//! - `build_narratives_prompt` is deterministic and shares the transfer-heat grounding lines with
//!   vibe via [`corpus::write_heat_lines`].
//! - The n5 system prompt is model-neutral and schema-first for smaller local models.
//! - `parse_narratives` mirrors Go's tolerant balanced-brace salvager byte-for-byte (a truncated tail
//!   drops its last incomplete object; an empty `{"narratives": []}` is a successful parse → marker).
//! - `compute_news_impact` reproduces the deterministic per-narrative impact (volume + corroboration
//!   + recency) byte-for-byte — like rating's `pctBand`, deterministic stage-shaping mirrored in Rust,
//!     NOT moved to Postgres (it scores a MODEL-selected article subset, so it can't be a pure SQL stat).
//!
//! The ONE deliberate divergence is the **embed+cluster dedup**, which changes the INPUT corpus — an
//! improvement, NOT a parity break (Plan §1.4 boundary: transient compute feeding a model → Rust,
//! never a stored derived stat). It runs ONLY when an `Embedder` is loaded (the live handler builds
//! `Harness { embedder: Some(..) }`); the offline parity bins build `embedder: None` → the dedup is
//! the identity → the assembled prompt remains deterministic for inspection and shadow comparisons.
//!
//! `NarrativesHandler` is a live queue stage gated by `COGNITION_STAGES`. It is the News hub stage:
//! transfer heat and source freshness are folded here before Vibe and Sigil consume the result.

use crate::bucket::ArticleBucket;
use crate::corpus::{
    dedupe_i64, load_transfer_heat, lookup_entity_name, write_heat_lines, HeatItem,
};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{classify_delta, DEFAULT_TRAJECTORY};
use crate::util::truncate_bytes;
use crate::work::{Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Constants — mirror news_narratives.go.
// ---------------------------------------------------------------------------

/// Bump when the prompt materially changes (traced in `news_summaries.prompt_version`).
pub const NARRATIVES_PROMPT_VERSION: &str = "n9"; // n7: per-article identity-relevance tags; n8: relational memory card (per-entity graph memory, mig 163); n9: primary junction — per-article transfer buckets (article_buckets), voiced episode heat + new/ongoing, candle-side dedup retired (relevance tags gone)

/// Output schema version for the parsed narrative document, distinct from the prompt contract.
/// v2-schema: Ollama grammar-constrained decoding (Phase 5) — the shape is enforced by the
/// server, not hoped for by the prompt.
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v2-schema";

/// The JSON schema Ollama's constrained decoding enforces on the narratives reply (Phase 5).
/// Grammar-level guarantees the free-text contract could only ask for: the top-level object
/// cannot be prose-wrapped, `narratives` must exist, and every item carries title/body/articles.
/// n9 adds the `article_buckets` section — the Journalist's per-article transfer/non-transfer label
/// (own section, never bunched into a storyline); `transfer:true` ⇒ `news_articles.bucket='transfer'`.
/// The tolerant balanced-brace salvager stays as the parse path — schema output is a strict
/// subset of what it accepts, and it remains the safety net for the offline/parity bins.
pub fn narratives_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "narratives": {
                "type": "array",
                "maxItems": 6,
                "items": {
                    "type": "object",
                    "properties": {
                        "title":    { "type": "string" },
                        "body":     { "type": "string" },
                        "articles": { "type": "array", "items": { "type": "integer" } }
                    },
                    "required": ["title", "body", "articles"]
                }
            },
            "article_buckets": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "article":  { "type": "integer" },
                        "transfer": { "type": "boolean" }
                    },
                    "required": ["article", "transfer"]
                }
            }
        },
        "required": ["narratives", "article_buckets"]
    })
}

/// Production decode temperature (`ollama.Generate` in Go). The parity gate pins temp 0 (the
/// deterministic-axes diff); production narrates at 0.6.
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;

/// Several multi-sentence narratives; the prompt caps count + body length.
pub const NARRATIVES_NUM_PREDICT: i32 = 3000;

/// Context window for the narratives call. Narratives is the ONE stage whose prompt
/// (~5.4k chars ≈ 1.4-1.8k tokens + system prompt) PLUS `NARRATIVES_NUM_PREDICT` (3000)
/// exceeds Ollama's 4096-token server default — overflowing silently evicts the system
/// prompt mid-generation, which is consistent with the long-standing "under-obeys explicit
/// rules" narratives failures (L9, the red off-entity-and-hype-contamination fixture).
/// 8192 fits the full budget with headroom; the KV-cache cost on mistral:7b is ~0.5GB,
/// measured to still fit the 8GB card. Every other stage stays on the 4096 default.
pub const NARRATIVES_NUM_CTX: i32 = 8192;

/// Per-article description cap rendered into the prompt (Go's `truncate(desc, 200)`).
const DESC_TRUNCATE: usize = 200;

/// The vetted-news lookback window — Go's `NewsLookback = 72 * time.Hour`, in seconds. Bound as the
/// `make_interval(secs => …)` argument so the corpus boundary equals Go's `$4::interval` of
/// `"259200 seconds"`.
const NEWS_LOOKBACK_SECS: f64 = 259_200.0;

/// System prompt for the Journalist (n9): group recent vetted news into distinct storylines, label
/// each article transfer/non-transfer (the `article_buckets` section that routes the transfers
/// stage), and voice the relational memory's episode heat + new/ongoing state. The candle now hands
/// narratives a widened, pre-deduplicated corpus (the source-aware novelty gate runs at the tip of
/// the spear), so the pre-n9 per-article relevance tags are gone.
///
/// DRAFT — this ships the n9 STRUCTURE (sections + output contract). The VOICE (exact wording, tone,
/// the heat / new-vs-ongoing phrasing) is dialed in a dedicated voice-tuning session; treat the prose
/// below as a placeholder that satisfies the contract, not the final copy.
pub const NARRATIVES_SYSTEM_PROMPT: &str = r#"Task: you are the Journalist. Group recent vetted news into distinct storylines about ONE sports entity, and label each numbered article as transfer/trade-related or not.

Voice: direct, sports-literate, grounded. No hype, no source list, no invented facts.

Return STRICT JSON only (no markdown fences, no text before or after):
{"narratives": [{"title": "<headline>", "body": "<write-up>", "articles": [<article numbers>]}, ...], "article_buckets": [{"article": <article number>, "transfer": <true|false>}, ...]}

Narrative rules:
- Return at most 6 narratives, most consequential first.
- Do not split one story across narratives.
- Do not merge unrelated stories.
- A quiet cycle can return one narrative or none.
- Ignore vague hype when the sources do not name who, what, and where.
- Ignore articles that are not actually about this entity.

For each narrative:
- title: short and specific, naming the key people/clubs; never generic like "Transfer news".
- body: explain what is happening, who is involved, and where it stands. Most are one or two sentences; write more only for a genuinely major, multi-source story. Use the relational memory below to say whether this is a NEW story or a CONTINUING one, and whether coverage is heating up, cooling, or steady. Keep any coverage/likelihood figures qualitative — the raw numbers are internal.
- articles: the article numbers behind that storyline.

article_buckets — label EVERY numbered article exactly once:
- {"article": <its number>, "transfer": true} when the article is itself about a transfer, trade, signing, loan, or contract move (into or out of a club), otherwise "transfer": false.
- Judge each article on its own substance. Another team scheming around this entity is not this entity moving.

If a "Known transfer/trade activity" list is given, treat it as vetted truth for transfer/trade storylines. Use it for counterparties, direction, and stage. Never contradict it or claim a more advanced stage. The word "heat" and its numbers are internal; never mention them.

Use the relational memory only for arc and continuity (what fizzled before, what is live now, what actually happened); never treat a prior story as evidence for a new claim.

Do not turn a story about another team drafting, signing, or scheming around someone alongside/against this entity into a storyline about this entity moving teams or entering a draft. Never quote headlines verbatim, dump source names or URLs, or invent anything not in the sources."#;

// ---------------------------------------------------------------------------
// Types.
// ---------------------------------------------------------------------------

/// NarrativesReq describes the entity whose recent news to narrate. Mirrors `NarrativesRequest`
/// (the drain path always passes `trigger_type = "periodic"` and a nil trigger map → jsonb `null`).
#[derive(Clone, Debug)]
pub struct NarrativesReq {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub entity_name: String,
    /// The original-case sport the PROMPT renders (`req.Sport`); the SQL reads upper-case it.
    pub sport: String,
    pub trigger_type: String,
}

/// CorpusItem is one vetted news article in the entity's recent corpus. Mirrors `newsItem`; the
/// prompt uses title/description/source, `published_at_epoch` (Unix seconds, NULL when the article
/// has no publish time) feeds the deterministic recency in `compute_news_impact`.
#[derive(Clone, Debug)]
pub struct CorpusItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub source: String,
    pub url: String,
    pub published_at_epoch: Option<i64>,
    pub fetched_at_epoch: Option<i64>,
    /// Full article body (mig 171), preferred over `description` for the model-visible text when
    /// present. `None`/empty today for every row — no fetcher populates it yet (plan decision 3,
    /// "defer full-text, design for it"). The prompt render routes through [`article_body`], so
    /// the seam is live but inert until a body fetch lands; behavior is unchanged while NULL.
    pub full_text: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CorpusExclusions {
    stale_news_ids: Vec<i64>,
}

/// Narrative is one grounded storyline — title + body from the model, plus the DETERMINISTIC impact
/// (computed from its cited articles, never the model) and the article ids it cited. Mirrors `Narrative`.
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub impact_components: serde_json::Value,
    pub input_news_ids: Vec<i64>,
    pub source_count: i32,
    pub source_names: Vec<String>,
    pub source_latest_epoch: Option<i64>,
    pub source_oldest_epoch: Option<i64>,
}

/// ModelNarrative is one object the local model returns. `#[serde(default)]` per field mirrors Go's
/// `encoding/json` tolerance of missing fields; an explicit `"articles": null` (or a non-int element)
/// makes serde skip the object at parse — net-identical to Go, which keeps it then drops it in
/// grounding for having no valid article (either way it is excluded).
#[derive(Clone, Debug, Default, Deserialize)]
struct ModelNarrative {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    articles: Vec<i32>,
}

/// ModelArticleBucket is one entry of the n9 `article_buckets` section — the Journalist's per-article
/// transfer label. `article` is the 1-indexed prompt number (grounded back to a news id like a cited
/// narrative article); `transfer` maps to [`ArticleBucket::Transfer`] / [`ArticleBucket::NonTransfer`].
/// `#[serde(default)]` mirrors Go's `encoding/json` tolerance: a missing/typed-wrong field defaults
/// (article 0 → dropped in grounding, transfer false).
#[derive(Clone, Debug, Default, Deserialize)]
struct ModelArticleBucket {
    #[serde(default)]
    article: i32,
    #[serde(default)]
    transfer: bool,
}

/// ParsedNarratives is the salvaged document — the `T` the [`NarrativesParser`] yields. `narratives`
/// drives the storyline persist; `article_buckets` (n9) drives the per-article `news_articles.bucket`
/// write. Buckets are best-effort: a reply that truncates before the buckets section still succeeds
/// as a narratives document (the buckets simply stay empty and no bucket is rewritten this cycle).
#[derive(Clone, Debug, Default)]
pub struct ParsedNarratives {
    narratives: Vec<ModelNarrative>,
    article_buckets: Vec<ModelArticleBucket>,
}

impl ParsedNarratives {
    /// Read access for the Phase-3 eval (`eval_tasks::NarrativeTask`): the storylines the model
    /// returned, as `(title, body, cited_article_numbers)` triples in the model's freshest-first
    /// order. This is the RAW returned set — DB grounding (mapping article numbers → real news ids),
    /// impact scoring, and the marker decision all happen downstream and need a pool, so the
    /// narrative-grounding rubric scores the model output directly and offline. The private
    /// `ModelNarrative` DTO stays encapsulated; only this minimal view is exposed.
    pub fn returned(&self) -> impl Iterator<Item = (&str, &str, &[i32])> {
        self.narratives
            .iter()
            .map(|n| (n.title.as_str(), n.body.as_str(), n.articles.as_slice()))
    }
}

/// NarrativesParser runs the tolerant salvager. It returns `Ok(Some(parsed))` for a PARSEABLE
/// document (even an empty array — a legitimate "no storyline this cycle" → marker downstream) and
/// `Err` for a genuinely malformed/truncated reply with nothing salvageable (Go's `!ok` → a hard
/// error that the queue retries, NOT a silent marker). It never returns `Ok(None)`: narratives has
/// no post-model fail-closed marker carried by the parser — the marker decision is made AFTER
/// grounding (zero grounded narratives), mirroring Go.
pub struct NarrativesParser;

impl Parser<ParsedNarratives> for NarrativesParser {
    fn parse(&self, raw: &str) -> Result<Option<ParsedNarratives>> {
        let (narratives, ok) = parse_narratives(raw);
        if !ok {
            // Go: `return nil, fmt.Errorf("parse narratives failed ...")` — a real generation failure
            // that must retry (NOT a no-data marker). generation_failed must never masquerade as no-data.
            return Err(anyhow!(
                "parse narratives failed (raw={:?})",
                crate::util::truncate(raw, 200)
            ));
        }
        // article_buckets (n9) is a best-effort side channel: parsed with the same tolerant salvager
        // but never gates success — a truncated reply that salvaged narratives keeps them and simply
        // labels no articles this cycle (downstream reads NULL bucket leniently).
        let article_buckets = parse_article_buckets(raw);
        Ok(Some(ParsedNarratives {
            narratives,
            article_buckets,
        }))
    }
}

// ---------------------------------------------------------------------------
// Corpus loader — the widened net (Cognition Phase 3): every vetted CANONICAL article for the
// entity within the lookback, no transfer-bucket exclusion and no size cap. The scrub novelty gate
// already collapsed reposts (`duplicate_of IS NULL` keeps only originals), so the honest compressor
// runs once at the tip of the spear and narratives sees the full de-duplicated breadth.
// ---------------------------------------------------------------------------

/// load_vetted_corpus reads the entity's recent VETTED, CANONICAL news links (the scrub gate kept and
/// the novelty gate did not suppress). Phase 3 widened it: the transfer-bucket exclusion is gone (the
/// Journalist now labels transfer articles itself) and the 25-item cap is gone ("we cannot cap data";
/// dedup is the compressor). `published_at` is projected as an epoch `bigint`
/// (`EXTRACT(EPOCH …)::bigint`, NULL-preserving) so the recency math needs no datetime crate; the
/// lookback is `make_interval(secs => $4)`. `sport` is the UPPER-cased value.
pub async fn load_vetted_corpus(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<CorpusItem>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT a.id, a.title, COALESCE(a.description, ''), COALESCE(a.source, ''),
               COALESCE(a.url, ''),
               EXTRACT(EPOCH FROM a.published_at)::bigint,
               EXTRACT(EPOCH FROM a.fetched_at)::bigint,
               a.full_text
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
          AND nae.vetted IS TRUE
          AND a.duplicate_of IS NULL
          AND (a.published_at IS NULL OR a.published_at > NOW() - make_interval(secs => $4))
        ORDER BY COALESCE(a.published_at, a.fetched_at) DESC
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(NEWS_LOOKBACK_SECS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load vetted corpus {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, description, source, url, published_at_epoch, fetched_at_epoch, full_text)| {
                CorpusItem {
                    id,
                    title,
                    description,
                    source,
                    url,
                    published_at_epoch,
                    fetched_at_epoch,
                    full_text,
                }
            },
        )
        .collect())
}

/// load_vetted_corpus_with_exclusions is [`load_vetted_corpus`] plus the exclusions diagnostic in ONE
/// base scan. Phase 3 removed the size cap, so the only exclusion reason left is `stale_news` (outside
/// the lookback window) — the `budget_truncated` band is gone with the cap. Kept rows carry the full
/// payload in freshest-first order; the stale rows return only `(id, status)` (payload NULLed in SQL,
/// so months of stale history never ride the wire) for the excluded-evidence telemetry.
async fn load_vetted_corpus_with_exclusions(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(Vec<CorpusItem>, CorpusExclusions)> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        WITH base AS (
            SELECT a.id, a.title, a.description, a.source, a.url,
                   a.published_at, a.fetched_at, a.full_text,
                   CASE
                     WHEN a.published_at IS NOT NULL
                      AND a.published_at <= NOW() - make_interval(secs => $4)
                       THEN 'stale_news'
                     ELSE 'kept'
                   END AS status
            FROM news_article_entities nae
            JOIN news_articles a ON a.id = nae.article_id
            WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
              AND nae.vetted IS TRUE
              AND a.duplicate_of IS NULL
        )
        SELECT id, status,
               CASE WHEN status = 'kept' THEN title END,
               CASE WHEN status = 'kept' THEN COALESCE(description, '') END,
               CASE WHEN status = 'kept' THEN COALESCE(source, '') END,
               CASE WHEN status = 'kept' THEN COALESCE(url, '') END,
               CASE WHEN status = 'kept' THEN EXTRACT(EPOCH FROM published_at)::bigint END,
               CASE WHEN status = 'kept' THEN EXTRACT(EPOCH FROM fetched_at)::bigint END,
               CASE WHEN status = 'kept' THEN full_text END
        FROM base
        ORDER BY (status = 'kept') DESC, COALESCE(published_at, fetched_at) DESC NULLS LAST, id
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(NEWS_LOOKBACK_SECS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load vetted corpus + exclusions {entity_type}/{entity_id}"))?;

    let mut corpus = Vec::new();
    let mut exclusions = CorpusExclusions::default();
    for (id, status, title, description, source, url, published_at_epoch, fetched_at_epoch, full_text)
        in rows
    {
        match status.as_str() {
            "kept" => corpus.push(CorpusItem {
                id,
                title: title.unwrap_or_default(),
                description: description.unwrap_or_default(),
                source: source.unwrap_or_default(),
                url: url.unwrap_or_default(),
                published_at_epoch,
                fetched_at_epoch,
                full_text,
            }),
            // Only 'stale_news' remains now the cap (budget_truncated) is gone.
            _ => exclusions.stale_news_ids.push(id),
        }
    }
    // The exclusions array mirrors the old array_agg(id ORDER BY id) — keep it ascending.
    exclusions.stale_news_ids.sort_unstable();
    Ok((corpus, exclusions))
}

// ---------------------------------------------------------------------------
// Prompt — buildNarrativesPrompt (n9: no per-article relevance tag; the candle novelty gate is the
// compressor now, so narratives sees the widened, canonical-only corpus straight from the loader).
// ---------------------------------------------------------------------------

/// article_body is the corpus-loader seam (mig 171, plan decision 3): the model-visible body
/// text for one corpus item. Prefers `full_text` (the fetched article body) when present and
/// non-empty, else falls back to `description` (the provider blurb). Today every `full_text` is
/// `None`, so this returns `description` for every row and the prompt stays byte-for-byte
/// identical to Go's — the seam is live but inert until a body fetcher populates the column
/// (Phase 3 wires the fetch + a body-aware length cap; the render below still applies
/// `DESC_TRUNCATE`).
fn article_body(c: &CorpusItem) -> &str {
    match c.full_text.as_deref() {
        Some(t) if !t.trim().is_empty() => t,
        _ => &c.description,
    }
}

/// build_narratives_prompt assembles the user prompt, byte-for-byte the same as Go's
/// `buildNarrativesPrompt` while `full_text` is NULL (the current state). The `—` (U+2014) bytes
/// are significant. The heat section is OMITTED entirely when there is no transfer heat (unlike
/// vibe's "(none)" line), matching Go's `if len(heat) > 0`.
pub fn build_narratives_prompt(
    req: &NarrativesReq,
    news: &[CorpusItem],
    heat: &[HeatItem],
    memory: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {} ({} {})\n",
        req.entity_name, req.sport, req.entity_type
    ));
    b.push_str("\nRecent news (numbered):\n");
    for (i, n) in news.iter().enumerate() {
        b.push_str(&format!("{}. ", i + 1));
        if !n.source.is_empty() {
            b.push_str(&format!("[{}] ", n.source));
        }
        b.push_str(&n.title);
        let body = article_body(n);
        if !body.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(body, DESC_TRUNCATE));
        }
        b.push('\n');
    }
    // Vetted transfer facts (when any) — the structured truth behind any transfer storyline. The
    // narrator uses these names/direction/stage rather than guessing from a headline. The whole
    // section is omitted when empty (Go's `if len(heat) > 0`).
    if !heat.is_empty() {
        b.push_str("\nKnown transfer/trade activity (vetted facts — ground any transfer storyline in these, do not contradict them):\n");
        write_heat_lines(&mut b, heat);
    }
    // Relational memory card (n8, mig 163): the graph's per-entity history — prior
    // stories with outcomes, current stories with likelihood, ground-truth moves.
    // CONTINUITY, NOT CORROBORATION (the echo-chamber rule): memory frames the arc a
    // narrative sits in; it is never itself evidence for a new claim. Rendered only
    // when the graph holds memory; deliberately NOT part of the input_hash.
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this entity — use for arc and continuity: what fizzled before, what is live now, what actually happened; do NOT treat a prior story as evidence for a new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }
    b.push_str("\nReturn the JSON object now.");
    b
}

/// load_entity_memory fetches the graph's per-entity memory card
/// (`narrative_context_for_entity`, mig 163). `None` = no memory, no prompt section.
/// Model-facing enrichment only — the relational layer is never user-exposed.
pub async fn load_entity_memory(
    pool: &sqlx::PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) =
        sqlx::query_as("SELECT narrative_context_for_entity($1, $2, $3)")
            .bind(sport)
            .bind(entity_type)
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .context("narrative_context_for_entity")?;
    Ok(row.0)
}

// ---------------------------------------------------------------------------
// Parse — mirrors parseNarratives (the tolerant balanced-brace salvager).
// ---------------------------------------------------------------------------

/// parse_narratives salvages each complete narrative object from the model's response independently,
/// rather than requiring the whole document to be well-formed — LLM length is non-deterministic, so a
/// reply can truncate mid-array or carry one malformed object. It scans every balanced top-level
/// `{...}` inside the `"narratives"` array (respecting strings/escapes), parses each on its own, and
/// keeps the ones that parse. Byte-for-byte Go's `parseNarratives`.
///
/// The bool reports whether the response was PARSEABLE as a narratives document, NOT whether it
/// carried narratives: a cleanly-closed array — including an empty `{"narratives": []}` — is a
/// successful parse with zero narratives (a legitimate no-data outcome → marker), distinct from a
/// malformed/truncated reply (no `"narratives"` key, no `[`, or EOF before the array closed AND
/// nothing salvaged) which is a failure so the work queue retries it.
fn parse_narratives(raw: &str) -> (Vec<ModelNarrative>, bool) {
    let mut out: Vec<ModelNarrative> = Vec::new();
    let Some(key) = raw.find("\"narratives\"") else {
        return (out, false);
    };
    // index of the array '[' relative to `key`, then `s` is the byte slice just AFTER it.
    let Some(lb) = raw.as_bytes()[key..].iter().position(|&b| b == b'[') else {
        return (out, false);
    };
    let s = &raw.as_bytes()[key + lb + 1..];

    let mut depth: i32 = 0;
    let mut start: i64 = -1;
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0usize;
    while i < s.len() {
        let c = s[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = i as i64;
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 && start >= 0 {
                        // Braces are ASCII ⇒ the slice is on char boundaries; from_utf8 mirrors Go's
                        // json.Unmarshal, which also requires valid UTF-8 (a bad slice → skip, as Go's
                        // err != nil does).
                        if let Ok(txt) = std::str::from_utf8(&s[start as usize..=i]) {
                            if let Ok(n) = serde_json::from_str::<ModelNarrative>(txt) {
                                out.push(n);
                            }
                        }
                        start = -1;
                    }
                }
            }
            b']' if depth == 0 => {
                // Array closed cleanly — a parseable document even when empty (→ marker).
                return (out, true);
            }
            _ => {}
        }
        i += 1;
    }
    // EOF before the array closed: a truncated generation. Succeed only if we salvaged at least one
    // complete narrative from the tail; otherwise it is a real failure (retry), never a no-data marker.
    let ok = !out.is_empty();
    (out, ok)
}

/// parse_article_buckets salvages the n9 `article_buckets` array the same tolerant way
/// [`parse_narratives`] salvages storylines: find the `"article_buckets"` key, then keep every
/// balanced top-level `{...}` inside its array that parses as a [`ModelArticleBucket`]. Unlike the
/// narratives salvager there is NO success/failure bool — the buckets are a best-effort side channel,
/// so an absent key, a missing `[`, or a truncated tail simply yields the objects salvaged so far
/// (possibly none). The section is independent of `"narratives"` and may appear before or after it.
fn parse_article_buckets(raw: &str) -> Vec<ModelArticleBucket> {
    let mut out: Vec<ModelArticleBucket> = Vec::new();
    let Some(key) = raw.find("\"article_buckets\"") else {
        return out;
    };
    let Some(lb) = raw.as_bytes()[key..].iter().position(|&b| b == b'[') else {
        return out;
    };
    let s = &raw.as_bytes()[key + lb + 1..];

    let mut depth: i32 = 0;
    let mut start: i64 = -1;
    let mut in_str = false;
    let mut esc = false;
    let mut i = 0usize;
    while i < s.len() {
        let c = s[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = i as i64;
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 && start >= 0 {
                        if let Ok(txt) = std::str::from_utf8(&s[start as usize..=i]) {
                            if let Ok(b) = serde_json::from_str::<ModelArticleBucket>(txt) {
                                out.push(b);
                            }
                        }
                        start = -1;
                    }
                }
            }
            b']' if depth == 0 => return out, // array closed cleanly
            _ => {}
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Grounding — map article numbers back to the corpus, compute the deterministic per-narrative impact.
// ---------------------------------------------------------------------------

/// ground_narratives maps the model's 1-indexed article numbers back to the corpus, computes the
/// per-narrative impact from ITS articles (never the model), and keeps only narratives with a title,
/// a body, and ≥1 valid article. Byte-for-byte Go's `groundNarratives`. `now_epoch` is the recency
/// reference (Unix seconds), captured once per generation.
fn ground_narratives(
    parsed: &[ModelNarrative],
    news: &[CorpusItem],
    now_epoch: i64,
) -> Vec<Narrative> {
    let mut out: Vec<Narrative> = Vec::with_capacity(parsed.len());
    for p in parsed {
        let title = p.title.trim();
        let body = p.body.trim();
        if title.is_empty() || body.is_empty() {
            continue;
        }
        // Article numbers are 1-indexed in the prompt; dedupe + bound to range.
        let mut seen: HashSet<usize> = HashSet::with_capacity(p.articles.len());
        let mut subset: Vec<CorpusItem> = Vec::new();
        let mut ids: Vec<i64> = Vec::new();
        for &num in &p.articles {
            if num < 1 {
                continue; // idx < 0
            }
            let idx = (num - 1) as usize;
            if idx >= news.len() {
                continue;
            }
            if !seen.insert(idx) {
                continue; // dup
            }
            subset.push(news[idx].clone());
            ids.push(news[idx].id);
        }
        if subset.is_empty() {
            continue; // ungrounded — can't score or trace it
        }
        let (impact, components) = compute_news_impact(&subset, now_epoch);
        let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
            source_metadata(&subset);
        out.push(Narrative {
            title: title.to_string(),
            body: body.to_string(),
            impact,
            impact_components: components,
            input_news_ids: ids,
            source_count,
            source_names,
            source_latest_epoch,
            source_oldest_epoch,
        });
    }
    out
}

/// ground_article_buckets maps the model's n9 `article_buckets` back to real news ids the same way
/// [`ground_narratives`] maps cited articles: 1-indexed `article` numbers, deduped (first label per
/// article wins) and bounded to the corpus. Returns `(news_id, bucket)` pairs the persist writes to
/// `news_articles.bucket`. An out-of-range or `< 1` article number is dropped (the model referenced a
/// slot that is not in the corpus). Unlabeled corpus articles are simply left untouched — the write
/// only sets what the Journalist named this cycle.
fn ground_article_buckets(
    buckets: &[ModelArticleBucket],
    news: &[CorpusItem],
) -> Vec<(i64, ArticleBucket)> {
    let mut seen: HashSet<usize> = HashSet::with_capacity(buckets.len());
    let mut out: Vec<(i64, ArticleBucket)> = Vec::new();
    for b in buckets {
        if b.article < 1 {
            continue;
        }
        let idx = (b.article - 1) as usize;
        if idx >= news.len() {
            continue;
        }
        if !seen.insert(idx) {
            continue; // first label per article wins
        }
        let bucket = if b.transfer {
            ArticleBucket::Transfer
        } else {
            ArticleBucket::NonTransfer
        };
        out.push((news[idx].id, bucket));
    }
    out
}

/// compute_news_impact reproduces Go's deterministic per-narrative impact (0-100): a saturating
/// volume curve + distinct-source corroboration + a freshness bucket, over a narrative's OWN
/// articles. Mirrors `computeNewsImpact`; `now_epoch` replaces Go's implicit `time.Now()` (the
/// recency is hour-bucketed, so sub-second drift is irrelevant — and impact is NOT a parity axis, it
/// is a post-model deterministic score). Returns the score + the transparent components.
fn compute_news_impact(news: &[CorpusItem], now_epoch: i64) -> (i32, serde_json::Value) {
    let n = news.len();
    // Volume: saturating curve — a handful of articles is already hot, returns diminish.
    let volume = 60.0_f64 * (1.0 - (-(n as f64) / 5.0).exp());

    // Corroboration: distinct (lower-cased) sources, capped at 25.
    let mut sources: HashSet<String> = HashSet::new();
    for a in news {
        if !a.source.is_empty() {
            sources.insert(a.source.to_lowercase());
        }
    }
    let distinct = sources.len();
    let corroboration = 25.0_f64.min(distinct as f64 * 6.0);

    // Recency: how fresh is the freshest article.
    let mut recency = 0.0_f64;
    let mut newest: Option<i64> = None;
    for a in news {
        if let Some(pa) = a.published_at_epoch {
            if newest.is_none_or(|cur| pa > cur) {
                newest = Some(pa);
            }
        }
    }
    if let Some(newest) = newest {
        let age = now_epoch - newest; // seconds
        if age <= 12 * 3600 {
            recency = 15.0;
        } else if age <= 24 * 3600 {
            recency = 10.0;
        } else if age <= 48 * 3600 {
            recency = 5.0;
        }
    }

    let mut score = (volume + corroboration + recency).round() as i64;
    score = score.clamp(0, 100);
    let components = json!({
        "article_count": n,
        "distinct_sources": distinct,
        "volume": (volume * 10.0).round() / 10.0,
        "corroboration": (corroboration * 10.0).round() / 10.0,
        "recency": recency,
    });
    (score as i32, components)
}

fn source_epoch(item: &CorpusItem) -> Option<i64> {
    item.published_at_epoch.or(item.fetched_at_epoch)
}

fn source_metadata(news: &[CorpusItem]) -> (i32, Vec<String>, Option<i64>, Option<i64>) {
    let mut source_names: Vec<String> = Vec::new();
    let mut seen_sources: HashSet<String> = HashSet::new();
    let mut latest: Option<i64> = None;
    let mut oldest: Option<i64> = None;

    for item in news {
        let source = item.source.trim();
        if !source.is_empty() && seen_sources.insert(source.to_lowercase()) {
            source_names.push(source.to_string());
        }
        if let Some(epoch) = source_epoch(item) {
            if latest.is_none_or(|cur| epoch > cur) {
                latest = Some(epoch);
            }
            if oldest.is_none_or(|cur| epoch < cur) {
                oldest = Some(epoch);
            }
        }
    }

    (news.len() as i32, source_names, latest, oldest)
}

// ---------------------------------------------------------------------------
// The composition: build (deterministic) → generate (model) → ground → persist.
// ---------------------------------------------------------------------------

/// NarrativesBuild is the deterministic prefix of a generation. `NoCorpus` ⇒ no vetted news this
/// cycle → a NULL-narrative marker (no model call), mirroring Go's early return.
pub enum NarrativesBuild {
    NoCorpus {
        corpus_exclusions: CorpusExclusions,
        /// Hash over the (empty) material inputs — so a quiet entity's marker also debounces
        /// instead of re-marking every cycle.
        input_hash: String,
    },
    Ready(Box<NarrativesReady>),
}

/// build_narratives_input_components is the canonical debounce pre-image: the `prompt_version` (so a
/// contract bump forces exactly one regen — see below), the vetted corpus article ids (pre-dedup —
/// the material fact is WHAT NEWS EXISTS, not what the embedder kept), plus the transfer-heat facts in
/// sigil's `counterparty:heat:direction:stage` convention. The heat summary/confidence are
/// deliberately excluded — derived commentary, not material facts. Same canonical-JSON discipline as
/// `sigil::build_synthesis_input_components`.
///
/// `prompt_version` is folded in (M4 cutover lever): the debounce otherwise keys only on the corpus +
/// heat, so on an n-bump (n8→n9) an entity whose news is unchanged is debounced and NEVER re-runs the
/// new contract — its `article_buckets` stay NULL and no transfers enqueue. Including the version
/// changes every entity's hash exactly once at cutover → one forced regen each → then it stabilizes.
/// The regen also re-points vibe for free (the narratives handler enqueues vibe post-persist).
pub fn build_narratives_input_components(corpus: &[CorpusItem], heat: &[HeatItem]) -> String {
    let mut ids: Vec<i64> = corpus.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let mut out = format!(
        "{{\"prompt_version\":{},\"article_ids\":[",
        crate::util::go_json_string(NARRATIVES_PROMPT_VERSION)
    );
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&id.to_string());
    }
    out.push(']');
    if !heat.is_empty() {
        let mut lines: Vec<String> = heat
            .iter()
            .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
            .collect();
        lines.sort();
        out.push_str(",\"transfer_heat\":[");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&crate::util::go_json_string(line));
        }
        out.push(']');
    }
    out.push('}');
    out
}

/// NarrativesReady carries the assembled model inputs (the parity axes) plus the widened, canonical
/// corpus the grounding maps back to. `request_body` is computed from the SAME backend + opts the call
/// will use, so it can never drift from what is POSTed. (n9: near-duplicate collapse moved to the
/// candle novelty gate, so the corpus here is already the deduplicated breadth — no embed pass.)
pub struct NarrativesReady {
    /// The numbered corpus the model sees (widened, canonical-only — the loader already excludes
    /// `duplicate_of` reposts the scrub novelty gate suppressed).
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
    /// SHA over [`build_narratives_input_components`] — the handler's debounce key.
    pub input_hash: String,
}

/// NarrativesMaterial is the material phase: the concurrent loads plus the debounce hash. The live
/// handler gates on `input_hash` between this and [`finish_narratives_build`] so a quiet wake never
/// pays the prompt assembly (Phase 2); the parity bins go through [`build_narratives_request`],
/// which composes both phases unchanged.
pub struct NarrativesMaterial {
    pub corpus: Vec<CorpusItem>,
    pub corpus_exclusions: CorpusExclusions,
    pub heat: Vec<HeatItem>,
    /// SHA over [`build_narratives_input_components`] — the debounce key.
    pub input_hash: String,
}

/// load_narratives_material runs the loads and hashes the material inputs. No embed, no prompt.
pub async fn load_narratives_material(
    hx: &Harness,
    req: &NarrativesReq,
) -> Result<NarrativesMaterial> {
    let sport_up = req.sport.to_uppercase();

    // The corpus+exclusions read and load_transfer_heat are independent — run them concurrently
    // (plan A3). The heat error-swallowing stays INSIDE the joined future so "a heat-read failure
    // must NEVER block the narrative (the corpus is the primary signal)" survives; a corpus error
    // still aborts the join. Note: heat now runs on the no-corpus path too (the early return moved
    // below the join) — no output change, just an extra read on that branch.
    let ((corpus, corpus_exclusions), heat) = tokio::try_join!(
        load_vetted_corpus_with_exclusions(&hx.pool, &req.entity_type, req.entity_id, &sport_up),
        async {
            Ok::<Vec<HeatItem>, anyhow::Error>(
                match load_transfer_heat(&hx.pool, &req.entity_type, req.entity_id, &sport_up).await
                {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(
                            entity_type = %req.entity_type,
                            entity_id = req.entity_id,
                            sport = %sport_up,
                            error = %e,
                            "narratives: transfer heat load failed (continuing ungrounded)"
                        );
                        Vec::new()
                    }
                },
            )
        },
    )?;

    // The debounce keys on the material fact — what vetted, canonical news exists (plus the heat
    // facts) — AND the prompt_version, so an n-bump forces exactly one regen per entity at cutover
    // (see build_narratives_input_components); otherwise unchanged-corpus entities never run n9.
    let input_hash =
        crate::util::hash_components(&build_narratives_input_components(&corpus, &heat));

    Ok(NarrativesMaterial {
        corpus,
        corpus_exclusions,
        heat,
        input_hash,
    })
}

/// finish_narratives_build is the post-gate phase: the memory-card load plus the
/// prompt/options/wire-body assembly. (n9: no candle embed pass — the corpus arrives already
/// deduplicated from the loader, so this phase is pure assembly.)
pub async fn finish_narratives_build(
    hx: &Harness,
    req: &NarrativesReq,
    material: NarrativesMaterial,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let sport_up = req.sport.to_uppercase();
    let NarrativesMaterial {
        corpus,
        corpus_exclusions,
        heat,
        input_hash,
    } = material;

    // No corpus → the NULL-narrative marker path (no model call).
    if corpus.is_empty() {
        return Ok(NarrativesBuild::NoCorpus {
            corpus_exclusions,
            input_hash,
        });
    }

    // Memory-load failure degrades to an unenriched prompt, mirroring the heat
    // error-swallowing above: the corpus is the primary signal, memory is enrichment.
    let memory = match load_entity_memory(&hx.pool, &sport_up, &req.entity_type, req.entity_id)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!(
                entity_type = %req.entity_type,
                entity_id = req.entity_id,
                sport = %sport_up,
                error = %e,
                "narratives: relational memory load failed (continuing without memory)"
            );
            None
        }
    };
    let built_prompt = build_narratives_prompt(req, &corpus, &heat, memory.as_deref());
    let opts = GenerateOptions {
        system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: NARRATIVES_NUM_PREDICT,
        num_ctx: NARRATIVES_NUM_CTX,
        json_mode: false,
        // Phase 5: grammar-constrained decoding replaces "hopefully JSON" (the failure class
        // the balanced-brace salvager was built for). The Go-parity free-text contract is
        // retired; the salvager stays as the tolerant parse path either way.
        format_schema: Some(narratives_format_schema()),
    };
    let backend = hx.router.for_role(Role::NarrativeLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(NarrativesBuild::Ready(Box::new(NarrativesReady {
        corpus,
        corpus_exclusions,
        opts,
        built_prompt,
        request_body,
        model_configured,
        input_hash,
    })))
}

/// build_narratives_request runs the full deterministic prefix: load the widened vetted corpus, load
/// the transfer heat for grounding, then
/// `build_narratives_prompt` plus the n4 options and the exact wire body. NO model call — these
/// are the deterministic axes (the L2 finding: the storyline grouping is not a temp-0 parity
/// axis). The role is [`Role::NarrativeLogic`] (the news/transfer reasoner — narratives shares it
/// with vibe/transfers). Composition of [`load_narratives_material`] +
/// [`finish_narratives_build`]; the live handler calls the phases directly to debounce between
/// them.
pub async fn build_narratives_request(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let material = load_narratives_material(hx, req).await?;
    finish_narratives_build(hx, req, material, temperature).await
}

/// The un-persisted result of one generation — everything the production persist (→ news_summaries)
/// and the parity harness need. `narratives` empty ⇒ a marker row (no corpus, OR a real generation
/// that yielded no usable grounded storyline). The twin of `rating::RatingOutput`.
#[derive(Clone, Debug)]
pub struct NarrativesOutput {
    pub narratives: Vec<Narrative>,
    /// The model that answered (configured model for the no-corpus marker — Go sets `a.ollama.Model()`).
    pub model: String,
    pub prompt_version: &'static str,
    /// The exact prompt + wire body (the deterministic axes). `None` for the no-corpus marker (no call).
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    /// Tokens evaluated by Ollama for this call. `None` on no-corpus marker rows.
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
    /// n9 per-article transfer labels grounded back to real news ids — the persist writes each to
    /// `news_articles.bucket`, which is what routes the transfers stage (mig 175). Empty on the
    /// no-corpus marker path.
    pub article_buckets: Vec<(i64, ArticleBucket)>,
    /// Corpus articles outside the lookback window (excluded-evidence telemetry). The cap-based
    /// `budget_truncated` reason retired with the 25-item cap (Phase 3), so `stale_news` is the only
    /// exclusion left.
    pub stale_news_ids: Vec<i64>,
    /// The debounce key this generation was built from (Phase 1); persisted on every row of the
    /// generation so the next cycle's gate has something to compare against.
    pub input_hash: String,
}

impl NarrativesOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope. The row-level
    /// `input_news_ids` are still bound per narrative because each grounded storyline cites a
    /// different subset; `input_hash` is generation-level (the Phase 1 debounce key).
    fn provenance(&self) -> Provenance {
        let mut ids = Vec::new();
        for n in &self.narratives {
            ids.extend(n.input_news_ids.iter().copied());
        }
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: dedupe_i64(ids),
            input_hash: Some(self.input_hash.clone()),
            trigger_payload: None,
        }
    }
}

/// generate_narratives runs the full per-entity generation (the analog of `NewsNarrator.Generate`,
/// minus persistence): `build_narratives_request` → `extract(EmotionalNews)` (the tolerant parse) →
/// `ground_narratives`. The per-entity core the handler (and the parity `--vet` path) drive.
/// `now_epoch` is the recency reference for the impact scoring.
pub async fn generate_narratives(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
    now_epoch: i64,
) -> Result<NarrativesOutput> {
    let build = build_narratives_request(hx, req, temperature).await?;
    generate_narratives_from_build(hx, build, now_epoch).await
}

/// generate_narratives_from_build finishes a generation from an already-built request — the
/// handler builds ONCE, debounces on the build's `input_hash`, then hands the same build here
/// (no double corpus/heat load). `generate_narratives` stays as the build-and-run composition
/// for the parity/eval paths, which never debounce.
pub async fn generate_narratives_from_build(
    hx: &Harness,
    build: NarrativesBuild,
    now_epoch: i64,
) -> Result<NarrativesOutput> {
    let ready = match build {
        NarrativesBuild::NoCorpus {
            corpus_exclusions,
            input_hash,
        } => {
            // The NULL-narrative marker. Go sets Model = a.ollama.Model() even here.
            let model = hx.router.for_role(Role::NarrativeLogic).model().to_string();
            return Ok(NarrativesOutput {
                narratives: Vec::new(),
                model,
                prompt_version: NARRATIVES_PROMPT_VERSION,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
                article_buckets: Vec::new(),
                stale_news_ids: corpus_exclusions.stale_news_ids,
                input_hash,
            });
        }
        NarrativesBuild::Ready(r) => *r,
    };

    // route(EmotionalNews) + extract(NarrativesParser). A malformed/unsalvageable reply surfaces as
    // the parser's Err → the item fails + backs off (Go's parse failure → retry), never a marker.
    let extracted = hx
        .extract(
            Role::NarrativeLogic,
            &ready.built_prompt,
            &ready.opts,
            &NarrativesParser,
        )
        .await?;
    let parsed = extracted.value.ok_or_else(|| {
        anyhow!("narratives: parser returned None (NarrativesParser signals failure via Err)")
    })?;

    let narratives = ground_narratives(&parsed.narratives, &ready.corpus, now_epoch);
    let article_buckets = ground_article_buckets(&parsed.article_buckets, &ready.corpus);

    Ok(NarrativesOutput {
        narratives,
        model: extracted.model,
        prompt_version: NARRATIVES_PROMPT_VERSION,
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        eval_count: Some(extracted.eval_count),
        wall_ms: Some(extracted.wall_ms),
        article_buckets,
        stale_news_ids: ready.corpus_exclusions.stale_news_ids,
        input_hash: ready.input_hash,
    })
}

fn narratives_included_evidence(out: &NarrativesOutput) -> serde_json::Value {
    let narratives: Vec<serde_json::Value> = out
        .narratives
        .iter()
        .map(|n| {
            json!({
                "title": &n.title,
                "input_news_ids": &n.input_news_ids,
                "source_count": n.source_count,
                "source_names": &n.source_names,
                "impact": n.impact,
            })
        })
        .collect();
    // n9 per-article transfer labels: how many articles the Journalist tagged, and which it routed
    // to the transfers stage (bucket='transfer').
    let transfer_ids: Vec<i64> = out
        .article_buckets
        .iter()
        .filter(|(_, b)| *b == ArticleBucket::Transfer)
        .map(|(id, _)| *id)
        .collect();
    json!({
        "input_news_ids": out.provenance().input_ids,
        "narratives": narratives,
        "article_buckets": {
            "labeled": out.article_buckets.len(),
            "transfer_count": transfer_ids.len(),
            "transfer_news_ids": transfer_ids,
        },
    })
}

fn narratives_excluded_evidence(out: &NarrativesOutput) -> serde_json::Value {
    let mut excluded = Vec::new();
    if !out.stale_news_ids.is_empty() {
        excluded.push(json!({
            "reason": "stale_news",
            "dropped_count": out.stale_news_ids.len(),
            "dropped_news_ids": &out.stale_news_ids,
            "lookback_seconds": NEWS_LOOKBACK_SECS,
        }));
    }
    json!(excluded)
}

fn narratives_parser_outcome(out: &NarrativesOutput) -> &'static str {
    if out.built_prompt.is_none() {
        "no_call"
    } else if out.narratives.is_empty() {
        "parsed_empty"
    } else {
        "parsed"
    }
}

/// persist_narratives writes ONE news_summaries row per narrative (all sharing the transaction's
/// `NOW()` — a "generation"), or a single NULL-narrative marker row when there is none. Mirrors
/// `news_narratives.go::persist`: `trigger_payload` is the caller's value (the drain passes jsonb
/// `null` — Go marshals the nil trigger map). Written + compiles; not run in the offline parity
/// bin. (The `source_attribution` column — always NULL here — was dropped in mig 139, plan C7.)
pub async fn persist_narratives(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &NarrativesOutput,
) -> Result<()> {
    let prov = out.provenance().with_trigger_payload(trigger_payload);
    let trigger_json = prov.trigger_payload_json("null");

    // Batch-load the previous impact per narrative title in ONE query (plan A2 — was N
    // per-narrative round-trips). DISTINCT ON (narrative_title) ... ORDER BY narrative_title,
    // generated_at DESC takes the latest matching row PER TITLE across ALL generations, exactly
    // what the former per-narrative SELECT did. NOT the global-latest max(generated_at) form:
    // that would pin every title to the single newest generation and flip a title last seen a
    // few generations ago from heating_up/cooling_off to new_or_unmatched.
    let titles: Vec<String> = out.narratives.iter().map(|n| n.title.clone()).collect();
    let prev_by_title: std::collections::HashMap<String, i32> = if titles.is_empty() {
        std::collections::HashMap::new()
    } else {
        let rows: Vec<(String, i32)> = sqlx::query_as(
            r#"
            SELECT DISTINCT ON (narrative_title) narrative_title, impact::int
            FROM news_summaries
            WHERE entity_type = $1
              AND entity_id = $2
              AND sport = $3
              AND narrative_title = ANY($4)
              AND body IS NOT NULL
              AND impact IS NOT NULL
            ORDER BY narrative_title, generated_at DESC
            "#,
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport)
        .bind(&titles)
        .fetch_all(pool)
        .await
        .with_context(|| format!("classify narrative trajectories {entity_type}/{entity_id}"))?;
        rows.into_iter().collect()
    };

    let classified: Vec<(&Narrative, &'static str, serde_json::Value)> = out
        .narratives
        .iter()
        .map(|n| {
            let previous = prev_by_title.get(&n.title).copied();
            let (trajectory, delta_reason, delta) = classify_delta(previous, Some(n.impact));
            let reason = match delta_reason {
                "up" => "impact_up",
                "down" => "impact_down",
                "stable" => "impact_stable",
                other => other,
            };
            let components = json!({
                "previous_impact": previous,
                "current_impact": n.impact,
                "impact_delta": delta,
                "reason": reason,
            });
            (n, trajectory, components)
        })
        .collect();

    // NOW() is constant within a transaction (transaction_timestamp), so every row of this generation
    // shares one generated_at — Go's `res.GeneratedAt`, without needing a datetime crate to bind it.
    let mut tx = pool.begin().await.context("begin narratives tx")?;

    const INSERT: &str = r#"
        INSERT INTO news_summaries (
            entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components,
            input_news_ids,
            narrative_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, input_hash, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,$20,NOW()
        )
        RETURNING id"#;

    let rows: Vec<Option<(&Narrative, &'static str, serde_json::Value)>> = if classified.is_empty()
    {
        vec![None]
    } else {
        classified.into_iter().map(Some).collect()
    };

    let mut product_row_ids: Vec<i64> = Vec::with_capacity(rows.len());
    for row in rows {
        let impact_components_json;
        let trajectory_json;
        let empty_names = Vec::<String>::new();
        let title: Option<&str>;
        let body: Option<&str>;
        let impact: Option<i16>;
        let input_news_ids: &Vec<i64>;
        let narrative_updated_at: Option<i64>;
        let source_count: i32;
        let source_names: &Vec<String>;
        let source_latest_at: Option<i64>;
        let source_oldest_at: Option<i64>;
        let trajectory: &str;
        let context: &str;

        match &row {
            Some((n, row_trajectory, row_trajectory_components)) => {
                impact_components_json = n.impact_components.to_string();
                trajectory_json = row_trajectory_components.to_string();
                title = Some(n.title.as_str());
                body = Some(n.body.as_str());
                impact = Some(n.impact as i16);
                input_news_ids = &n.input_news_ids;
                // Keep source_latest_epoch bound twice: narrative_updated_at ($11) and
                // source_latest_at ($14), matching the pre-loop scored path.
                narrative_updated_at = n.source_latest_epoch;
                source_count = n.source_count;
                source_names = &n.source_names;
                source_latest_at = n.source_latest_epoch;
                source_oldest_at = n.source_oldest_epoch;
                trajectory = row_trajectory;
                context = "persist narrative row";
            }
            None => {
                impact_components_json = "{}".to_string();
                trajectory_json = "{}".to_string();
                title = None;
                body = None;
                impact = None;
                input_news_ids = &prov.input_ids;
                narrative_updated_at = Option::<i64>::None;
                source_count = 0_i32;
                source_names = &empty_names;
                source_latest_at = Option::<i64>::None;
                source_oldest_at = Option::<i64>::None;
                trajectory = DEFAULT_TRAJECTORY;
                context = "persist narratives marker";
            }
        }

        let inserted = sqlx::query(INSERT)
            .bind(entity_type)
            .bind(entity_id)
            .bind(sport)
            .bind(trigger_type)
            .bind(&trigger_json)
            .bind(title)
            .bind(body)
            .bind(impact)
            .bind(&impact_components_json)
            .bind(input_news_ids)
            .bind(narrative_updated_at)
            .bind(source_count)
            .bind(source_names)
            .bind(source_latest_at)
            .bind(source_oldest_at)
            .bind(trajectory)
            .bind(&trajectory_json)
            .bind(prov.model_version.as_str())
            .bind(prov.prompt_version)
            .bind(prov.input_hash.as_deref())
            .fetch_one(&mut *tx)
            .await
            .context(context)?;
        product_row_ids.push(inserted.get("id"));
    }

    // n9: write the Journalist's per-article transfer labels to news_articles.bucket, in the SAME
    // transaction as the storylines so the label and the narrative it came from commit atomically.
    // The `IS DISTINCT FROM` guard skips no-op rewrites, so the mig-175 AFTER-UPDATE trigger
    // (bucket → 'transfer' ⇒ enqueue transfers for the article's TEAM entities) fires only on a real
    // change, never re-enqueueing an already-transfer article every cycle.
    if !out.article_buckets.is_empty() {
        let bucket_ids: Vec<i64> = out.article_buckets.iter().map(|(id, _)| *id).collect();
        let bucket_labels: Vec<String> = out
            .article_buckets
            .iter()
            .map(|(_, b)| b.as_db().to_string())
            .collect();
        sqlx::query(
            r#"
            UPDATE news_articles a
               SET bucket = v.bucket
              FROM unnest($1::bigint[], $2::text[]) AS v(id, bucket)
             WHERE a.id = v.id
               AND a.bucket IS DISTINCT FROM v.bucket
            "#,
        )
        .bind(&bucket_ids)
        .bind(&bucket_labels)
        .execute(&mut *tx)
        .await
        .context("persist article buckets")?;
    }

    tx.commit().await.context("commit narratives tx")?;
    insert_cognition_ledger_best_effort(
        pool,
        CognitionLedgerEntry {
            stage: "narratives".to_string(),
            lens: "narratives".to_string(),
            role: Role::NarrativeLogic.as_str().to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: trigger_type.to_string(),
            trigger_payload: trigger_payload.clone(),
            product_table: "news_summaries".to_string(),
            product_row_ids,
            model_version: prov.model_version,
            prompt_version: prov.prompt_version.to_string(),
            output_contract_version: NARRATIVES_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: prov.input_ids,
            input_hash: prov.input_hash,
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: narratives_included_evidence(out),
            excluded_evidence: narratives_excluded_evidence(out),
            context_budget: json!({
                "num_predict": NARRATIVES_NUM_PREDICT,
                "num_ctx": NARRATIVES_NUM_CTX,
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: narratives_parser_outcome(out).to_string(),
        },
    )
    .await;
    Ok(())
}

/// now_unix is the recency reference for `compute_news_impact` — Unix seconds, no datetime crate.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Handler.
// ---------------------------------------------------------------------------

/// NarrativesHandler drains the durable `narratives` stage: read the vetted corpus, (live) dedup it,
/// group it into storylines with the model, score each deterministically, and persist one
/// news_summaries row per narrative (or a marker). Unlike rating, narratives is a `pipeline_work`
/// stage (`Stage::Narratives`).
pub struct NarrativesHandler;

impl NarrativesHandler {
    pub fn new() -> Self {
        NarrativesHandler
    }
}

impl Default for NarrativesHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for NarrativesHandler {
    fn stage(&self) -> Stage {
        Stage::Narratives
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // nameOf uses the queue's raw sport value (drainNarratives), as does the prompt's req.Sport.
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;
        let req = NarrativesReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: item.sport.clone(),
            trigger_type: "periodic".to_string(),
        };
        let sport_up = item.sport.to_uppercase();

        // Load material, gate, THEN build (Phase 2 refines Phase 1's build-once-then-gate):
        // narratives was the heaviest GPU stage and regenerated unconditionally every wake
        // cycle. When the material inputs (vetted corpus ids + heat facts) match the latest
        // persisted generation's hash, skip the dedup embed, the model call, AND the insert —
        // readers use max(generated_at), so the previous generation keeps serving, and
        // downstream vibe/sigil see no phantom "new" input. Pre-145 rows carry a NULL hash,
        // which never matches → one regeneration stamps it. The hash keys on the pre-dedup
        // material, so gating before the candle pass changes no debounce semantics.
        let material = load_narratives_material(hx, &req).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport_up.clone(),
            season: None,
        };
        if hx
            .debounce_unchanged("news_summaries", &key, &material.input_hash)
            .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id,
                sport = %sport_up,
                "narratives: inputs unchanged, skipping generation"
            );
            return Ok(());
        }

        let build = finish_narratives_build(hx, &req, material, NARRATIVES_TEMPERATURE).await?;
        let out = generate_narratives_from_build(hx, build, now_unix()).await?;

        // Go marshals the nil trigger map → jsonb `null`.
        persist_narratives(
            &hx.pool,
            &item.entity_type,
            entity_id,
            &sport_up,
            &req.trigger_type,
            &serde_json::Value::Null,
            &out,
        )
        .await?;

        // Phase 3 hand-off: narratives now feeds Vibe (mirroring vibe → momentum). Vibe reads this
        // generation's storylines + the transfer heat, so enqueue it once that material has moved.
        // (The scrub `vetted` trigger no longer enqueues vibe — mig 174.) Any transfers routing
        // rides the news_articles.bucket write in persist_narratives (mig 175 trigger).
        if !crate::vibe::enqueue_vibe_if_needed(
            hx,
            &item.entity_type,
            entity_id,
            &req.entity_name,
            &sport_up,
        )
        .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id,
                sport = %sport_up,
                "narratives: vibe enqueue skipped unchanged/empty context"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, source: &str, title: &str, desc: &str, epoch: Option<i64>) -> CorpusItem {
        CorpusItem {
            id,
            title: title.to_string(),
            description: desc.to_string(),
            source: source.to_string(),
            url: String::new(),
            published_at_epoch: epoch,
            fetched_at_epoch: epoch,
            full_text: None,
        }
    }

    fn req(name: &str, sport: &str, etype: &str) -> NarrativesReq {
        NarrativesReq {
            entity_type: etype.to_string(),
            entity_id: 1,
            entity_name: name.to_string(),
            sport: sport.to_string(),
            trigger_type: "periodic".to_string(),
        }
    }

    // --- build_narratives_prompt byte-fixtures: deterministic prompt assembly. ----------------------

    #[test]
    fn prompt_numbered_news_no_heat() {
        let news = vec![
            item(
                10,
                "BBC",
                "Saka shines again",
                "A strong display in the win.",
                None,
            ),
            item(11, "", "Arsenal eye a new winger", "", None),
        ];
        let p = build_narratives_prompt(&req("Bukayo Saka", "FOOTBALL", "player"), &news, &[], None);
        assert!(
            !p.contains("Relational memory"),
            "no memory ⇒ no section (n7 byte-shape preserved)"
        );
        let with_mem = build_narratives_prompt(
            &req("Bukayo Saka", "FOOTBALL", "player"),
            &news,
            &[],
            Some("Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nGround truth: Bukayo Saka completed a confirmed move to Arsenal on Jul 01 2026."),
        );
        assert!(with_mem.contains("Relational memory (computed history"));
        assert!(with_mem.contains("- Prior story: Real Madrid — fizzled"));
        assert!(with_mem.contains("- Ground truth: Bukayo Saka completed"));
        assert_eq!(
            p,
            "Entity: Bukayo Saka (FOOTBALL player)\n\
\nRecent news (numbered):\n\
1. [BBC] Saka shines again — A strong display in the win.\n\
2. Arsenal eye a new winger\n\
\nReturn the JSON object now."
        );
    }

    #[test]
    fn prompt_with_heat_section() {
        let news = vec![item(20, "ESPN", "Trade buzz grows", "", None)];
        let heat = vec![
            HeatItem {
                counterparty: "Lakers".to_string(),
                heat: 80,
                stage: "advanced talks".to_string(),
                direction: "incoming".to_string(),
                summary: "Lakers in advanced talks per ESPN".to_string(),
                confidence: Some(0.8),
            },
            HeatItem {
                counterparty: "Heat".to_string(),
                heat: 40,
                stage: String::new(),
                direction: String::new(),
                summary: String::new(),
                confidence: None,
            },
        ];
        let p = build_narratives_prompt(&req("Some Team", "NBA", "team"), &news, &heat, None);
        assert_eq!(
            p,
            "Entity: Some Team (NBA team)\n\
\nRecent news (numbered):\n\
1. [ESPN] Trade buzz grows\n\
\nKnown transfer/trade activity (vetted facts — ground any transfer storyline in these, do not contradict them):\n\
- Lakers — heat 80, incoming, advanced talks (confidence 0.8) — \"Lakers in advanced talks per ESPN\"\n\
- Heat — heat 40\n\
\nReturn the JSON object now."
        );
    }

    // --- full_text corpus seam (mig 171, plan decision 3) ------------------------------------------

    #[test]
    fn full_text_seam_prefers_body_else_falls_back_and_is_inert_when_null() {
        // None (today's state for every row): renders `description` — byte-for-byte the pre-seam
        // prompt, so the seam is inert until a fetcher populates full_text.
        let none = item(1, "BBC", "Saka shines again", "A strong display in the win.", None);
        assert_eq!(article_body(&none), "A strong display in the win.");

        // Some(non-empty): the fetched body wins over the provider blurb.
        let mut full = item(2, "BBC", "Title", "short blurb", None);
        full.full_text = Some("The full article body with much more detail.".to_string());
        assert_eq!(article_body(&full), "The full article body with much more detail.");

        // Some(blank): whitespace-only body is not a body — fall back to description.
        let mut blank = item(3, "BBC", "Title", "blurb here", None);
        blank.full_text = Some("   \n ".to_string());
        assert_eq!(article_body(&blank), "blurb here");

        // The rendered prompt is unchanged when full_text is None (the parity guarantee).
        let p = build_narratives_prompt(&req("Bukayo Saka", "FOOTBALL", "player"), &[none], &[], None);
        assert!(p.contains("1. [BBC] Saka shines again — A strong display in the win.\n"));
    }

    // --- input components: the debounce pre-image ---------------------------------------------------

    #[test]
    fn input_components_are_stable_across_input_order() {
        // Same articles + heat in a different order ⇒ identical pre-image (sorted), and the
        // heat SUMMARY/CONFIDENCE never enter it (derived commentary — a re-worded transfer
        // summary alone must not regenerate narratives).
        let a = |id: i64| item(id, "ESPN", "t", "", None);
        let h = |cp: &str, heat: i32, summary: &str| HeatItem {
            counterparty: cp.to_string(),
            heat,
            stage: "speculation".to_string(),
            direction: "incoming".to_string(),
            summary: summary.to_string(),
            confidence: Some(0.5),
        };
        let one = build_narratives_input_components(
            &[a(3), a(1)],
            &[h("B", 40, "worded one way"), h("A", 70, "x")],
        );
        let two = build_narratives_input_components(
            &[a(1), a(3)],
            &[h("A", 70, "y"), h("B", 40, "worded another way")],
        );
        assert_eq!(one, two);
        // prompt_version leads the pre-image (single-sourced from the const, so a bump can't silently
        // rot this pin) — an n-bump changes every entity's hash once, forcing the cutover regen.
        assert_eq!(
            one,
            format!(
                r#"{{"prompt_version":"{NARRATIVES_PROMPT_VERSION}","article_ids":[1,3],"transfer_heat":["A:70:incoming:speculation","B:40:incoming:speculation"]}}"#
            )
        );
        // No heat ⇒ no transfer_heat key (mirrors sigil's conditional-key convention).
        assert_eq!(
            build_narratives_input_components(&[a(1)], &[]),
            format!(r#"{{"prompt_version":"{NARRATIVES_PROMPT_VERSION}","article_ids":[1]}}"#)
        );
    }

    // --- parse_narratives: the tolerant salvager ---------------------------------------------------

    #[test]
    fn parse_clean_array() {
        let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1,2]},{"title":"C","body":"d","articles":[3]}]}"#;
        let (ns, ok) = parse_narratives(raw);
        assert!(ok);
        assert_eq!(ns.len(), 2);
        assert_eq!(ns[0].title, "A");
        assert_eq!(ns[0].articles, vec![1, 2]);
        assert_eq!(ns[1].articles, vec![3]);
    }

    #[test]
    fn parse_empty_array_is_ok_no_narratives() {
        // A cleanly-closed empty array is a SUCCESSFUL parse with zero narratives → marker, not failure.
        let (ns, ok) = parse_narratives(r#"{"narratives": []}"#);
        assert!(ok);
        assert!(ns.is_empty());
    }

    #[test]
    fn parse_truncated_tail_salvages_complete_objects() {
        // EOF before the array closes: keep the complete leading object, drop the half-written tail.
        let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1]},{"title":"C","body":"#;
        let (ns, ok) = parse_narratives(raw);
        assert!(ok); // salvaged ≥1
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].title, "A");
    }

    #[test]
    fn parse_missing_key_is_failure() {
        let (ns, ok) = parse_narratives(r#"{"something_else": 1}"#);
        assert!(!ok);
        assert!(ns.is_empty());
    }

    #[test]
    fn parse_malformed_nothing_salvaged_is_failure() {
        // Has the key + '[' but the lone object never closes and nothing parses → failure (retry).
        let (ns, ok) = parse_narratives(r#"{"narratives": [{"title":"A"#);
        assert!(!ok);
        assert!(ns.is_empty());
    }

    #[test]
    fn parse_respects_braces_inside_strings() {
        // A '}' inside a string value must not close the object early.
        let raw = r#"{"narratives": [{"title":"A } B","body":"x","articles":[1]}]}"#;
        let (ns, ok) = parse_narratives(raw);
        assert!(ok);
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].title, "A } B");
    }

    // --- n9 article_buckets: tolerant parse + grounding -------------------------------------------

    #[test]
    fn parse_article_buckets_reads_the_section() {
        let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1]}],
                      "article_buckets": [{"article":1,"transfer":true},{"article":2,"transfer":false}]}"#;
        let b = parse_article_buckets(raw);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].article, b[0].transfer), (1, true));
        assert_eq!((b[1].article, b[1].transfer), (2, false));
        // The full parse yields both sections; a reply with no buckets key parses to an empty section.
        let doc = NarrativesParser.parse(raw).unwrap().unwrap();
        assert_eq!(doc.article_buckets.len(), 2);
        assert!(parse_article_buckets(r#"{"narratives": []}"#).is_empty());
    }

    #[test]
    fn parse_article_buckets_salvages_truncated_tail() {
        // The narratives array parsed cleanly; the buckets section truncates mid-object → keep the
        // complete leading entries, drop the half-written one, never fail the document.
        let raw = r#"{"narratives": [{"title":"A","body":"b","articles":[1]}], "article_buckets": [{"article":1,"transfer":true},{"article":2,"tr"#;
        let doc = NarrativesParser.parse(raw).unwrap().unwrap();
        assert_eq!(doc.narratives.len(), 1);
        assert_eq!(doc.article_buckets.len(), 1);
        assert_eq!(doc.article_buckets[0].article, 1);
    }

    #[test]
    fn ground_article_buckets_maps_dedupes_and_bounds() {
        let news = vec![
            item(100, "BBC", "one", "", None),
            item(101, "ESPN", "two", "", None),
        ];
        let parsed = vec![
            ModelArticleBucket { article: 1, transfer: true },
            ModelArticleBucket { article: 1, transfer: false }, // dup article → first label wins
            ModelArticleBucket { article: 2, transfer: false },
            ModelArticleBucket { article: 9, transfer: true }, // out of range → dropped
            ModelArticleBucket { article: 0, transfer: true }, // < 1 → dropped
        ];
        let out = ground_article_buckets(&parsed, &news);
        assert_eq!(
            out,
            vec![(100, ArticleBucket::Transfer), (101, ArticleBucket::NonTransfer)]
        );
    }

    // --- ground_narratives: numbering, dedupe, bounds, drop-rules ---------------------------------

    #[test]
    fn ground_maps_numbers_dedupes_and_bounds() {
        let news = vec![
            item(100, "BBC", "one", "", Some(1_000)),
            item(101, "ESPN", "two", "", Some(2_000)),
        ];
        let parsed = vec![
            ModelNarrative {
                title: " Title ".to_string(), // trimmed
                body: "Body".to_string(),
                articles: vec![1, 1, 2, 9, 0, -3], // dup 1, out-of-range 9/0/-3 dropped
            },
            ModelNarrative {
                title: "".to_string(), // empty title → dropped
                body: "x".to_string(),
                articles: vec![1],
            },
            ModelNarrative {
                title: "no articles".to_string(),
                body: "y".to_string(),
                articles: vec![9, 0], // all out of range → ungrounded → dropped
            },
        ];
        let out = ground_narratives(&parsed, &news, 10_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Title");
        assert_eq!(out[0].input_news_ids, vec![100, 101]); // 1→id100, 2→id101, dup/oob removed
        assert_eq!(out[0].source_count, 2);
        assert_eq!(out[0].source_names, vec!["BBC", "ESPN"]);
        assert_eq!(out[0].source_latest_epoch, Some(2_000));
        assert_eq!(out[0].source_oldest_epoch, Some(1_000));
    }

    // --- compute_news_impact: the deterministic score ---------------------------------------------

    #[test]
    fn impact_volume_corroboration_recency() {
        // 2 articles, 2 distinct sources, newest 1h old (now=10000, newest=6400 → age 3600s ≤ 12h).
        let news = vec![
            item(1, "BBC", "a", "", Some(6_400)),
            item(2, "ESPN", "b", "", Some(5_000)),
        ];
        let (score, comp) = compute_news_impact(&news, 10_000);
        // volume = 60*(1-e^(-2/5)) ≈ 19.78 → round1 19.8; corroboration = min(25, 2*6)=12; recency 15.
        // score = round(19.78 + 12 + 15) = round(46.78) = 47.
        assert_eq!(score, 47);
        assert_eq!(comp["article_count"], json!(2));
        assert_eq!(comp["distinct_sources"], json!(2));
        assert_eq!(comp["corroboration"], json!(12.0));
        assert_eq!(comp["recency"], json!(15.0));
    }

    #[test]
    fn impact_clamps_and_buckets_recency() {
        // No publish times → recency 0; one source → corroboration capped low.
        let news = vec![item(1, "src", "a", "", None)];
        let (score, comp) = compute_news_impact(&news, 10_000);
        // volume = 60*(1-e^-0.2) ≈ 10.88; corroboration 6; recency 0 → round(16.88)=17.
        assert_eq!(score, 17);
        assert_eq!(comp["recency"], json!(0.0));
    }

    #[test]
    fn impact_recency_buckets() {
        let day = 24 * 3600;
        // newest 30h old → falls in the ≤48h bucket → recency 5.
        let news = vec![item(1, "s", "a", "", Some(0))];
        let (_, comp) = compute_news_impact(&news, 30 * 3600);
        assert_eq!(comp["recency"], json!(5.0));
        // newest 20h old → ≤24h → recency 10.
        let (_, comp2) = compute_news_impact(&news, 20 * 3600);
        assert_eq!(comp2["recency"], json!(10.0));
        // 3 days old → no bucket → 0.
        let (_, comp3) = compute_news_impact(&news, 3 * day);
        assert_eq!(comp3["recency"], json!(0.0));
    }
}
