//! Vibe stage — the first ported derivation handler (Phase 1 beachhead).
//!
//! Rust implementation of the vibe stage. The Go source provided the original machinery spec:
//!   - `go/internal/ml/vibe.go`         — Generate, prompt assembly, parsing, persist
//!   - `go/internal/ml/transfer_heat.go` — the shared transfer-heat primitive
//!   - `go/internal/derive/derive.go`    — drainVibe: queue Item → request, downstream hand-off
//!
//! The deterministic loaders, prompt assembly, parser, and persist path live here so prompt changes
//! are versioned and inspectable.
//!
//! Fail-closed semantics reproduced verbatim: when an entity has NO narratives AND no
//! transfer heat, we skip the model and write a NULL-sentiment marker row (the read
//! path returns "no data"). A completed vibe enqueues Momentum before the terminal Sigil
//! convergence.
//!
//! F2 (2026-07-12) gives vibe a real debounce: the handler hashes the MATERIAL inputs only —
//! latest narrative titles/impacts/trajectories + transfer-heat facts, no prose, no
//! timestamps (mirroring narratives' key discipline) — and skips the GPU call when the
//! latest `vibe_scores` row carries the same `input_hash` (mig 147). Marker rows carry the
//! empty-material hash too, so quiet entities debounce instead of re-marking every cycle.
//! On a debounce-skip the handler still enqueues Momentum (hash-gated and cheap) — the same
//! self-healing shape as sigil's skip-path oracle enqueue. The offline parity harness
//! (`bin/parity`) deliberately does NOT debounce: a parity run must always exercise the model.

use crate::corpus::{
    dedupe_i64, load_transfer_heat, lookup_entity_name, write_heat_lines, HeatItem,
};
use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::{trajectory_label, DEFAULT_TRAJECTORY};
use crate::util::{go_json_string, hash_components, truncate, truncate_bytes};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

/// Prompt version for the Vibe sentiment + felt-read contract.
pub const VIBE_PROMPT_VERSION: &str = "v11"; // v11: heat lines carry the transfer lens summary + confidence

/// Output contract captured separately in the Phase 2 diagnostic ledger.
pub const VIBE_OUTPUT_CONTRACT_VERSION: &str = "vibe-score-v1";

/// Production sentiment temperature (vibe.go uses 0.7). The parity harness overrides
/// this with an explicit 0.
pub const VIBE_TEMPERATURE: f64 = 0.7;

/// Token cap for the two-line answer.
pub const VIBE_NUM_PREDICT: i32 = 512;

/// Body truncation in the prompt — mirrors `truncate(n.body, 280)` in vibe.go.
const BODY_TRUNCATE: usize = 280;

/// System prompt for the Vibe sentiment + felt-read contract.
pub const VIBE_SYSTEM_PROMPT: &str = r#"Task: produce a sentiment score and a short felt read from the supplied narratives and transfer/trade activity.

Voice: direct, sports-literate, grounded. No hype, no melodrama, no invented drama.

SCORE (1-100):
- 1 = grim or in freefall.
- 50 = quiet, unclear, or genuinely mixed.
- 100 = euphoric or surging.
- Weigh narratives by impact.
- Transfer/trade activity is energy, not automatically good or bad.
- If little is happening, stay near 50.

VIBE:
- One or two sentences. Use three only for a truly major multi-strand moment.
- Name the actual players, clubs, moves, or numbers behind the dominant threads.
- Do not list every minor item.
- Do not use generic phrases when the signals give specifics.
- Ground every claim in the supplied signals.

Reply with exactly these two lines:
SCORE: <integer 1-100>
VIBE: <the felt read>"#;

/// One narrative from the entity's latest generation (news_summaries).
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub trajectory: String,
    pub topic_heat: i32,
    /// Corroboration weight from the narratives stage (Phase 1): how many distinct sources
    /// backed the storyline. Was persisted from day one and read by nothing downstream.
    pub source_count: i32,
    /// Whole days since the storyline's freshest source (`source_latest_at`), when known.
    pub source_age_days: Option<i32>,
}

