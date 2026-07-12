//! Momentum stage — the generated trajectory card over PEAK, Vibe, and deterministic slopes.
//!
//! `momentum_scores` remains the numeric backbone for leaderboards and ranking. This stage adds the
//! client-surfaced read: a direction, signed score, and concise blurb with provenance, persisted to
//! `momentum_summaries` and consumed by Sigil as the Momentum pillar.

use crate::harness::{EntityKey, Harness, Parser};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::sigil::{self, SynthMomentum, SynthRating, SynthVibe};
use crate::stage::StageHandler;
use crate::util::{go_json_float, go_json_string, hash_components, round1};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

/// Prompt version for the generated Momentum card.
pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s2";

/// Output contract captured separately in the diagnostic ledger.
pub const MOMENTUM_OUTPUT_CONTRACT_VERSION: &str = "momentum-summary-v1";

/// Keep Momentum on the incumbent stats route until a broader fixture set proves a split.
pub const MOMENTUM_TEMPERATURE: f64 = 0.3;

pub const MOMENTUM_NUM_PREDICT: i32 = 700;

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";

pub const MOMENTUM_SYSTEM_PROMPT: &str = r#"Task: write a Momentum read from the supplied PEAK and Vibe trajectory context.

Operator frame: savvy, nimble trader tracking two markets: PEAK/rating as price action and Vibe/news as investor sentiment. You are detached, not emotionally attached to the position, and results-only.

Voice: direct, analytical, sports-literate. No hype, no fan logic, no melodrama. Ground every claim in the supplied numbers.

Definitions:
- PEAK trajectory = recent movement in statistical performance / rating signal.
- Vibe trajectory = recent movement in narrative sentiment.
- Momentum score is signed direction, not overall player/team quality: positive is rising, negative is falling, near zero is hold/steady or mixed.

Output exactly:
MOMENTUM: <rising|falling|steady>
SCORE: <integer -5 to 5>
READ: <one concise paragraph>

Rules:
- Pick rising only when the supplied trajectory is clearly positive overall.
- Pick falling only when the supplied trajectory is clearly negative overall.
- Pick steady when the signals are flat, sparse, or meaningfully split.
- Do not chase sentiment hype when PEAK/rating does not confirm it.
- Do not cling to a strong PEAK label when current trajectory is deteriorating.
- When PEAK and Vibe disagree, name the conflict and keep the score near zero unless one side is clearly dominant.
- Do not invent games, rankings, injuries, trades, or stats not in the prompt."#;

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

#[derive(Clone, Debug)]
pub struct MomentumReply {
    pub direction: String,
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
    let season = sigil::resolve_season(&hx.pool, sport, None).await?;
    let (rating, vibe, snapshot) = tokio::try_join!(
        sigil::load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season)),
        sigil::load_vibe_pillar(&hx.pool, entity_type, entity_id, sport),
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

fn momentum_direction_label(score: i32) -> &'static str {
    if score >= 1 {
        "rising"
    } else if score <= -1 {
        "falling"
    } else {
        "steady"
    }
}

fn build_momentum_input_components(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
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
        pairs.push(("vibe_sentiment", v.sentiment.to_string()));
        if !v.prompt.is_empty() {
            pairs.push(("vibe_prompt", go_json_string(&v.prompt)));
        }
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

fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {entity_name} ({sport} {entity_type})\n\n"
    ));
    b.push_str("=== PEAK TRAJECTORY ===\n");
    match rating {
        Some(r) => {
            b.push_str(&format!("PEAK label: {}\n", empty_dash(&r.divined_peak)));
            b.push_str(&format!("Notability: {}/100\n", r.notability));
            if !r.peak_trajectory_label.trim().is_empty() {
                b.push_str(&format!("PEAK trajectory: {}\n", r.peak_trajectory_label));
            } else {
                b.push_str(&format!("PEAK trajectory: {}\n", r.peak_trajectory));
            }
        }
        None => b.push_str("(no PEAK report available)\n"),
    }
    b.push_str("\n=== VIBE TRAJECTORY ===\n");
    match vibe {
        Some(v) => {
            b.push_str(&format!("Sentiment: {}/100\n", v.sentiment));
            if !v.prompt.trim().is_empty() {
                b.push_str(&format!("Vibe prompt: {}\n", v.prompt));
            }
        }
        None => b.push_str("(no vibe prompt available)\n"),
    }
    b.push_str("\n=== MOMENTUM SNAPSHOT ===\n");
    if let Some(score) = mom.momentum_score {
        b.push_str(&format!("Momentum score: {:.1}\n", score));
    }
    if let Some(s) = mom.rating_slope {
        b.push_str(&format!(
            "PEAK/rating slope: {:.1} over {} samples\n",
            s, mom.rating_samples
        ));
    }
    if let Some(s) = mom.vibe_slope {
        b.push_str(&format!(
            "Vibe slope: {:.1} over {} samples\n",
            s, mom.vibe_samples
        ));
    }
    if mom.empty() {
        b.push_str("(no durable momentum snapshot)\n");
    }
    b.push_str("\nWrite the Momentum read now.");
    b
}

