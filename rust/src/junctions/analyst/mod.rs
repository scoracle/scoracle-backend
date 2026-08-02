//! Momentum stage — the generated trajectory card over PEAK, Vibe, and deterministic slopes.
//!
//! `momentum_scores` remains the numeric backbone for leaderboards and ranking. This stage adds the
//! client-surfaced read: a direction, a signed ±5 conviction, and the blurb with provenance,
//! persisted to `momentum_summaries` and consumed by the Oracle as the Momentum pillar.
//!
//! As of s11 BOTH numbers are computed here and only the blurb comes from the model — see
//! [`momentum_conviction_from_score`] for why the magnitude stopped being asked for.

use crate::harness::{EntityKey, Harness, Parser};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::junctions::oracle::{self, SynthMomentum, SynthRating, SynthVibe};
use crate::stage::StageHandler;
use crate::util::{go_json_float, go_json_string, hash_components, round1};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::{debug, warn};

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{MOMENTUM_PROMPT_VERSION, MOMENTUM_SYSTEM_PROMPT, build_momentum_prompt};

/// Output contract captured separately in the diagnostic ledger.
pub const MOMENTUM_OUTPUT_CONTRACT_VERSION: &str = "momentum-summary-v1";

/// Keep Momentum on the incumbent stats route until a broader fixture set proves a split.
pub const MOMENTUM_TEMPERATURE: f64 = 0.3;

pub const MOMENTUM_NUM_PREDICT: i32 = 1200;

/// The steady band on the deterministic `momentum_score` (±100-scale: the average of the
/// vibe-sentiment delta and the rating-percentile delta over the window). |score| < band ⇒
/// steady; at or beyond ⇒ rising/falling by sign. ±10 blessed by Scott (Session D): a
/// 10-point percentile/sentiment move is a real story, smaller is noise. Measured on
/// 2026-07-14→16 live rows this yields ~33% steady / ~26% rising / ~41% falling.
pub const MOMENTUM_STEADY_BAND: f64 = 10.0;

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";

#[derive(Clone, Debug)]
pub struct MomentumContext {
    pub season: i32,
    pub rating: Option<SynthRating>,
    pub vibe: Option<SynthVibe>,
    pub snapshot: SynthMomentum,
    pub input_components_json: String,
    pub input_hash: String,
}

impl MomentumContext {
    fn empty(&self) -> bool {
        self.rating.is_none() && self.vibe.is_none() && self.snapshot.empty()
    }
}

/// The parsed model output — SCORE + READ only (s4). Direction left this contract in
/// Session D: it is a deterministic fact (`momentum_direction_from_score`), decided in
/// code and narrated by the model, exactly like sigil's omen.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The READ, and only the READ. s11 removed `score`: the Analyst voices the momentum, it
    /// does not decide it — see [`momentum_conviction_from_score`].
    pub blurb: String,
}

#[derive(Clone, Debug)]
pub struct MomentumOutput {
    pub direction: String,
    pub score: i32,
    pub blurb: String,
    pub season: i32,
    pub input_components_json: String,
    pub input_hash: String,
    pub model: String,
    pub prompt_version: &'static str,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub eval_count: i32,
    pub wall_ms: u64,
}

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        // Carry a raw excerpt in the error: a dozen live items sat failed with only
        // "invalid response", leaving nothing to diagnose which contract line broke.
        parse_momentum_reply(raw).map(Some).ok_or_else(|| {
            anyhow!(
                "momentum: invalid response (raw={:?})",
                crate::util::truncate_bytes(raw.trim(), 160)
            )
        })
    }
}

pub async fn load_momentum_snapshot(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(Option<f64>, i32, Option<f64>, i32, Option<f64>)> = sqlx::query_as(
        r#"
        SELECT vibe_slope::float8, vibe_samples,
               rating_slope::float8, rating_samples,
               momentum_score::float8
        FROM public.latest_momentum_scores_per_entity
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load momentum snapshot {entity_type}/{entity_id}"))?;

    Ok(row
        .map(
            |(vibe_slope, vibe_samples, rating_slope, rating_samples, momentum_score)| {
                SynthMomentum {
                    vibe_slope,
                    vibe_samples,
                    rating_slope,
                    rating_samples,
                    momentum_score,
                    ..SynthMomentum::default()
                }
            },
        )
        .unwrap_or_default())
}

pub async fn load_momentum_context(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<MomentumContext> {
    let season = oracle::resolve_season(&hx.pool, sport, None).await?;
    let (rating, vibe, snapshot) = tokio::try_join!(
        oracle::load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season)),
        oracle::load_vibe_pillar(&hx.pool, entity_type, entity_id, sport),
        load_momentum_snapshot(&hx.pool, entity_type, entity_id, sport),
    )?;
    let input_components_json =
        build_momentum_input_components(rating.as_ref(), vibe.as_ref(), &snapshot);
    let input_hash = hash_components(&input_components_json);
    Ok(MomentumContext {
        season,
        rating,
        vibe,
        snapshot,
        input_components_json,
        input_hash,
    })
}