/// The result of running the vibe core for one entity, before persistence. Captures
/// the production row payload for `vibe_scores`; parity-only prompt/body capture
/// lives in `src/bin/parity.rs`.
#[derive(Clone, Debug)]
pub struct VibeOutput {
    /// `None` ⇒ no-corpus NULL marker (no model call was made).
    pub sentiment: Option<i32>,
    /// The one-sentence felt read; `None` when empty (the column is nullable).
    pub vibe_prompt: Option<String>,
    /// Provenance: the narratives' source article ids, deduped in first-seen order.
    pub input_news_ids: Vec<i64>,
    /// The canonical material-inputs JSON — the pre-image of `input_hash` (F2).
    pub input_components_json: String,
    /// SHA-256 (128-bit hex prefix) of `input_components_json` — the debounce key. Always
    /// real: the no-corpus marker carries the empty-material hash so quiet entities debounce
    /// too (the narratives precedent).
    pub input_hash: String,
    /// no-corpus → the configured model name; scored → the model echoed in the response.
    pub model: String,
    pub prompt_version: &'static str,
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
}

/// vibe_version fingerprints a vibe result for the sigil queue's input_version, exactly
/// as `vibeVersion` in derive.go: `s<sentiment>` (`s0` for the no-corpus marker). Coarse
/// on purpose — the SigilGenerator's own pillar input-hash is the real convergence gate;
/// this only keeps the queue row's reopen/dedupe sane.
pub fn vibe_version(out: &VibeOutput) -> String {
    format!("s{}", out.sentiment.unwrap_or(0))
}

