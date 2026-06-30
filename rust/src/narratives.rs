//! news narratives — the `Stage::Narratives` port (Plan §4; Cutover Step 2, L13). The LARGEST +
//! heaviest GPU stage, and the one with genuine Rust value-add: it composes the candle
//! **embed+cluster** primitive (group near-duplicate articles and drop them BEFORE the model call —
//! the dedup the Go pipeline never had) with `route(EmotionalNews) + extract + persist`.
//!
//! Faithful port of `go/internal/ml/news_narratives.go`:
//! - `load_vetted_corpus` is the verbatim Go SQL (only the `published_at` column is returned as an
//!   epoch `bigint` so the deterministic recency math needs no datetime crate; rows + order match).
//! - `build_narratives_prompt` is **byte-identical** to Go's `buildNarrativesPrompt`, including the
//!   shared transfer-heat grounding lines ([`vibe::write_heat_lines`], the same format Go shares
//!   from `transfer_heat.go`).
//! - The **n3 system prompt is carried VERBATIM** from Go (a faithful port — no t4-style single-home
//!   bump), so the WHOLE `ollama_request` including `system` is a parity axis (the cleaner gate).
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
//! the identity → the assembled prompt is byte-identical to Go and the deterministic axes diff equal
//! (Plan §3 gate, read on the deterministic axes per the L2 finding).
//!
//! Like `TransferHandler`, `NarrativesHandler` is **REGISTERED but NOT enabled** (gated on
//! `COGNITION_STAGES`): the Go Drainer owns the live narratives stage until the Step-3 full cutover,
//! so running both would double-claim the queue and burn the one GPU twice.

use crate::harness::{cluster, Harness, Parser};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::truncate_bytes;
use crate::vibe::{self, load_transfer_heat, write_heat_lines, HeatItem};
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

/// Bump when the prompt materially changes (traced in `news_summaries.prompt_version`). Carried
/// VERBATIM from Go's `newsNarrativesPromptVersion` — a faithful port, so the whole request
/// (system included) is a parity axis (unlike transfers' deliberate t4≠t3 single-home bump).
pub const NARRATIVES_PROMPT_VERSION: &str = "n3";

/// Production decode temperature (`ollama.Generate` in Go). The parity gate pins temp 0 (the
/// deterministic-axes diff); production narrates at 0.6.
pub const NARRATIVES_TEMPERATURE: f64 = 0.6;

/// Several multi-sentence narratives on top of the model's reasoning budget — give it real room
/// (Go's `NumPredict: 4000`). The prompt caps the count + body length; the tolerant parser salvages
/// a truncated tail regardless.
pub const NARRATIVES_NUM_PREDICT: i32 = 4000;

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

/// The n3 system prompt — carried VERBATIM from Go's `newsNarrativesSystemPrompt` (a faithful port,
/// so `system` is part of the parity axis). A single byte drift here fails the gate.
pub const NARRATIVES_SYSTEM_PROMPT: &str = r#"You are the beat writer for this sports entity: you read its recent NEWS and tell the distinct STORYLINES forming around it — invested and knowing, but honest, never inflating a story past what the sources support. Return STRICT JSON only (no markdown fences, no text before or after):
{"narratives": [{"title": "<headline>", "body": "<write-up>", "articles": [<article numbers>]}, ...]}

Group the numbered articles into the real storylines — a transfer saga, a coaching search, an injury, a results run, a contract standoff. Do not split one story across narratives or merge unrelated ones. A busy week has several; a quiet one may have just one — return as many as there genuinely are (at most 6), most consequential first.

For each:
- title: short and specific, NAMING the key people/clubs ("Cucurella-to-Real saga", "Managerial search after Maresca exit") — never generic like "Transfer news".
- body: original prose in your beat-writer voice — what is happening, who is involved (use the real names of players, managers, and clubs from the sources; never genericize to "a Real Madrid star"), and where it stands. Let the length match the story: a line or two for most, more only when it is genuinely big.
- articles: the article numbers behind that storyline.