fn empty_dash(s: &str) -> &str {
    if s.trim().is_empty() {
        "-"
    } else {
        s
    }
}

fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    let mut direction = String::new();
    let mut score = None;
    let mut read_lines: Vec<String> = Vec::new();
    let mut in_read = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "MOMENTUM:") {
            direction = normalize_momentum_direction(rest);
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
    if direction.is_empty() || blurb.is_empty() {
        return None;
    }
    Some(MomentumReply {
        direction,
        score,
        blurb,
    })
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &s[prefix.len()..])
}

fn normalize_momentum_direction(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if lower.contains("fall") || lower.contains("slid") || lower.contains("sliding") {
        "falling".to_string()
    } else if lower.contains("ris") || lower.contains("surg") || lower.contains("up") {
        "rising".to_string()
    } else if lower.contains("steady") || lower.contains("flat") || lower.contains("mixed") {
        "steady".to_string()
    } else {
        String::new()
    }
}

fn parse_first_i32(s: &str) -> Option<i32> {
    let mut buf = String::new();
    let mut started = false;
    for c in s.chars() {
        if c == '-' && !started {
            buf.push(c);
            started = true;
        } else if c.is_ascii_digit() {
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
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: Some(ctx.season),
        };
        if hx
            .debounce_unchanged("momentum_summaries", &key, &ctx.input_hash)
            .await?
        {
            return Ok(());
        }

        let prompt = build_momentum_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            ctx.rating.as_ref(),
            ctx.vibe.as_ref(),
            &ctx.snapshot,
        );
        let opts = GenerateOptions {
            system: Some(MOMENTUM_SYSTEM_PROMPT.to_string()),
            temperature: Some(MOMENTUM_TEMPERATURE),
            num_predict: MOMENTUM_NUM_PREDICT,
            num_ctx: 0,
            json_mode: false,
        };
        let extracted = hx
            .extract(Role::MomentumLogic, &prompt, &opts, &MomentumParser)
            .await?;
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;
        let out = MomentumOutput {
            direction: reply.direction,
            score: reply.score,
            blurb: reply.blurb,
            season: ctx.season,
            input_components_json: ctx.input_components_json.clone(),
            input_hash: ctx.input_hash.clone(),
            model: extracted.model,
            prompt_version: MOMENTUM_PROMPT_VERSION,
            built_prompt: extracted.built_prompt,
            request_body: extracted.request_body,
            eval_count: extracted.eval_count,
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
                    "deterministic_direction": ctx.snapshot.momentum_score
                        .map(|s| momentum_direction_label(s.round() as i32)),
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
        let parsed = parse_momentum_reply(
            "MOMENTUM: rising\nSCORE: 3\nREAD: PEAK is rising while Vibe is calm.",
        )
        .unwrap();
        assert_eq!(parsed.direction, "rising");
        assert_eq!(parsed.score, 3);
        assert_eq!(parsed.blurb, "PEAK is rising while Vibe is calm.");
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
        assert_eq!(
            build_momentum_input_components(Some(&rating), Some(&vibe), &mom),
            r#"{"divined_peak":"Rim protection","momentum_rating_samples":6,"momentum_rating_slope":1.2,"momentum_score":1.2,"momentum_vibe_samples":4,"momentum_vibe_slope":-0,"notability":88,"peak_trajectory":"rising","peak_trajectory_label":"Composite rising","vibe_prompt":"Coverage is warmer","vibe_sentiment":62}"#
        );
    }
}