impl VibeOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope (Plan §1.6).
    /// Vibe debounces since F2 (mig 147), so `input_hash` is carried; the scored row and the
    /// no-corpus marker produce the same envelope shape, differing only in the values they carry.
    fn provenance(&self) -> Provenance {
        Provenance {
            model_version: self.model.clone(),
            prompt_version: self.prompt_version,
            input_ids: self.input_news_ids.clone(),
            input_hash: Some(self.input_hash.clone()),
            trigger_payload: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Corpus loaders — byte-for-byte the same SQL the Go stage runs.
// ---------------------------------------------------------------------------

/// load_latest_narratives returns the narratives from the entity's most recent
/// generation (news_summaries), hottest first, plus the deduped union of their source
/// article ids. Empty when the latest generation was a no-narratives marker (body NULL)
/// or the entity has none yet. Mirrors VibeGenerator.loadLatestNarratives.
pub async fn load_latest_narratives(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(Vec<Narrative>, Vec<i64>)> {
    // COALESCE(impact, 0): impact is int2 but the `0` literal is int4, so the result is
    // int4 → scan as i32 (matches Go scanning into `int`).
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, i32, Vec<i64>, String, i32, i32, Option<i32>)> =
        sqlx::query_as(
            r#"
        SELECT narrative_title, body, COALESCE(impact, 0), input_news_ids,
               COALESCE(trajectory, $4),
               COALESCE((
                   SELECT max(a.topic_heat)::int
                   FROM unnest(input_news_ids) AS nid(article_id)
                   JOIN news_articles a ON a.id = nid.article_id
               ), 1) AS topic_heat,
               COALESCE(source_count, 0) AS source_count,
               EXTRACT(day FROM NOW() - source_latest_at)::int AS source_age_days
        FROM news_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND body IS NOT NULL
          AND generated_at = (
              SELECT max(generated_at) FROM news_summaries
              WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          )
        ORDER BY impact DESC NULLS LAST
        "#,
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport)
        .bind(DEFAULT_TRAJECTORY)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load narratives {entity_type}/{entity_id}"))?;

    let mut narratives = Vec::with_capacity(rows.len());
    let mut ids: Vec<i64> = Vec::new();
    for (title, body, impact, mut nids, trajectory, topic_heat, source_count, source_age_days) in
        rows
    {
        ids.append(&mut nids);
        narratives.push(Narrative {
            title,
            body,
            impact,
            trajectory,
            topic_heat,
            source_count,
            source_age_days,
        });
    }
    Ok((narratives, dedupe_i64(ids)))
}

/// order_narratives orders the storylines for the prompt: topic heat → impact → title.
///
/// Phase 2 dropped the identity re-load + candle embed that used to compute a per-narrative
/// relevance ordering key: its only product was this sort's primary axis, the offline parity
/// bins (no embedder) never ran it, and every debounce-skipped wake paid it for nothing.
/// Topic heat is the durable relevance proxy (candle-clustered upstream, and already in both
/// the prompt tags and the debounce pre-image), so live ordering now matches what the parity
/// bins always produced.
fn order_narratives(narratives: &mut [Narrative]) {
    narratives.sort_by(|a, b| {
        b.topic_heat
            .cmp(&a.topic_heat)
            .then_with(|| b.impact.cmp(&a.impact))
            .then_with(|| a.title.cmp(&b.title))
    });
}


// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (F2, mig 147).
// ---------------------------------------------------------------------------

/// build_vibe_input_components is the canonical debounce pre-image: the latest narrative
/// titles/impacts/trajectories plus the transfer-heat facts in sigil's
/// `counterparty:heat:direction:stage` convention — the material signals behind the sentiment.
/// Same canonical-JSON discipline as `narratives::build_narratives_input_components`.
///
/// Deliberately EXCLUDED: narrative bodies (model prose — the F1 rule), `topic_heat` and
/// `relevance` (derived ordering signals that tick without the storylines moving),
/// `source_count`/`source_age_days` (corroboration/freshness are prompt-only, sigil's
/// precedent — an age ticking over a day boundary must not re-run the GPU), and the heat
/// summary/confidence (derived commentary, the narratives precedent). The three narrative
/// keys are ALWAYS present (even `[]`) so the no-corpus marker has a stable pre-image;
/// `transfer_heat` is conditional, matching the sigil/narratives convention.
pub fn build_vibe_input_components(narratives: &[Narrative], heat: &[HeatItem]) -> String {
    fn push_sorted_lines(out: &mut String, mut lines: Vec<String>) {
        lines.sort();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&go_json_string(line));
        }
    }

    let mut out = String::from("{\"narrative_impacts\":[");
    push_sorted_lines(
        &mut out,
        narratives
            .iter()
            .map(|n| format!("{}:{}", n.title, n.impact))
            .collect(),
    );
    out.push_str("],\"narrative_titles\":[");
    push_sorted_lines(&mut out, narratives.iter().map(|n| n.title.clone()).collect());
    out.push_str("],\"narrative_trajectories\":[");
    push_sorted_lines(
        &mut out,
        narratives
            .iter()
            .map(|n| format!("{}:{}", n.title, n.trajectory))
            .collect(),
    );
    out.push(']');
    if !heat.is_empty() {
        out.push_str(",\"transfer_heat\":[");
        push_sorted_lines(
            &mut out,
            heat.iter()
                .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
                .collect(),
        );
        out.push(']');
    }
    out.push('}');
    out
}

/// The loaded-and-weighted vibe context: everything the prompt needs plus the debounce key.
/// Splitting the load from the model call lets the production handler gate on `input_hash`
/// BEFORE paying for the GPU (the parity harness skips the gate — it must always run).
pub struct VibeContext {
    /// Ordered for the prompt (topic heat → impact → title).
    pub narratives: Vec<Narrative>,
    pub heat: Vec<HeatItem>,
    /// Deduped union of the narratives' source article ids (provenance).
    pub news_ids: Vec<i64>,
    pub input_components_json: String,
    pub input_hash: String,
}

impl VibeContext {
    /// No derived signal at all → the no-corpus marker path (no model call).
    pub fn empty(&self) -> bool {
        self.narratives.is_empty() && self.heat.is_empty()
    }
}

