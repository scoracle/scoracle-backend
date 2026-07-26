//! Momentum stage — the generated trajectory card over PEAK, Vibe, and deterministic slopes.
//!
//! `momentum_scores` remains the numeric backbone for leaderboards and ranking. This stage adds the
//! client-surfaced read: a direction, signed score, and concise blurb with provenance, persisted to
//! `momentum_summaries` and consumed by Sigil as the Momentum pillar.

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

pub const MOMENTUM_NUM_PREDICT: i32 = 700;

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
    pub score: i32,
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

/// The legal SCORE range for a decided direction — the contract the model is asked to
/// honor. A violating sign is clamped into range (never a hard failure: the direction is
/// the truth, the score is narration, and failing the item would re-litigate a decided
/// fact through retries).
fn clamp_score_to_direction(direction: &str, score: i32) -> i32 {
    match direction {
        "rising" => score.clamp(1, 5),
        "falling" => score.clamp(-5, -1),
        _ => score.clamp(-1, 1),
    }
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
    let mut score = None;
    let mut read_lines: Vec<String> = Vec::new();
    let mut in_read = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // s4 dropped the MOMENTUM line from the contract (direction is decided in code),
        // but a model echoing the decided direction back is not an error — skip the line.
        if strip_prefix_ci(trimmed, "MOMENTUM:").is_some() {
            in_read = false;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "SCORE:") {
            score = parse_first_i32(rest).map(|s| s.clamp(-5, 5));
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
    let score = score?;
    if blurb.is_empty() {
        return None;
    }
    Some(MomentumReply { score, blurb })
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &s[prefix.len()..])
}