pub async fn enqueue_momentum_if_needed(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let ctx = load_momentum_context(hx, entity_type, entity_id, &sport).await?;
    if ctx.empty() {
        return Ok(false);
    }
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.clone(),
        season: Some(ctx.season),
    };
    if hx
        .debounce_unchanged("momentum_summaries", &key, &ctx.input_hash)
        .await?
    {
        return Ok(false);
    }
    let it = Item {
        stage: Stage::Momentum,
        entity_type: entity_type.to_string(),
        entity_id: i64::from(entity_id),
        sport,
        input_version: Some(momentum_work_input_version(ctx.season, &ctx.input_hash)),
        attempts: 0,
    };
    work::enqueue(&hx.pool, &it).await?;
    Ok(true)
}

pub fn momentum_work_input_version(season: i32, input_hash: &str) -> String {
    format!("{MOMENTUM_WORK_PREFIX}{season}:{input_hash}")
}

/// The deterministic direction — the single author of `momentum_summaries.direction`
/// (Session D, North Star #4: deterministic facts are computed, models narrate). The
/// score is the ±100-scale signed slope average from `momentum_scores`; `None` (no
/// durable snapshot, ~3.5% of generations) is honestly "steady": with no measured
/// trajectory there is no measured move.
pub fn momentum_direction_from_score(momentum_score: Option<f64>) -> &'static str {
    match momentum_score {
        Some(s) if s >= MOMENTUM_STEADY_BAND => "rising",
        Some(s) if s <= -MOMENTUM_STEADY_BAND => "falling",
        _ => "steady",
    }
}

/// The deterministic CONVICTION — the ±5 signed magnitude that used to be asked of the model
/// and is now computed, completing the same pattern that already owns `direction`
/// (North Star #4: deterministic facts are computed, models narrate).
///
/// Why it moved. The Analyst was miscast as a seat that *generates* a number. It is not: the
/// number is already decided by the collision of the Scout rail and the emotional rails, and
/// arrives as the ±100 `momentum_score`. Asking the model to re-derive that magnitude on a ±5
/// scale duplicated information the system already had exactly — and it did not survive contact:
/// `ministral-3:14b` never left {-1, 0, 1} across 8 fixtures and THREE prompt revisions (s8, s9,
/// s10), so a genuine surge persisted as a 1. Sign disagreements were previously papered over by
/// clamping; magnitude collapse had no such defence and silently weakened the Oracle's Momentum
/// pillar. Computing it makes both failure classes structurally impossible rather than instructed
/// against, and leaves the Analyst doing the thing it is actually good at: voicing the read.
///
/// The ladder. `momentum_score` is the ±100-scale signed slope average; `MOMENTUM_STEADY_BAND`
/// (±10) already splits steady from rising/falling, and the bands below subdivide what is left.
/// Inside the steady band a lean past half the band still reads as ±1, which preserves the old
/// contract's "steady is -1..1". `None` (no durable snapshot, ~3.5% of generations) is honestly 0.
///
/// NOTE: these thresholds are reasoned, NOT calibrated — they were chosen without sight of the
/// live distribution. Worth checking against real rows before trusting the tails:
///   SELECT width_bucket(abs(momentum_score),0,100,10)*10 AS band, count(*)
///     FROM public.latest_momentum_scores_per_entity GROUP BY 1 ORDER BY 1;
pub fn momentum_conviction_from_score(momentum_score: Option<f64>) -> i32 {
    let Some(s) = momentum_score else { return 0 };
    let mag = s.abs();
    let sign = if s < 0.0 { -1 } else { 1 };
    let step = if mag < MOMENTUM_STEADY_BAND / 2.0 {
        return 0; // genuinely flat: no measured lean at all
    } else if mag < 20.0 {
        1 // covers the top half of the steady band AND the first rising/falling notch
    } else if mag < 35.0 {
        2
    } else if mag < 55.0 {
        3
    } else if mag < 80.0 {
        4
    } else {
        5
    };
    sign * step
}