/// load_vibe_context runs the deterministic prefix: validate, load narratives + heat
/// concurrently, weight/order the narratives for the prompt, and compute the material-only
/// debounce key. NO model call.
pub async fn load_vibe_context(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
) -> Result<VibeContext> {
    if entity_id <= 0 || entity_name.is_empty() || sport_raw.is_empty() || entity_type.is_empty() {
        bail!("vibe: entity context incomplete");
    }
    // Reads use the upper-cased sport; the prompt uses the original-case value (req.Sport).
    let sport = sport_raw.to_uppercase();

    // Independent reads (news_summaries vs transfer_rumors, no data dependency) — run them
    // concurrently (plan A3).
    let ((mut narratives, news_ids), heat) = tokio::try_join!(
        load_latest_narratives(&hx.pool, entity_type, entity_id, &sport),
        load_transfer_heat(&hx.pool, entity_type, entity_id, &sport),
    )?;
    order_narratives(&mut narratives);

    // Hash BEFORE any model involvement: the pre-image is order-insensitive (sorted lines),
    // so the weighting above cannot perturb it.
    let input_components_json = build_vibe_input_components(&narratives, &heat);
    let input_hash = hash_components(&input_components_json);

    Ok(VibeContext {
        narratives,
        heat,
        news_ids,
        input_components_json,
        input_hash,
    })
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// build_sentiment_prompt assembles the user prompt. `sport` is the original-case value used in
/// the prompt; the SQL reads use the upper-cased form.
pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat: &[HeatItem],
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {} {} ({})\n",
        title_first(entity_type),
        entity_name,
        sport
    ));

    b.push_str(
        "\nNarratives forming around them (ordered by relevance/topic heat; impact in brackets):\n",
    );
    if narratives.is_empty() {
        b.push_str("- (none this cycle)\n");
    } else {
        for n in narratives {
            let mut tags = format!(
                "{}, {}, topic heat {}",
                n.impact,
                trajectory_label(&n.trajectory),
                n.topic_heat
            );
            // Corroboration + freshness (Phase 1): the felt read should weigh a 5-source
            // storyline from today differently than a single stale one.
            if n.source_count > 0 {
                tags.push_str(&format!(", {} sources", n.source_count));
            }
            if let Some(d) = n.source_age_days {
                tags.push_str(&format!(", latest {d}d ago"));
            }
            b.push_str(&format!(
                "- [{tags}] {}: {}\n",
                n.title,
                truncate_bytes(&n.body, BODY_TRUNCATE)
            ));
        }
    }

    b.push_str("\nCurrent transfer/trade activity (heat 0-100):\n");
    if heat.is_empty() {
        b.push_str("- (none)\n");
    } else {
        write_heat_lines(&mut b, heat);
    }

    b.push_str("\nRespond now (SCORE line, then VIBE line).");
    b
}

/// title_first upper-cases the first character, mirroring `strings.Title` for the
/// single-word entity types ("player" → "Player", "team" → "Team").
fn title_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parseSentimentAndPrompt + parseSentiment.
// ---------------------------------------------------------------------------

/// parse_sentiment_and_prompt extracts the SCORE (1-100) and the one-line VIBE from the
/// model's two-line v6 reply. The score falls back to the first integer anywhere
/// (format drift); the prompt is "" when absent. Mirrors `parseSentimentAndPrompt`.
pub fn parse_sentiment_and_prompt(raw: &str) -> Result<(i32, String)> {
    let mut score: i32 = 0;
    let mut prompt = String::new();
    let lines: Vec<&str> = raw.trim().split('\n').collect();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let up = t.to_uppercase();
        if score == 0 && up.starts_with("SCORE:") {
            // "SCORE:" is ASCII (6 bytes) regardless of case, so t[6..] is a boundary.
            let rest = t[6..].trim();
            if let Ok(n) = rest.parse::<i64>() {
                score = n.clamp(1, 100) as i32;
            }
        } else if prompt.is_empty() && up.starts_with("VIBE:") {
            prompt = t[5..].trim().to_string();
            for extra in &lines[i + 1..] {
                let e = extra.trim();
                if !e.is_empty() {
                    prompt.push(' ');
                    prompt.push_str(e);
                }
            }
        }
    }

    if score == 0 {
        // No SCORE: label parsed — fall back to the first integer anywhere.
        score = parse_sentiment(raw)?;
    }
    Ok((score, prompt))
}

