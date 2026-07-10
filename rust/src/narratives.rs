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

use crate::corpus::{
    dedupe_i64, load_transfer_heat, lookup_entity_name, write_heat_lines, HeatItem,
};
use crate::harness::{cluster, Harness, Parser, Provenance};
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
use sqlx::PgPool;
use std::collections::HashSet;
use tracing::warn;

// ---------------------------------------------------------------------------
// Constants — mirror news_narratives.go.
// ---------------------------------------------------------------------------

/// Bump when the prompt materially changes (traced in `news_summaries.prompt_version`).
pub const NARRATIVES_PROMPT_VERSION: &str = "n5";

/// Production decode temperature (`ollama.Generate` in Go). The parity gate pins temp 0 (the
/// deterministic-axes diff); production narrates at 0.6.
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;

/// Several multi-sentence narratives; the prompt caps count + body length.
pub const NARRATIVES_NUM_PREDICT: i32 = 3000;

/// Bounds the articles fed to the grouping prompt — wider than the vibe window so the model sees
/// enough breadth to find the distinct storylines (Go's `maxNarrativeCorpus`).
const MAX_NARRATIVE_CORPUS: i64 = 25;

/// Per-article description cap rendered into the prompt (Go's `truncate(desc, 200)`).
const DESC_TRUNCATE: usize = 200;

/// The vetted-news lookback window — Go's `NewsLookback = 72 * time.Hour`, in seconds. Bound as the
/// `make_interval(secs => …)` argument so the corpus boundary equals Go's `$4::interval` of
/// `"259200 seconds"`.
const NEWS_LOOKBACK_SECS: f64 = 259_200.0;

/// DEDUP_THRESHOLD is the single-link cosine cutoff for the candle near-duplicate dedup VALUE-ADD
/// (Plan §1.4). Two articles whose embeddings are ≥ this similar collapse to one storyline
/// representative before the model call.
///
/// It is a deterministic CLUSTERING param, not a model identity/route — the embedding MODEL itself is
/// config (`COGNITION_EMBED_*`, the boundary that matters: "models by role, never by name"), and the
/// dedup ON/OFF is governed by whether the live worker loads the embedder (offline/parity bins do
/// not → identity → byte-parity with Go). Kept a `pub const` (like the `cluster()` thresholds) to
/// stay surgical — promoting it to `COGNITION_NARRATIVES_DEDUP_THRESHOLD` is a one-line `env_float`
/// if live tuning ever demands it.
pub const DEDUP_THRESHOLD: f32 = 0.85;

/// System prompt for grouping recent vetted news into distinct storylines.
pub const NARRATIVES_SYSTEM_PROMPT: &str = r#"Task: group recent vetted news into distinct storylines about ONE sports entity.

Voice: direct, sports-literate, grounded. No hype, no source list, no invented facts.

Return STRICT JSON only (no markdown fences, no text before or after):
{"narratives": [{"title": "<headline>", "body": "<write-up>", "articles": [<article numbers>]}, ...]}

Rules:
- Return at most 6 narratives, most consequential first.
- Do not split one story across narratives.
- Do not merge unrelated stories.
- A quiet cycle can return one narrative or none.
- Ignore vague hype when the sources do not name who, what, and where.
- Ignore articles that are not actually about this entity.

For each:
- title: short and specific, naming the key people/clubs; never generic like "Transfer news".
- body: explain what is happening, who is involved, and where it stands. Most are one or two sentences; write more only for a genuinely major, multi-source story.
- articles: the article numbers behind that storyline.

If a "Known transfer/trade activity" list is given, treat it as vetted truth for transfer/trade storylines. Use it for counterparties, direction, and stage. Never contradict it or claim a more advanced stage. The word "heat" and its numbers are internal; never mention them.

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

/// ParsedNarratives is the salvaged document — the `T` the [`NarrativesParser`] yields.
#[derive(Clone, Debug, Default)]
pub struct ParsedNarratives {
    narratives: Vec<ModelNarrative>,
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
        Ok(Some(ParsedNarratives { narratives }))
    }
}

// ---------------------------------------------------------------------------
// Corpus loader — byte-for-byte the SQL news_narratives.go runs (same query ⇒ same rows).
// ---------------------------------------------------------------------------