fn parse_first_i32(s: &str) -> Option<i32> {
    let mut buf = String::new();
    let mut started = false;
    for c in s.chars() {
        if (c == '-' && !started) || c.is_ascii_digit() {
            buf.push(c);
            started = true;
        } else if started {
            break;
        }
    }
    buf.parse::<i32>().ok()
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

async fn enqueue_sigil_for_momentum(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    out: &MomentumOutput,
) -> Result<()> {
    let sig = Item {
        stage: Stage::Sigil,
        entity_type: item.entity_type.clone(),
        entity_id: item.entity_id,
        sport: sport.to_string(),
        input_version: Some(momentum_work_input_version(out.season, &out.input_hash)),
        attempts: 0,
    };
    work::enqueue(pool, &sig).await
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
            num_ctx: 0,
            json_mode: false,
            format_schema: None,
        };
        let extracted = hx
            .extract(Role::MomentumLogic, &prompt, &opts, &MomentumParser)
            .await?;
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;

        // Direction is DECIDED here, not by the model (Session D, North Star #4). The same
        // decided value went into the prompt, so the READ narrates it by construction; the
        // score is clamped into the direction's legal range so the persisted row can never
        // tell two stories (the 645-WARNs/day disagreement class is gone by design).
        let direction = momentum_direction_from_score(ctx.snapshot.momentum_score);
        let score = clamp_score_to_direction(direction, reply.score);
        let score_clamped = score != reply.score;
        if score_clamped {
            debug!(
                entity_type = %item.entity_type,
                entity_id,
                sport = %sport,
                direction,
                model_score = reply.score,
                clamped_score = score,
                "momentum: model score sign disagreed with decided direction; clamped"
            );
        }

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
                    "score_clamped_to_direction": score_clamped,
                }),
                parser_outcome: "scored".to_string(),
            },
        )
        .await;
        enqueue_sigil_for_momentum(&hx.pool, item, &sport, &out).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_momentum_reply() {
        let parsed =
            parse_momentum_reply("SCORE: 3\nREAD: PEAK is rising while Vibe is calm.").unwrap();
        assert_eq!(parsed.score, 3);
        assert_eq!(parsed.blurb, "PEAK is rising while Vibe is calm.");
    }

    #[test]
    fn parse_first_i32_scans_sign_and_digits() {
        assert_eq!(parse_first_i32("SCORE: 3"), Some(3));
        assert_eq!(parse_first_i32("SCORE: -1.0"), Some(-1));
        assert_eq!(parse_first_i32("no digits here"), None);
    }

    #[test]
    fn stray_momentum_line_is_tolerated_and_ignored() {
        // s4 dropped MOMENTUM from the contract; a model echoing the decided direction (in
        // any word, even the old Conflict failure) is skipped, never parsed as content.
        let parsed = parse_momentum_reply(
            "MOMENTUM: Conflict\nSCORE: -1.0\nREAD: Signals split between PEAK and Vibe.",
        )
        .unwrap();
        assert_eq!(parsed.score, -1);
        assert_eq!(parsed.blurb, "Signals split between PEAK and Vibe.");
        // Missing SCORE or empty READ still fail closed.
        assert!(parse_momentum_reply("READ: prose only, no score.").is_none());
        assert!(parse_momentum_reply("SCORE: 2").is_none());
    }

    #[test]
    fn direction_is_decided_by_band_not_model() {
        // ±MOMENTUM_STEADY_BAND on the ±100-scale momentum_score; None (no durable
        // snapshot) is honestly steady.
        assert_eq!(momentum_direction_from_score(Some(10.0)), "rising");
        assert_eq!(momentum_direction_from_score(Some(9.9)), "steady");
        assert_eq!(momentum_direction_from_score(Some(-9.9)), "steady");
        assert_eq!(momentum_direction_from_score(Some(-10.0)), "falling");
        assert_eq!(momentum_direction_from_score(Some(0.0)), "steady");
        assert_eq!(momentum_direction_from_score(None), "steady");
    }

    #[test]
    fn score_clamps_into_the_decided_directions_range() {
        assert_eq!(clamp_score_to_direction("rising", -3), 1);
        assert_eq!(clamp_score_to_direction("rising", 4), 4);
        assert_eq!(clamp_score_to_direction("falling", 2), -1);
        assert_eq!(clamp_score_to_direction("falling", -5), -5);
        assert_eq!(clamp_score_to_direction("steady", 4), 1);
        assert_eq!(clamp_score_to_direction("steady", -4), -1);
        assert_eq!(clamp_score_to_direction("steady", 0), 0);
    }

    #[test]
    fn prompt_carries_the_decided_direction_line() {
        let mom = SynthMomentum {
            rating_slope: Some(50.7),
            rating_samples: 4,
            momentum_score: Some(50.7),
            ..SynthMomentum::default()
        };
        let prompt =
            build_momentum_prompt("player", "Test Player", "FOOTBALL", None, None, &mom, None);
        assert!(prompt.contains(
            "Direction (decided upstream, final): rising (momentum score +50.7, steady band ±10)"
        ));
        // No memory ⇒ no section (s4 byte-shape preserved).
        assert!(!prompt.contains("RELATIONAL MEMORY"));
        // No snapshot → the decided line still exists and is honestly steady.
        let empty = build_momentum_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            None,
            None,
            &SynthMomentum::default(),
            None,
        );
        assert!(empty.contains(
            "Direction (decided upstream, final): steady (no durable momentum snapshot)"
        ));
    }

    #[test]
    fn relational_memory_renders_before_the_decided_direction() {
        // The s5 memory card sits between the snapshot and the decided-direction line, so
        // the decided fact stays adjacent to the reply cue. Instruction line + bullets.
        let mom = SynthMomentum {
            momentum_score: Some(50.7),
            ..SynthMomentum::default()
        };
        let mem = "Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nCurrent story: Chelsea — tracked since Jun 09, computed likelihood 85/100.";
        let p = build_momentum_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            None,
            None,
            &mom,
            Some(mem),
        );
        assert!(
            p.contains("=== RELATIONAL MEMORY (computed history) ===\nArc context for the READ")
        );
        assert!(p.contains("- Prior story: Real Madrid — fizzled"));
        let mem_pos = p.find("RELATIONAL MEMORY").unwrap();
        let dir_pos = p.find("Direction (decided upstream, final)").unwrap();
        let cue_pos = p.find("Write the Momentum read now").unwrap();
        assert!(mem_pos < dir_pos && dir_pos < cue_pos);
        // Blank memory ⇒ no section.
        let blank = build_momentum_prompt(
            "player",
            "Test Player",
            "FOOTBALL",
            None,
            None,
            &mom,
            Some(" \n  "),
        );
        assert!(!blank.contains("RELATIONAL MEMORY"));
    }

    #[test]
    fn input_components_are_stable_and_sorted() {
        let rating = SynthRating {
            divined_peak: "Rim protection".to_string(),
            body: "body".to_string(),
            notability: 88,
            peak_trajectory: "rising".to_string(),
            peak_trajectory_label: "Composite rising".to_string(),
        };
        let vibe = SynthVibe {
            sentiment: 62,
            prompt: "Coverage is warmer".to_string(),
        };
        let mom = SynthMomentum {
            rating_slope: Some(1.24),
            rating_samples: 6,
            vibe_slope: Some(-0.04),
            vibe_samples: 4,
            momentum_score: Some(1.19),
            ..SynthMomentum::default()
        };
        // The vibe prompt is non-empty on purpose: the golden proves the felt-read prose is
        // NOT in the hash pre-image (F1 material-only debounce) — only vibe_sentiment is.
        // prompt_version joined at s6 (single-sourced from the const, so a bump can't
        // silently rot this pin); keys stay sorted, so it lands alphabetically.
        assert_eq!(
            build_momentum_input_components(Some(&rating), Some(&vibe), &mom),
            format!(
                r#"{{"divined_peak":"Rim protection","momentum_rating_samples":6,"momentum_rating_slope":1.2,"momentum_score":1.2,"momentum_vibe_samples":4,"momentum_vibe_slope":-0,"notability":88,"peak_trajectory":"rising","peak_trajectory_label":"Composite rising","prompt_version":"{MOMENTUM_PROMPT_VERSION}","vibe_sentiment":62}}"#
            )
        );
    }
}