/// parse_sentiment pulls the first run of ASCII digits out of the response and clamps it
/// into 1-100; errors only when there are no digits at all (or the run overflows, as
/// Go's strconv.Atoi does). Mirrors `parseSentiment` (the `\d+` regex path).
fn parse_sentiment(raw: &str) -> Result<i32> {
    let bytes = raw.as_bytes();
    let start = match bytes.iter().position(|b| b.is_ascii_digit()) {
        Some(i) => i,
        None => return Err(anyhow!("no digit in response")),
    };
    let end = bytes[start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .map(|off| start + off)
        .unwrap_or(bytes.len());
    let digits = &raw[start..end];
    match digits.parse::<i64>() {
        Ok(n) => Ok(n.clamp(1, 100) as i32),
        Err(_) => Err(anyhow!("parse digit {digits:?}")),
    }
}

/// VibeReply is the validated two-line answer — the SCORE (1-100) and the one-line felt
/// read. The vibe Extract output shape (the `T` in `Parser<T>` / `Extracted<T>`).
#[derive(Clone, Debug)]
pub struct VibeReply {
    pub sentiment: i32,
    pub vibe_prompt: String,
}

/// VibeParser is the vibe stage's `Parser` plug-in: it wraps `parse_sentiment_and_prompt`
/// behind the capability library's `Parser<T>` seam.
/// It never returns the fail-closed `Ok(None)` — vibe's only fail-closed path is the
/// no-corpus short-circuit *before* the model call (a NULL marker), so an unparseable reply
/// is a genuine failure → `Err` → the work item backs off, exactly as the Go stage does.
pub struct VibeParser;

impl Parser<VibeReply> for VibeParser {
    fn parse(&self, raw: &str) -> Result<Option<VibeReply>> {
        let (sentiment, vibe_prompt) = parse_sentiment_and_prompt(raw)
            .with_context(|| format!("parse sentiment (raw={:?})", truncate(raw, 120)))?;
        Ok(Some(VibeReply {
            sentiment,
            vibe_prompt,
        }))
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// generate_vibe runs the full vibe derivation for one entity at the given temperature and
/// returns the un-persisted result. This is the L1 composition — `route(EmotionalNews) +
/// extract(VibeParser)` — over the same loaders + prompt: load the context, short-circuit
/// to the no-corpus marker when it is empty, else build the prompt and `extract`. The
/// production handler calls `load_vibe_context` itself (to debounce before the model call)
/// and then `generate_vibe_from_context`; this convenience wrapper is the undebounced
/// load-and-generate composition.
pub async fn generate_vibe(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<VibeOutput> {
    let ctx = load_vibe_context(hx, entity_type, entity_id, entity_name, sport_raw).await?;
    let (out, _, _) =
        generate_vibe_from_context(hx, entity_type, entity_name, sport_raw, ctx, temperature, false)
            .await?;
    Ok(out)
}

/// generate_vibe_parity runs the same core as production while returning the
/// parity-era prompt and request-body axes. NO debounce — a parity run must always
/// exercise the model. Removed with the parity bins (see plan C1).
pub async fn generate_vibe_parity(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    entity_name: &str,
    sport_raw: &str,
    temperature: f64,
) -> Result<(VibeOutput, Option<String>, Option<serde_json::Value>)> {
    let ctx = load_vibe_context(hx, entity_type, entity_id, entity_name, sport_raw).await?;
    generate_vibe_from_context(hx, entity_type, entity_name, sport_raw, ctx, temperature, true)
        .await
}

async fn generate_vibe_from_context(
    hx: &Harness,
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    ctx: VibeContext,
    temperature: f64,
    capture_parity: bool,
) -> Result<(VibeOutput, Option<String>, Option<serde_json::Value>)> {
    // No derived signal (no narratives AND no transfer heat) → no rating. Persist a
    // NULL-sentiment marker (handled by the caller); the read path returns "no data". No
    // model call is made, so the marker's model_version is the role's configured model.
    // The marker still carries the (empty-material) hash so quiet entities debounce.
    if ctx.empty() {
        return Ok((
            VibeOutput {
                sentiment: None,
                vibe_prompt: None,
                input_news_ids: Vec::new(),
                input_components_json: ctx.input_components_json,
                input_hash: ctx.input_hash,
                model: hx.router.for_role(Role::EmotionalNews).model().to_string(),
                prompt_version: VIBE_PROMPT_VERSION,
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
            },
            None,
            None,
        ));
    }

    let prompt =
        build_sentiment_prompt(entity_type, entity_name, sport_raw, &ctx.narratives, &ctx.heat);
    let opts = GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: VIBE_NUM_PREDICT,
        num_ctx: 0,
        json_mode: false,
    format_schema: None,
    };

    // vibe = route(EmotionalNews) + extract(VibeParser). The fail-closed contract lives in
    // the parser: an unparseable reply surfaces as its `Err` (item fails + backs off), and
    // `extract` records the exact wire body it sent.
    let extracted = hx
        .extract(Role::EmotionalNews, &prompt, &opts, &VibeParser)
        .await?;

    // VibeParser only ever returns `Ok(Some)` on success (vibe's no-corpus marker is the
    // pre-model short-circuit above, not a parser fail-closed), so a `None` here would be a
    // contract violation — fail the item rather than fabricate a row.
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("vibe: parser returned no value"))?;

    let built_prompt = extracted.built_prompt;
    let request_body = extracted.request_body;
    let eval_count = extracted.eval_count;
    let wall_ms = extracted.wall_ms;

    Ok((
        VibeOutput {
            sentiment: Some(reply.sentiment),
            vibe_prompt: if reply.vibe_prompt.is_empty() {
                None
            } else {
                Some(reply.vibe_prompt)
            },
            input_news_ids: ctx.news_ids,
            input_components_json: ctx.input_components_json,
            input_hash: ctx.input_hash,
            model: extracted.model,
            prompt_version: VIBE_PROMPT_VERSION,
            built_prompt: Some(built_prompt.clone()),
            request_body: Some(request_body.clone()),
            eval_count: Some(eval_count),
            wall_ms: Some(wall_ms),
        },
        capture_parity.then_some(built_prompt),
        capture_parity.then_some(request_body),
    ))
}

fn vibe_included_evidence(out: &VibeOutput) -> serde_json::Value {
    serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&out.input_components_json)
            .unwrap_or_else(|_| serde_json::json!({"raw_input_components": out.input_components_json})),
        "input_news_ids": &out.input_news_ids,
        "sentiment": out.sentiment,
        "vibe_prompt": &out.vibe_prompt,
    })
}