/// load_vetted_corpus reads the entity's recent VETTED news links (the scrub gate kept), wider than
/// the vibe window so the model sees enough breadth to find the distinct storylines. Verbatim Go SQL;
/// only `published_at` is projected as an epoch `bigint` (`EXTRACT(EPOCH …)::bigint`, NULL-preserving)
/// so the recency math needs no datetime crate, and the lookback is `make_interval(secs => $4)`
/// (= Go's `$4::interval` of `"259200 seconds"`). `sport` is the UPPER-cased value (Go upper-cases
/// before the read).
pub async fn load_vetted_corpus(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<CorpusItem>> {
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
    )> = sqlx::query_as(
        r#"
        SELECT a.id, a.title, COALESCE(a.description, ''), COALESCE(a.source, ''),
               COALESCE(a.url, ''),
               EXTRACT(EPOCH FROM a.published_at)::bigint,
               EXTRACT(EPOCH FROM a.fetched_at)::bigint
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
          AND nae.vetted IS TRUE
          AND a.bucket IS DISTINCT FROM 'transfer'
          AND (a.published_at IS NULL OR a.published_at > NOW() - make_interval(secs => $4))
        ORDER BY a.topic_heat DESC NULLS LAST, COALESCE(a.published_at, a.fetched_at) DESC
        LIMIT $5
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(NEWS_LOOKBACK_SECS)
    .bind(MAX_NARRATIVE_CORPUS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load vetted corpus {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, description, source, url, published_at_epoch, fetched_at_epoch)| {
                CorpusItem {
                    id,
                    title,
                    description,
                    source,
                    url,
                    published_at_epoch,
                    fetched_at_epoch,
                }
            },
        )
        .collect())
}

// ---------------------------------------------------------------------------
// Embed+cluster dedup — the candle VALUE-ADD (Plan §1.4). Identity when no embedder (parity bins).
// ---------------------------------------------------------------------------

/// embed_text is what we vectorize per article — the title plus its blurb (the storyline content).
fn embed_text(c: &CorpusItem) -> String {
    if c.description.is_empty() {
        c.title.clone()
    } else {
        format!("{} {}", c.title, c.description)
    }
}

/// dedup_corpus collapses near-duplicate coverage before the model call: embed each article (candle,
/// CPU), single-link `cluster()` at [`DEDUP_THRESHOLD`], and keep ONE representative per cluster — the
/// freshest (smallest index, since the corpus is `published_at DESC`). `cluster()` returns members
/// ascending and clusters ordered by smallest member, so the survivors stay in freshest-first order.
///
/// This is the dedup the Go pipeline never had (a deliberate improvement). It runs ONLY when an
/// `Embedder` is loaded (the live handler); the offline parity bins build `Harness { embedder: None }`,
/// so this is the IDENTITY and the assembled prompt is byte-identical to Go — the deterministic axes
/// diff equal (Plan §3 gate). Where it DOES change the input set (live), that is documented value-add,
/// never a parity break.
async fn dedup_corpus(hx: &Harness, corpus: Vec<CorpusItem>) -> Result<Vec<CorpusItem>> {
    if hx.embedder.is_none() || corpus.len() < 2 || DEDUP_THRESHOLD <= 0.0 {
        return Ok(corpus);
    }
    let texts: Vec<String> = corpus.iter().map(embed_text).collect();
    let vectors = hx.embed(&texts).await.context("embed narrative corpus")?;
    let clusters = cluster(&vectors, DEDUP_THRESHOLD);
    let keep: Vec<usize> = clusters.iter().map(|c| c.members[0]).collect();
    Ok(keep.into_iter().map(|i| corpus[i].clone()).collect())
}

// ---------------------------------------------------------------------------
// Prompt — byte-for-byte buildNarrativesPrompt.
// ---------------------------------------------------------------------------

/// build_narratives_prompt assembles the user prompt, byte-for-byte the same as Go's
/// `buildNarrativesPrompt`. The `—` (U+2014) bytes are significant. The heat section is OMITTED
/// entirely when there is no transfer heat (unlike vibe's "(none)" line), matching Go's `if len(heat) > 0`.
pub fn build_narratives_prompt(
    req: &NarrativesReq,
    news: &[CorpusItem],
    heat: &[HeatItem],
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
        if !n.description.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(&n.description, DESC_TRUNCATE));
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
    b.push_str("\nReturn the JSON object now.");
    b
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
    NoCorpus,
    Ready(Box<NarrativesReady>),
}

/// NarrativesReady carries the assembled model inputs (the parity axes) plus the (possibly deduped)
/// corpus the grounding maps back to. `request_body` is computed from the SAME backend + opts the call
/// will use, so it can never drift from what is POSTed.
pub struct NarrativesReady {
    /// The numbered corpus the model sees (deduped when an embedder is loaded; identity otherwise).
    pub corpus: Vec<CorpusItem>,
    /// Corpus size BEFORE dedup (inspection: how many near-duplicates the candle pass dropped).
    pub original_corpus_size: usize,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
}