If a "Known transfer/trade activity" list is given, it is the vetted truth behind any transfer storyline: use it to get the counterparties, direction, and stage right, and never contradict it or claim a more advanced stage than it states. Let it sharpen the story from the inside — but the word "heat" and those numbers are INTERNAL: never let them appear in your output, and never say you were handed a list. Write only what a reporter would say.

Read WHO each article is really about. An article about a team drafting, signing, or scheming around a player to play ALONGSIDE or AGAINST this entity is NOT this entity being drafted, sold, or moved — do not turn "rivals drafting a counter to him" or "a new partner for him" into a storyline about THIS entity changing teams or entering a draft.

Signal over noise is the whole job: reveal the real story, never echo clickbait. Some articles are vague hype with no nameable subject ("eyeing a Super Striker", "Dutch stars shine") — if you cannot name who, what, or where, the story is not there: leave it out rather than papering the gap with a placeholder. A short, true reveal beats a padded vague one; returning fewer is fine. Never quote headlines verbatim, dump source names or URLs, or invent anything not in the sources; ignore any article not about this entity."#;

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
    pub published_at_epoch: Option<i64>,
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
}

/// GemmaNarrative is one object the model returns. `#[serde(default)]` per field mirrors Go's
/// `encoding/json` tolerance of missing fields; an explicit `"articles": null` (or a non-int element)
/// makes serde skip the object at parse — net-identical to Go, which keeps it then drops it in
/// grounding for having no valid article (either way it is excluded).
#[derive(Clone, Debug, Default, Deserialize)]
struct GemmaNarrative {
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
    narratives: Vec<GemmaNarrative>,
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
    let rows: Vec<(i64, String, String, String, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT a.id, a.title, COALESCE(a.description, ''), COALESCE(a.source, ''),
               EXTRACT(EPOCH FROM a.published_at)::bigint
        FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        WHERE nae.entity_type = $1 AND nae.entity_id = $2 AND nae.sport = $3
          AND nae.vetted IS TRUE
          AND (a.published_at IS NULL OR a.published_at > NOW() - make_interval(secs => $4))
        ORDER BY COALESCE(a.published_at, a.fetched_at) DESC
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
            |(id, title, description, source, published_at_epoch)| CorpusItem {
                id,
                title,
                description,
                source,
                published_at_epoch,
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
fn parse_narratives(raw: &str) -> (Vec<GemmaNarrative>, bool) {
    let mut out: Vec<GemmaNarrative> = Vec::new();
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
                            if let Ok(n) = serde_json::from_str::<GemmaNarrative>(txt) {
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
    parsed: &[GemmaNarrative],
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
        out.push(Narrative {
            title: title.to_string(),
            body: body.to_string(),
            impact,
            impact_components: components,
            input_news_ids: ids,
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
/// plus the n3 options and the exact wire body. NO model call — these are the parity axes (the L2
/// finding: the storyline grouping is not a temp-0 parity axis). The role is [`Role::EmotionalNews`]
/// (the news/transfer reasoner — narratives shares it with vibe/transfers).
pub async fn build_narratives_request(
    hx: &Harness,
    req: &NarrativesReq,
    temperature: f64,
) -> Result<NarrativesBuild> {
    let sport_up = req.sport.to_uppercase();

    let corpus = load_vetted_corpus(&hx.pool, &req.entity_type, req.entity_id, &sport_up).await?;
    // No corpus → the NULL-narrative marker path (no model call).
    if corpus.is_empty() {
        return Ok(NarrativesBuild::NoCorpus);
    }
    let original_corpus_size = corpus.len();
    let corpus = dedup_corpus(hx, corpus).await?;

    // Vetted transfer rumors ground any transfer/trade storyline. Best-effort: a heat-read failure
    // must NEVER block the narrative (the corpus is the primary signal) — warn and continue ungrounded.
    let heat: Vec<HeatItem> =
        match load_transfer_heat(&hx.pool, &req.entity_type, req.entity_id, &sport_up).await {
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
        };

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
    /// True only for the PRE-model no-corpus path (logging/inspection). A post-model empty grounding
    /// also persists a marker, distinguished by `narratives.is_empty()` with `built_prompt: Some`.
    pub skipped_no_corpus: bool,
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
                skipped_no_corpus: true,
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
        skipped_no_corpus: false,
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
/// `news_narratives.go::persist`: `source_attribution` is always NULL, `trigger_payload` is the
/// caller's value (the drain passes jsonb `null` — Go marshals the nil trigger map). Written +
/// compiles; NOT run in the offline parity bin — its first live run is the Step-3 cutover.
pub async fn persist_narratives(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &NarrativesOutput,
) -> Result<()> {
    let trigger_json = trigger_payload.to_string();

    // NOW() is constant within a transaction (transaction_timestamp), so every row of this generation
    // shares one generated_at — Go's `res.GeneratedAt`, without needing a datetime crate to bind it.
    let mut tx = pool.begin().await.context("begin narratives tx")?;

    const INSERT: &str = r#"
        INSERT INTO news_summaries (
            entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components,
            source_attribution, input_news_ids, model_version, prompt_version, generated_at
        ) VALUES ($1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, NULL,$10, $11,$12,NOW())"#;

    if out.narratives.is_empty() {
        // No-narratives marker row.
        sqlx::query(INSERT)
            .bind(entity_type)
            .bind(entity_id)
            .bind(sport)
            .bind(trigger_type)
            .bind(&trigger_json)
            .bind(Option::<String>::None) // narrative_title
            .bind(Option::<String>::None) // body
            .bind(Option::<i16>::None) // impact
            .bind("{}") // impact_components
            .bind(Vec::<i64>::new()) // input_news_ids
            .bind(&out.model)
            .bind(NARRATIVES_PROMPT_VERSION)
            .execute(&mut *tx)
            .await
            .context("persist narratives marker")?;
    } else {
        for n in &out.narratives {
            let components_json = n.impact_components.to_string();
            sqlx::query(INSERT)
                .bind(entity_type)
                .bind(entity_id)
                .bind(sport)
                .bind(trigger_type)
                .bind(&trigger_json)
                .bind(Some(&n.title))
                .bind(Some(&n.body))
                .bind(Some(n.impact as i16))
                .bind(&components_json)
                .bind(&n.input_news_ids)
                .bind(&out.model)
                .bind(NARRATIVES_PROMPT_VERSION)
                .execute(&mut *tx)
                .await
                .context("persist narrative row")?;
        }
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
// Handler — REGISTERED but NOT enabled until the Step-3 cutover (Go Drainer still owns the stage).
// ---------------------------------------------------------------------------

/// NarrativesHandler drains the durable `narratives` stage: read the vetted corpus, (live) dedup it,
/// group it into storylines with the model, score each deterministically, and persist one
/// news_summaries row per narrative (or a marker). REGISTERED but NOT enabled until the narratives
/// cutover (Step 3): the Go Drainer still owns this stage, so running both would double-claim the one
/// GPU. Unlike rating, narratives IS a `pipeline_work` stage (`Stage::Narratives`).
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
        let name =
            vibe::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;
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
            published_at_epoch: epoch,
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

    // --- build_narratives_prompt byte-fixtures: the deterministic parity axis. The expected strings
    // are computed by hand from Go's buildNarrativesPrompt, so a drift in the Rust assembly fails here
    // (offline, no model) before the live diff ever runs. -------------------------------------------

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
            GemmaNarrative {
                title: " Title ".to_string(), // trimmed
                body: "Body".to_string(),
                articles: vec![1, 1, 2, 9, 0, -3], // dup 1, out-of-range 9/0/-3 dropped
            },
            GemmaNarrative {
                title: "".to_string(), // empty title → dropped
                body: "x".to_string(),
                articles: vec![1],
            },
            GemmaNarrative {
                title: "no articles".to_string(),
                body: "y".to_string(),
                articles: vec![9, 0], // all out of range → ungrounded → dropped
            },
        ];
        let out = ground_narratives(&parsed, &news, 10_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Title");
        assert_eq!(out[0].input_news_ids, vec![100, 101]); // 1→id100, 2→id101, dup/oob removed
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