fn vibe_excluded_evidence(out: &VibeOutput) -> serde_json::Value {
    if out.built_prompt.is_none() {
        serde_json::json!([{
            "reason": "no_latest_narratives_or_transfer_heat",
        }])
    } else {
        serde_json::json!([])
    }
}

fn vibe_parser_outcome(out: &VibeOutput) -> &'static str {
    if out.built_prompt.is_none() {
        "no_call"
    } else {
        "parsed"
    }
}

/// persist_to_vibe_scores writes one row to the LIVE vibe_scores table — both the scored
/// row and the no-corpus NULL marker, which differ only in the bound values. Mirrors
/// persistSentiment / persistNoCorpus: trigger_type 'periodic', trigger_payload the JSON
/// `null` (marshal of a nil trigger map), empty felt-read stored as NULL.
async fn persist_to_vibe_scores(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &VibeOutput,
) -> Result<i64> {
    // Route the moat fields through the shared Provenance envelope — input_hash included
    // since F2 (mig 147); the typed INSERT stays the stage's own (Postgres-as-serializer).
    let entity_id = item.entity_id_i32()?;
    let prov = out.provenance();
    let sentiment: Option<i16> = out.sentiment.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO vibe_scores (
            entity_type, entity_id, sport,
            trigger_type, trigger_payload,
            sentiment, prompt, input_news_ids,
            model_version, prompt_version, input_hash
        ) VALUES ($1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8,$9)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(sentiment)
    .bind(out.vibe_prompt.as_deref())
    .bind(prov.input_ids.as_slice())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(prov.input_hash.as_deref())
    .fetch_one(pool)
    .await
    .context("persist vibe")?;
    Ok(row.get("id"))
}