/// build_narratives_request runs the deterministic prefix: load the vetted corpus, (if an embedder is
/// loaded) dedup near-duplicates, load the transfer heat for grounding, then `build_narratives_prompt`
/// plus the n4 options and the exact wire body. NO model call — these are the deterministic axes (the L2
/// finding: the storyline grouping is not a temp-0 parity axis). The role is [`Role::EmotionalNews`]
/// (the news/transfer reasoner — narratives shares it with vibe/transfers).
pub async fn build_narratives_request(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let sport_up = req.sport.to_uppercase();

    // load_vetted_corpus and load_transfer_heat are independent reads — run them concurrently
    // (plan A3). The heat error-swallowing stays INSIDE the joined future so "a heat-read failure
    // must NEVER block the narrative (the corpus is the primary signal)" survives; a corpus error
    // still aborts the join. Note: heat now runs on the no-corpus path too (the early return moved
    // below the join) — no output change, just an extra read on that branch.
    let (corpus, heat) = tokio::try_join!(
        load_vetted_corpus(&hx.pool, &req.entity_type, req.entity_id, &sport_up),
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

    // No corpus → the NULL-narrative marker path (no model call).
    if corpus.is_empty() {
        return Ok(NarrativesBuild::NoCorpus);
    }
    let original_corpus_size = corpus.len();
    let corpus = dedup_corpus(hx, corpus).await?;

    let built_prompt = build_narratives_prompt(req, &corpus, &heat);
    let opts = GenerateOptions {
        system: Some(NARRATIVES_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: NARRATIVES_NUM_PREDICT,
        json_mode: false, // narratives is free-text JSON-instructed, NOT Ollama format=json (Go parity)
    };
    let backend = hx.router.for_role(Role::EmotionalNews);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(NarrativesBuild::Ready(Box::new(NarrativesReady {
        corpus,
        original_corpus_size,
        opts,
        built_prompt,
        request_body,
        model_configured,
    })))
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
    /// Inspection: corpus size before/after the candle dedup.
    pub original_corpus_size: usize,
    pub deduped_corpus_size: usize,
}

impl NarrativesOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope. Narratives has no
    /// input_hash debounce; the row-level `input_news_ids` are still bound per narrative because
    /// each grounded storyline cites a different subset.
    fn provenance(&self) -> Provenance {
        let mut ids = Vec::new();
        for n in &self.narratives {
            ids.extend(n.input_news_ids.iter().copied());
        }
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: dedupe_i64(ids),
            input_hash: None,
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
    let ready = match build_narratives_request(hx, req, temperature).await? {
        NarrativesBuild::NoCorpus => {
            // The NULL-narrative marker. Go sets Model = a.ollama.Model() even here.
            let model = hx.router.for_role(Role::EmotionalNews).model().to_string();
            return Ok(NarrativesOutput {
                narratives: Vec::new(),
                model,
                prompt_version: NARRATIVES_PROMPT_VERSION,
                built_prompt: None,
                request_body: None,
                original_corpus_size: 0,
                deduped_corpus_size: 0,
            });
        }
        NarrativesBuild::Ready(r) => *r,
    };

    // route(EmotionalNews) + extract(NarrativesParser). A malformed/unsalvageable reply surfaces as
    // the parser's Err → the item fails + backs off (Go's parse failure → retry), never a marker.
    let extracted = hx
        .extract(
            Role::EmotionalNews,
            &ready.built_prompt,
            &ready.opts,
            &NarrativesParser,
        )
        .await?;
    let parsed = extracted.value.ok_or_else(|| {
        anyhow!("narratives: parser returned None (NarrativesParser signals failure via Err)")
    })?;

    let narratives = ground_narratives(&parsed.narratives, &ready.corpus, now_epoch);

    Ok(NarrativesOutput {
        narratives,
        model: extracted.model,
        prompt_version: NARRATIVES_PROMPT_VERSION,
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        original_corpus_size: ready.original_corpus_size,
        deduped_corpus_size: ready.corpus.len(),
    })
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
            model_version, prompt_version, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,NOW()
        )"#;

    let rows: Vec<Option<(&Narrative, &'static str, serde_json::Value)>> = if classified.is_empty()
    {
        vec![None]
    } else {
        classified.into_iter().map(Some).collect()
    };

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

        sqlx::query(INSERT)
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
            .execute(&mut *tx)
            .await
            .context(context)?;
    }

    tx.commit().await.context("commit narratives tx")?;
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

        let out = generate_narratives(hx, &req, NARRATIVES_TEMPERATURE, now_unix()).await?;

        let sport_up = item.sport.to_uppercase();
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
        let p = build_narratives_prompt(&req("Bukayo Saka", "FOOTBALL", "player"), &news, &[]);
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
            },
            HeatItem {
                counterparty: "Heat".to_string(),
                heat: 40,
                stage: String::new(),
                direction: String::new(),
            },
        ];
        let p = build_narratives_prompt(&req("Some Team", "NBA", "team"), &news, &heat);
        assert_eq!(
            p,
            "Entity: Some Team (NBA team)\n\
\nRecent news (numbered):\n\
1. [ESPN] Trade buzz grows\n\
\nKnown transfer/trade activity (vetted facts — ground any transfer storyline in these, do not contradict them):\n\
- Lakers — heat 80, incoming, advanced talks\n\
- Heat — heat 40\n\
\nReturn the JSON object now."
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