fn build_momentum_input_components(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    // prompt_version joins the pre-image at s6 (the narratives M4 / vibe v13 pattern): an
    // s-bump changes every entity's hash once, forcing one regen as its pipeline next wakes.
    let mut pairs: Vec<(&'static str, String)> =
        vec![("prompt_version", go_json_string(MOMENTUM_PROMPT_VERSION))];
    if let Some(r) = rating {
        pairs.push(("divined_peak", go_json_string(&r.divined_peak)));
        pairs.push(("notability", r.notability.to_string()));
        pairs.push(("peak_trajectory", go_json_string(&r.peak_trajectory)));
        if !r.peak_trajectory_label.is_empty() {
            pairs.push((
                "peak_trajectory_label",
                go_json_string(&r.peak_trajectory_label),
            ));
        }
    }
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose stays in the PROMPT but out of the hash
        // (F1, material-only debounce): vibe generates at temp 0.7, so its prose changes on
        // every re-run even when material is byte-identical; hashing it cascaded
        // momentum→sigil→oracle regenerations on zero material change.
        pairs.push(("vibe_sentiment", v.sentiment.to_string()));
    }
    if let Some(s) = mom.rating_slope {
        pairs.push(("momentum_rating_slope", go_json_float(round1(s))));
        pairs.push(("momentum_rating_samples", mom.rating_samples.to_string()));
    }
    if let Some(s) = mom.vibe_slope {
        pairs.push(("momentum_vibe_slope", go_json_float(round1(s))));
        pairs.push(("momentum_vibe_samples", mom.vibe_samples.to_string()));
    }
    if let Some(score) = mom.momentum_score {
        pairs.push(("momentum_score", go_json_float(round1(score))));
    }
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::from("{");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&go_json_string(k));
        out.push(':');
        out.push_str(v);
    }
    out.push('}');
    out
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    let mut read_lines: Vec<String> = Vec::new();
    let mut in_read = false;

    for line in raw.lines() {
        // Strip Markdown decoration before matching: `**SCORE: -1**` does not start with
        // `SCORE:`, and on 2026-07-26 that rejected every reply from the post-split model.
        // See `util::strip_markdown_emphasis`.
        let trimmed_owned = crate::util::strip_markdown_emphasis(line);
        let trimmed = trimmed_owned.as_str();
        if trimmed.is_empty() {
            continue;
        }
        // s4 dropped the MOMENTUM line from the contract (direction is decided in code),
        // but a model echoing the decided direction back is not an error — skip the line.
        if strip_prefix_ci(trimmed, "MOMENTUM:").is_some() {
            in_read = false;
            continue;
        }
        // s11 dropped SCORE from the contract (the magnitude is computed, like the direction
        // before it). A model that still emits one is not an error — every prompt revision
        // through s10 asked for it, and cached/echoing output is harmless. Skip the line.
        if strip_prefix_ci(trimmed, "SCORE:").is_some() {
            in_read = false;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "READ:") {
            read_lines.push(rest.trim().to_string());
            in_read = true;
            continue;
        }
        if in_read {
            read_lines.push(trimmed.to_string());
        }
    }

    let blurb = clean_joined_lines(&read_lines);
    if blurb.is_empty() {
        return None;
    }
    Some(MomentumReply { blurb })
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &s[prefix.len()..])
}


fn clean_joined_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

async fn persist_momentum_summary(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &MomentumOutput,
) -> Result<i64> {
    let trigger_payload = serde_json::json!({});
    let row = sqlx::query(
        r#"
        INSERT INTO public.momentum_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            direction, score, blurb, input_components, input_hash,
            model_version, prompt_version, generated_at
        ) VALUES ($1,$2,$3,$4,'periodic',$5::jsonb,$6,$7,$8,$9::jsonb,$10,$11,$12,NOW())
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(out.season)
    .bind(&trigger_payload)
    .bind(&out.direction)
    .bind(out.score as i16)
    .bind(&out.blurb)
    .bind(&out.input_components_json)
    .bind(&out.input_hash)
    .bind(&out.model)
    .bind(out.prompt_version)
    .fetch_one(pool)
    .await
    .context("persist momentum summary")?;
    Ok(row.get("id"))
}


fn momentum_included_evidence(ctx: &MomentumContext) -> serde_json::Value {
    serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&ctx.input_components_json)
            .unwrap_or_else(|_| serde_json::json!({"raw_input_components": ctx.input_components_json})),
        "has_peak": ctx.rating.is_some(),
        "has_vibe": ctx.vibe.is_some(),
        "has_momentum_snapshot": !ctx.snapshot.empty(),
    })
}

fn momentum_excluded_evidence(ctx: &MomentumContext) -> serde_json::Value {
    serde_json::json!({
        "empty_context": ctx.empty(),
    })
}

pub struct MomentumHandler;