/// VibeHandler drains the durable `vibe` stage: read the fresh narratives + heat, score
/// with the model, persist to vibe_scores, and enqueue the Momentum gate before completing.
/// This is the production path for the Phase 2 cutover (registered in main.rs); the Phase 1
/// parity harness reuses the same core but writes the shadow table.
pub struct VibeHandler;

impl VibeHandler {
    pub fn new() -> Self {
        VibeHandler
    }
}

impl Default for VibeHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for VibeHandler {
    fn stage(&self) -> Stage {
        Stage::Vibe
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // nameOf: the name lookup uses the queue's raw sport value (drainVibe).
        let name = lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport).await?;
        let sport = item.sport.to_uppercase();

        // F2: gate on the material-only input_hash BEFORE the model call. Vibe is
        // entity-scoped — no season in its key.
        let ctx = load_vibe_context(hx, &item.entity_type, entity_id, &name, &item.sport).await?;
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: None,
        };
        if hx
            .debounce_unchanged("vibe_scores", &key, &ctx.input_hash)
            .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "vibe: debounce-skip, material inputs unchanged"
            );
            // Still hand off: the momentum enqueue is hash-gated and cheap, so a previously
            // lost hand-off self-heals as a no-op — the same shape as sigil's skip-path
            // oracle enqueue.
            crate::momentum::enqueue_momentum_if_needed(hx, &item.entity_type, entity_id, &sport)
                .await?;
            return Ok(());
        }

        let (out, _, _) = generate_vibe_from_context(
            hx,
            &item.entity_type,
            &name,
            &item.sport,
            ctx,
            VIBE_TEMPERATURE,
            false,
        )
        .await?;
        let product_row_id = persist_to_vibe_scores(&hx.pool, item, &sport, &out).await?;
        insert_cognition_ledger_best_effort(
            &hx.pool,
            CognitionLedgerEntry {
                stage: "vibe".to_string(),
                lens: "vibe".to_string(),
                role: Role::EmotionalNews.as_str().to_string(),
                entity_type: item.entity_type.clone(),
                entity_id,
                sport: sport.clone(),
                pair_entity_type: None,
                pair_entity_id: None,
                trigger_type: "periodic".to_string(),
                trigger_payload: serde_json::Value::Null,
                product_table: "vibe_scores".to_string(),
                product_row_ids: vec![product_row_id],
                model_version: out.model.clone(),
                prompt_version: out.prompt_version.to_string(),
                output_contract_version: VIBE_OUTPUT_CONTRACT_VERSION.to_string(),
                input_ids: out.input_news_ids.clone(),
                input_hash: Some(out.input_hash.clone()),
                request_body: out.request_body.clone(),
                built_prompt: out.built_prompt.clone(),
                included_evidence: vibe_included_evidence(&out),
                excluded_evidence: vibe_excluded_evidence(&out),
                context_budget: serde_json::json!({
                    "num_predict": VIBE_NUM_PREDICT,
                    "eval_count": out.eval_count,
                    "wall_ms": out.wall_ms,
                }),
                parser_outcome: vibe_parser_outcome(&out).to_string(),
            },
        )
        .await;

        // Vibe now feeds Momentum first; Momentum persists the generated trajectory card and then
        // enqueues Sigil if the Momentum context actually moved.
        if !crate::momentum::enqueue_momentum_if_needed(hx, &item.entity_type, entity_id, &sport)
            .await?
        {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "vibe: momentum enqueue skipped unchanged/empty context"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_line_reply() {
        let (score, vibe) =
            parse_sentiment_and_prompt("SCORE: 73\nVIBE: Quietly surging into the playoff race.")
                .unwrap();
        assert_eq!(score, 73);
        assert_eq!(vibe, "Quietly surging into the playoff race.");
    }

    #[test]
    fn clamps_and_joins_trailing_vibe_lines() {
        let (score, vibe) =
            parse_sentiment_and_prompt("SCORE: 250\nVIBE: line one\nline two").unwrap();
        assert_eq!(score, 100);
        assert_eq!(vibe, "line one line two");
    }

    #[test]
    fn falls_back_to_first_integer() {
        let (score, vibe) = parse_sentiment_and_prompt("the vibe is about 64 today").unwrap();
        assert_eq!(score, 64);
        assert_eq!(vibe, "");
    }

    #[test]
    fn errors_without_digits() {
        assert!(parse_sentiment_and_prompt("no number here").is_err());
    }

    #[test]
    fn builds_prompt_with_empty_sections() {
        let p = build_sentiment_prompt("player", "Test Player", "NBA", &[], &[]);
        assert_eq!(
            p,
            "Entity: Player Test Player (NBA)\n\nNarratives forming around them (ordered by relevance/topic heat; impact in brackets):\n- (none this cycle)\n\nCurrent transfer/trade activity (heat 0-100):\n- (none)\n\nRespond now (SCORE line, then VIBE line)."
        );
    }

    #[test]
    fn dedupes_preserving_order() {
        assert_eq!(dedupe_i64(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[test]
    fn vibe_parser_wraps_valid_reply_as_some() {
        let reply = VibeParser
            .parse("SCORE: 73\nVIBE: Quietly surging into the playoff race.")
            .unwrap()
            .expect("a valid reply is Some, never the fail-closed None");
        assert_eq!(reply.sentiment, 73);
        assert_eq!(reply.vibe_prompt, "Quietly surging into the playoff race.");
    }

    #[test]
    fn vibe_parser_errors_without_digits() {
        // No digit anywhere ⇒ Err (retry/back-off), NOT Ok(None): vibe's only fail-closed
        // path is the pre-model no-corpus marker, never an unparseable reply.
        assert!(VibeParser.parse("no number here").is_err());
    }

    fn test_narrative(title: &str, impact: i32, trajectory: &str) -> Narrative {
        Narrative {
            title: title.into(),
            // Non-empty prose + derived/freshness signals on purpose: the goldens below
            // prove NONE of them enter the hash pre-image (F2 material-only debounce).
            body: "long derived prose that must never enter the hash".into(),
            impact,
            trajectory: trajectory.into(),
            topic_heat: 42,
            source_count: 5,
            source_age_days: Some(1),
        }
    }

    #[test]
    fn input_components_are_material_only_and_sorted() {
        // Two narratives OUT of sorted order (the pre-image sorts, so weighting can't
        // perturb the hash), plus one heat line in the sigil convention.
        let narratives = vec![
            test_narrative("Trade buzz", 7, "heating_up"),
            test_narrative("Alpha story", 3, "developing_story"),
        ];
        let heat = vec![HeatItem {
            counterparty: "Arsenal".into(),
            heat: 40,
            stage: "speculation".into(),
            direction: "incoming".into(),
            // Derived commentary — excluded (the narratives precedent).
            summary: "derived commentary".into(),
            confidence: Some(0.8),
        }];
        assert_eq!(
            build_vibe_input_components(&narratives, &heat),
            r#"{"narrative_impacts":["Alpha story:3","Trade buzz:7"],"narrative_titles":["Alpha story","Trade buzz"],"narrative_trajectories":["Alpha story:developing_story","Trade buzz:heating_up"],"transfer_heat":["Arsenal:40:incoming:speculation"]}"#
        );
    }

    #[test]
    fn input_components_empty_material_is_stable() {
        // The no-corpus marker pre-image: narrative keys always present as [], NO
        // transfer_heat key (sigil convention) — so quiet entities debounce on a stable hash.
        assert_eq!(
            build_vibe_input_components(&[], &[]),
            r#"{"narrative_impacts":[],"narrative_titles":[],"narrative_trajectories":[]}"#
        );
    }

    #[test]
    fn input_hash_ignores_narrative_ordering() {
        // order_narratives reorders for the prompt (topic-heat/impact/title); the
        // debounce key must not care.
        let a = test_narrative("Alpha story", 3, "developing_story");
        let b = test_narrative("Trade buzz", 7, "heating_up");
        let forward = build_vibe_input_components(&[a.clone(), b.clone()], &[]);
        let reversed = build_vibe_input_components(&[b, a], &[]);
        assert_eq!(hash_components(&forward), hash_components(&reversed));
    }
}