impl MomentumHandler {
    pub fn new() -> Self {
        MomentumHandler
    }
}

impl Default for MomentumHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for MomentumHandler {
    fn stage(&self) -> Stage {
        Stage::Momentum
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport)
                .await?;
        let ctx = load_momentum_context(hx, &item.entity_type, entity_id, &sport).await?;
        if ctx.empty() {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                "momentum: skipped empty context"
            );
            return Ok(());
        }
        // No handler-side debounce (Phase 2): every momentum enqueue goes through
        // enqueue_momentum_if_needed, which already empty-gates and debounces against the
        // latest row — the hash it carried into input_version is the admission ticket. The
        // recomputed ctx.input_hash still stamps provenance on the row actually generated.

        // Relational memory card (s5): load failure degrades to an unenriched prompt (the
        // n8/v12 discipline — the trajectory numbers are the primary signal, memory is
        // the arc context that keeps the READ entity-specific).
        let memory = match crate::junctions::journalist::load_entity_memory(
            &hx.pool,
            &sport,
            &item.entity_type,
            entity_id,
        )
        .await
        {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    entity_type = %item.entity_type,
                    entity_id,
                    sport = %sport,
                    error = %e,
                    "momentum: relational memory load failed (continuing without memory)"
                );
                None
            }
        };

        let prompt = build_momentum_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            ctx.rating.as_ref(),
            ctx.vibe.as_ref(),
            &ctx.snapshot,
            memory.as_deref(),
        );
        let opts = GenerateOptions {
            system: Some(MOMENTUM_SYSTEM_PROMPT.to_string()),
            temperature: Some(MOMENTUM_TEMPERATURE),
            num_predict: MOMENTUM_NUM_PREDICT,
            num_ctx: crate::route::VOICE_NUM_CTX,
            json_mode: false,
            format_schema: None,
            format_schema_raw: None,
        };
        let extracted = hx
            .extract(Role::MomentumLogic, &prompt, &opts, &MomentumParser)
            .await?;
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;

        // BOTH numbers are DECIDED here, not by the model (North Star #4). Direction has been
        // computed since s4; s11 moves the signed magnitude alongside it. The same decided
        // direction went into the prompt, so the READ narrates it by construction — and because
        // the score is now derived from the same `momentum_score` the direction came from, the
        // persisted row cannot tell two stories. There is nothing left to clamp.
        let direction = momentum_direction_from_score(ctx.snapshot.momentum_score);
        let score = momentum_conviction_from_score(ctx.snapshot.momentum_score);

        let out = MomentumOutput {
            direction: direction.to_string(),
            score,
            blurb: reply.blurb,
            season: ctx.season,
            input_components_json: ctx.input_components_json.clone(),
            input_hash: ctx.input_hash.clone(),
            model: extracted.model,
            prompt_version: MOMENTUM_PROMPT_VERSION,
            built_prompt: extracted.built_prompt,
            request_body: extracted.request_body,
            eval_count: extracted.eval_count,
            wall_ms: extracted.wall_ms,
        };
        let product_row_id = persist_momentum_summary(&hx.pool, item, &sport, &out).await?;
        insert_cognition_ledger_best_effort(
            &hx.pool,
            CognitionLedgerEntry {
                stage: "momentum".to_string(),
                lens: "momentum".to_string(),
                role: Role::MomentumLogic.as_str().to_string(),
                entity_type: item.entity_type.clone(),
                entity_id,
                sport: sport.clone(),
                pair_entity_type: None,
                pair_entity_id: None,
                trigger_type: "periodic".to_string(),
                trigger_payload: serde_json::json!({}),
                product_table: "momentum_summaries".to_string(),
                product_row_ids: vec![product_row_id],
                model_version: out.model.clone(),
                prompt_version: out.prompt_version.to_string(),
                output_contract_version: MOMENTUM_OUTPUT_CONTRACT_VERSION.to_string(),
                input_ids: Vec::new(),
                input_hash: Some(out.input_hash.clone()),
                request_body: Some(out.request_body.clone()),
                built_prompt: Some(out.built_prompt.clone()),
                included_evidence: momentum_included_evidence(&ctx),
                excluded_evidence: momentum_excluded_evidence(&ctx),
                context_budget: serde_json::json!({
                    "num_predict": MOMENTUM_NUM_PREDICT,
                    "eval_count": out.eval_count,
                    "wall_ms": out.wall_ms,
                    "decided_direction": direction,
                    "steady_band": MOMENTUM_STEADY_BAND,
                    "computed_conviction": score,
                }),
                parser_outcome: "scored".to_string(),
            },
        )
        .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
