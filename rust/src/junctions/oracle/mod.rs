//! Terminal Oracle stage: assemble the available evidence, compute its direction, ask the model
//! for a reading and score, then persist with a material-input debounce.
//!
//! With no pillars, the stage writes a NULL marker without a model call. Previous output may
//! provide prompt continuity but never enters the input hash.

use crate::corpus::{load_transfer_heat, HeatItem};
use crate::harness::{EntityKey, Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::trajectory::DEFAULT_TRAJECTORY;
use crate::util::{hash_components, round1, truncate};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

mod inputs;
pub mod prompt;
pub use crate::junctions::form::oracle_format_schema;
pub use inputs::{build_crown_prompt, CROWN_CARD_BODY_CAP};
pub use prompt::{ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT};

/// Output contract captured separately from the prompt version in the diagnostic ledger.
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v2";

const ORACLE_LEDGER: LedgerSpec = LedgerSpec {
    stage: "sigil",
    lens: "oracle",
    role: Role::OracleLogic,
    product_table: "sigil_synthesis",
    output_contract_version: ORACLE_OUTPUT_CONTRACT_VERSION,
};

/// Production crown temperature. Fixtures pin zero.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Token cap for the short `{reading, score}` reply.
pub const ORACLE_NUM_PREDICT: i32 = 350;

/// Output reservation inside the small voice window.
pub const SMALL_WINDOW_NUM_PREDICT: i32 = 700;

// ---------------------------------------------------------------------------
// Pillar values.
// ---------------------------------------------------------------------------

/// One narrative from the entity's latest generation. `impact` is widened from a database
/// integer and rendered without a fractional part.
#[derive(Clone, Debug)]
pub struct SynthNarrative {
    pub title: String,
    pub body: String,
    pub impact: f64,
    pub trajectory: String,
    /// Prompt-only corroboration and freshness; excluded from the material hash.
    pub source_count: i32,
    pub source_age_days: Option<i32>,
}

/// The Scout's rating pillar. A latest NULL body suppresses the pillar.
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub body: String,
    pub notability: i32,
    pub rating_trajectory: String,
    pub rating_trajectory_label: String,
}

/// The vibe pillar (P3): the latest felt-read product, distinct from the Momentum trajectory.
#[derive(Clone, Debug)]
pub struct SynthVibe {
    pub sentiment: i32,
    pub prompt: String,
}

/// The momentum pillar (P4): durable trajectory values from `momentum_scores`.
#[derive(Clone, Debug, Default)]
pub struct SynthMomentum {
    pub direction: Option<String>,
    pub blurb: Option<String>,
    pub input_hash: Option<String>,
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub momentum_score: Option<f64>,
}

impl SynthMomentum {
    /// empty mirrors `synthMomentum.empty()`: no momentum signal at all.
    pub fn empty(&self) -> bool {
        self.direction.is_none()
            && self.blurb.is_none()
            && self.vibe_slope.is_none()
            && self.rating_slope.is_none()
            && self.momentum_score.is_none()
    }
}

/// Validated Oracle reply.
#[derive(Clone, Debug)]
pub struct CrownReply {
    /// The reading generated from the available evidence.
    pub reading: String,
    /// Optional title; absence never fails the reading.
    pub headline: Option<String>,
    /// The 1-100 verdict the reading earned, generated LAST. Clamped to 1-100 at parse.
    pub score: i32,
}

/// Complete `sigil_synthesis` row before persistence. The model supplies reading and score;
/// code supplies omen and convergence.
#[derive(Clone, Debug)]
pub struct SigilSynthesis {
    /// `None` ⇒ no-pillar NULL marker (no model call was made).
    pub score: Option<i32>,
    /// The crown reading — the served voice. `None` ⇒ marker; `Some` ⇒ a scored reading.
    pub reading: Option<String>,
    /// The season this convergence is for (current_season, resolved + stamped). Never NULL.
    pub season: i32,
    /// Canonical input-components JSON persisted as the hash pre-image. `"{}"` for a marker.
    pub input_components_json: String,
    /// Optional model-emitted title.
    pub headline: Option<String>,
    /// Deterministic convergence (1-100) from `pillar_convergence` — NOT model-emitted. `None`
    /// for the marker and when no directional pillar pair exists. NOT part of the `input_hash`.
    pub convergence: Option<i32>,
    /// The computed omen the reading was drawn under (`compute_omen`). `None` for the marker.
    pub omen: Option<&'static str>,
}

pub type SigilOutput = Generation<SigilSynthesis>;

// ---------------------------------------------------------------------------
// Completion barrier.
// ---------------------------------------------------------------------------

/// Enqueue the Oracle after every pillar has settled for this entity. Call after completing the
/// current work row; checking earlier is racy under a concurrent drain.
pub async fn enqueue_oracle_if_pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
    input_version: Option<String>,
) -> Result<bool> {
    if !work::pillars_settled(pool, entity_type, entity_id, sport).await? {
        debug!(
            %entity_type, entity_id, %sport,
            "oracle barrier: pillars still outstanding; not enqueuing"
        );
        return Ok(false);
    }

    let sig = Item {
        stage: Stage::Sigil,
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.to_string(),
        input_version,
        attempts: 0,
    };
    work::enqueue(pool, &sig).await?;
    debug!(
        %entity_type, entity_id, %sport,
        "oracle barrier: last pillar settled; enqueued sigil"
    );
    Ok(true)
}

// ---------------------------------------------------------------------------
// Pillar loaders.
// ---------------------------------------------------------------------------

/// resolve_season returns the concrete season this synthesis is for: the caller's explicit
/// season when given, else the sport's `current_season`.
/// `sport` is the upper-cased value (the SQL key).
pub async fn resolve_season(pool: &PgPool, sport: &str, want: Option<i32>) -> Result<i32> {
    if let Some(s) = want {
        return Ok(s);
    }
    let cur: i32 = sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("resolve current_season for {sport}"))?;
    Ok(cur)
}

/// Load the entity's latest non-marker narratives, hottest first.
pub async fn load_narrative_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SynthNarrative>> {
    // COALESCE promotes the impact expression to int4, so scan it as i32.
    let rows: Vec<(String, String, i32, String, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), COALESCE(trajectory, $4),
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
    .with_context(|| format!("load narrative pillar {entity_type}/{entity_id}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(title, body, impact, trajectory, source_count, source_age_days)| SynthNarrative {
                title,
                body,
                impact: impact as f64,
                trajectory,
                source_count,
                source_age_days,
            },
        )
        .collect())
}

/// Load the latest rating row, suppressing the pillar when that row is a NULL marker. Never fall
/// back behind a marker to older prose.
pub async fn load_rating_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<SynthRating>> {
    // COALESCE(notability, 0): int2 coalesced with int4 → int4 → scan i32.
    let row: Option<(Option<String>, i32, String, String)> = sqlx::query_as(
        r#"
        SELECT body, COALESCE(notability, 0),
               COALESCE(rating_trajectory, 'steady'), COALESCE(rating_trajectory_label, '')
        FROM stat_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND ($4::int IS NULL OR season = $4)
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load rating pillar {entity_type}/{entity_id}"))?;

    match row {
        None => Ok(None),                  // pgx.ErrNoRows → pillar absent
        Some((None, _, _, _)) => Ok(None), // latest generation is a marker (body NULL) → suppressed
        Some((Some(body), notability, rating_trajectory, rating_trajectory_label)) => {
            Ok(Some(SynthRating {
                body,
                notability,
                rating_trajectory,
                rating_trajectory_label,
            }))
        }
    }
}

/// load_vibe_pillar (P3) reads the latest Vibe felt-state product. A latest NULL-sentiment marker
/// suppresses the pillar instead of falling back to an older real Vibe.
pub async fn load_vibe_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<SynthVibe>> {
    let row: Option<(Option<i16>, String)> = sqlx::query_as(
        r#"
        SELECT sentiment, COALESCE(prompt, '')
        FROM vibe_scores
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load vibe pillar {entity_type}/{entity_id}"))?;

    match row {
        Some((Some(sentiment), prompt)) => Ok(Some(SynthVibe {
            sentiment: sentiment as i32,
            prompt,
        })),
        _ => Ok(None),
    }
}

/// Load the latest generated Momentum card and its deterministic inputs.
pub async fn load_momentum_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"
        SELECT direction, score, blurb, input_hash, COALESCE(input_components, '{}'::jsonb)
        FROM public.momentum_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
          AND ($4::int IS NULL OR season = $4)
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load momentum pillar {entity_type}/{entity_id}"))?;

    let Some((direction, score, blurb, input_hash, components)) = row else {
        return Ok(SynthMomentum::default());
    };
    let rating_slope = components
        .get("momentum_rating_slope")
        .and_then(serde_json::Value::as_f64);
    let rating_samples = components
        .get("momentum_rating_samples")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default() as i32;
    let vibe_slope = components
        .get("momentum_vibe_slope")
        .and_then(serde_json::Value::as_f64);
    let vibe_samples = components
        .get("momentum_vibe_samples")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default() as i32;
    Ok(SynthMomentum {
        direction: direction.filter(|s| !s.trim().is_empty()),
        blurb: blurb.filter(|s| !s.trim().is_empty()),
        input_hash,
        vibe_slope,
        vibe_samples,
        rating_slope,
        rating_samples,
        momentum_score: score.map(f64::from),
    })
}

/// Resolve the season and load every pillar concurrently for production and eval callers.
pub async fn load_pillars(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str, // upper-cased
) -> Result<(
    i32,
    Vec<SynthNarrative>,
    Option<SynthRating>,
    Option<SynthVibe>,
    SynthMomentum,
    Vec<HeatItem>,
)> {
    let season = resolve_season(&hx.pool, sport, None).await?;
    // The pillars are independent once the season is known. Transfer heat uses the same served
    // rumor loader as the cards and trigger gate.
    let (narratives, rating, vibe, momentum, transfers) = tokio::try_join!(
        async {
            load_narrative_pillar(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("narrative pillar")
        },
        async {
            load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
                .await
                .context("rating pillar")
        },
        async {
            load_vibe_pillar(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("vibe pillar")
        },
        async {
            load_momentum_pillar(&hx.pool, entity_type, entity_id, sport, Some(season))
                .await
                .context("momentum pillar")
        },
        async {
            load_transfer_heat(&hx.pool, entity_type, entity_id, sport)
                .await
                .context("transfer pillar")
        },
    )?;
    Ok((season, narratives, rating, vibe, momentum, transfers))
}

// ---------------------------------------------------------------------------
// Deterministic trend math.
// ---------------------------------------------------------------------------

/// linear_slope computes the slope of a simple OLS regression on the series [0..N-1] → values.
/// Positive means trending up. Keep this separate from the Scout's mean-centered implementation:
/// their different floating-point accumulation order can move values at bucket boundaries.
#[cfg(test)]
fn linear_slope(vals: &[f64]) -> f64 {
    let n = vals.len() as f64;
    if vals.len() < 2 {
        return 0.0;
    }
    let (mut sum_x, mut sum_y, mut sum_xy, mut sum_xx) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, v) in vals.iter().enumerate() {
        let x = i as f64;
        sum_x += x;
        sum_y += *v;
        sum_xy += x * *v;
        sum_xx += x * x;
    }
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-9 {
        return 0.0;
    }
    (n * sum_xy - sum_x * sum_y) / denom
}

/// momentum_score reads the durable signed Momentum trajectory value. It is directional force,
/// not entity quality: positive is rising, negative is sliding, zero is flat.
fn momentum_score(mom: &SynthMomentum) -> Option<i32> {
    mom.momentum_score.map(|s| s.round() as i32)
}

fn momentum_score_label(score: i32) -> &'static str {
    if score >= 3 {
        "surging"
    } else if score >= 1 {
        "rising"
    } else if score <= -3 {
        "falling"
    } else if score <= -1 {
        "sliding"
    } else {
        "steady"
    }
}

// ---------------------------------------------------------------------------
// Input components and material-only debounce hash. Upstream model prose is prompt context but
// never part of this key.
// ---------------------------------------------------------------------------

/// Build canonical input-components JSON. Narrative keys are always present; the rest are
/// conditional.
pub fn build_synthesis_input_components(
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
) -> String {
    let mut titles: Vec<String> = narratives.iter().map(|n| n.title.clone()).collect();
    titles.sort();

    let mut trajectory_pairs: Vec<String> = narratives
        .iter()
        .map(|n| format!("{}:{}", n.title, n.trajectory))
        .collect();
    trajectory_pairs.sort();

    let mut components = serde_json::Map::new();
    components.insert("narrative_titles".into(), serde_json::json!(titles));
    components.insert(
        "narrative_trajectories".into(),
        serde_json::json!(trajectory_pairs),
    );

    if let Some(r) = rating {
        // The Oracle reads the Scout and Analyst outputs, not the raw trajectory marker.
        components.insert("notability".into(), serde_json::json!(r.notability));
    }
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose is PROMPT-ONLY (F1, material-only
        // debounce): vibe generates at temp 0.7, so hashing its prose flipped this hash on
        // every vibe re-run even when nothing material moved.
        components.insert("vibe_sentiment".into(), serde_json::json!(v.sentiment));
    }
    if let Some(s) = mom.vibe_slope {
        components.insert("momentum_vibe_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_vibe_samples".into(),
            serde_json::json!(mom.vibe_samples),
        );
    }
    if let Some(s) = mom.rating_slope {
        components.insert("momentum_rating_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_rating_samples".into(),
            serde_json::json!(mom.rating_samples),
        );
    }
    if let Some(score) = mom.momentum_score {
        components.insert("momentum_score".into(), serde_json::json!(round1(score)));
    }
    if let Some(direction) = &mom.direction {
        components.insert("momentum_direction".into(), serde_json::json!(direction));
    }
    // momentum_blurb is PROMPT-ONLY (F1, material-only debounce): the blurb is momentum's
    // model prose, so hashing it made every momentum regeneration flip sigil's hash even when
    // the material signals were unchanged. momentum_summary_hash below is momentum's own
    // input_hash — material-only after F1 — so sigil still re-runs when momentum's INPUTS
    // genuinely move.
    if let Some(input_hash) = &mom.input_hash {
        components.insert(
            "momentum_summary_hash".into(),
            serde_json::json!(input_hash),
        );
    }

    // Transfer heat is conditional and sorted into a stable canonical shape.
    if !transfers.is_empty() {
        let mut lines: Vec<String> = transfers
            .iter()
            .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
            .collect();
        lines.sort();
        components.insert("transfer_heat".into(), serde_json::json!(lines));
    }
    serde_json::Value::Object(components).to_string()
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// Deterministic cross-pillar direction comparison handed to the model as a decided fact.
#[derive(Clone, Debug, PartialEq)]
pub struct PillarComparison {
    pub label: String,
    pub agree: bool,
}

/// Reduce a value to a direction sign: `None` = not directional (skip the comparison).
fn trajectory_sign(key: &str) -> Option<i8> {
    match key {
        "rising" | "heating_up" => Some(1),
        "falling" | "cooling_off" => Some(-1),
        _ => None,
    }
}

fn sentiment_sign(sentiment: i32) -> Option<i8> {
    if sentiment >= 60 {
        Some(1)
    } else if sentiment <= 40 {
        Some(-1)
    } else {
        None
    }
}

fn sign_word(s: i8) -> &'static str {
    if s > 0 {
        "positive"
    } else {
        "negative"
    }
}

/// build_pillar_divergence emits one comparison per directional pillar pair that is actually
/// present. Neutral/steady/absent signals produce NO line (a steady lens neither agrees nor
/// disagrees — the system prompt's own convergence rule). Pure and deterministic; the card is
/// prompt-only and derives entirely from values already in the input hash, so it can never
/// trigger a regeneration by itself.
pub fn build_pillar_divergence(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> Vec<PillarComparison> {
    let mut out = Vec::new();

    // Momentum is the sole direction signal; the Oracle never reads the raw tracker.
    let vibe_sign = vibe.and_then(|v| sentiment_sign(v.sentiment));
    let mom_sign = mom.direction.as_deref().and_then(trajectory_sign);
    // Profile strength: the LEVEL sign (is this an elite or a weak profile), distinct from the
    // direction sign. The classic rails conflict the fixtures measure — "strong profile vs
    // sliding momentum and negative narrative" — is a LEVEL-vs-direction disagreement that
    // direction pairs alone cannot see. (Narrative heating_up/cooling_off is deliberately NOT
    // compared: it measures story intensity, not valence — a negative story heating up must
    // not read as "positive narrative".)
    let strength_sign = rating.and_then(|r| {
        if r.notability >= 70 {
            Some(1i8)
        } else if r.notability <= 35 {
            Some(-1i8)
        } else {
            None
        }
    });
    let strength_word = |s: i8| if s > 0 { "strong" } else { "weak" };
    let mut push = |label: String, a: i8, b: i8| {
        out.push(PillarComparison {
            label,
            agree: (i32::from(a) * i32::from(b)) > 0,
        });
    };

    if let (Some(v), Some(m)) = (vibe_sign, mom_sign) {
        push(
            format!("Vibe ({}) vs Momentum ({})", sign_word(v), sign_word(m)),
            v,
            m,
        );
    }
    if let (Some(s), Some(m)) = (strength_sign, mom_sign) {
        push(
            format!(
                "Profile strength ({}) vs Momentum ({})",
                strength_word(s),
                sign_word(m)
            ),
            s,
            m,
        );
    }
    if let (Some(s), Some(v)) = (strength_sign, vibe_sign) {
        push(
            format!(
                "Profile strength ({}) vs Vibe ({})",
                strength_word(s),
                sign_word(v)
            ),
            s,
            v,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Deterministic omen and convergence: code decides, the model narrates.
// ---------------------------------------------------------------------------

/// Closed omen set used by the database constraint and served card.
pub const OMENS: [&str; 4] = ["ascendant", "steady", "waning", "crossroads"];

fn direction_sign(key: &str) -> i32 {
    match key {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

/// pillar_convergence turns the deterministic pillar comparisons into a 1-100 agreement number —
/// a computed measurement, not a model opinion. `round(100·agree/total)` floored at 1; `None` when no directional pair
/// exists (a quiet spread has nothing to converge on). The floor matches the DB contract
/// (`sigil_synthesis_convergence_check`: NULL or 1-100) — an all-disagree spread rounds to 0,
/// which the check rejects and which carries no product meaning beyond 1 (anything ≤ 50 is
/// already a crossroads to `compute_omen`, faithfully preserving the panel's soft rule).
pub fn pillar_convergence(comparisons: &[PillarComparison]) -> Option<i32> {
    if comparisons.is_empty() {
        return None;
    }
    let agree = comparisons.iter().filter(|c| c.agree).count();
    Some((((agree as f64 / comparisons.len() as f64) * 100.0).round() as i32).max(1))
}

/// compute_omen decides the reading's direction deterministically:
/// - a split spread (convergence ≤ 50 — half or more of the directional pairs disagree) is a
///   `crossroads` regardless of net direction — the contested arc IS the story;
/// - otherwise Momentum decides alone: positive ⇒
///   `ascendant`, negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(convergence: Option<i32>, mom: &SynthMomentum) -> &'static str {
    if let Some(c) = convergence {
        if c <= 50 {
            return "crossroads";
        }
    }
    let net = mom.direction.as_deref().map(direction_sign).unwrap_or(0);
    if net > 0 {
        "ascendant"
    } else if net < 0 {
        "waning"
    } else {
        "steady"
    }
}

// ---------------------------------------------------------------------------
// Output parsing — the crown reply is a bare {reading, score} object under format_schema.
// ---------------------------------------------------------------------------

/// parse_crown_score coerces the emitted score to an integer 1-100. `format_schema` makes it an
/// integer on the live route; the coercions keep offline/no-schema eval tolerant. Clamped 1-100.
fn parse_crown_score(v: &serde_json::Value) -> Option<i32> {
    let n = if let Some(i) = v.as_i64() {
        i
    } else if let Some(f) = v.as_f64() {
        if !f.is_finite() {
            return None;
        }
        f.round() as i64
    } else if let Some(s) = v.as_str() {
        let head = s.split_whitespace().next()?;
        let head = head.split_once('/').map(|(n, _)| n).unwrap_or(head).trim();
        match head.parse::<i64>() {
            Ok(n) => n,
            Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
        }
    } else {
        return None;
    };
    Some(n.clamp(1, 100) as i32)
}

/// parse_crown_reply extracts `{reading, score}` from the JSON reply. On the ollama path
/// `format_schema` makes a bare object the only thing the live route emits; the balanced-brace
/// salvage keeps the offline/eval path tolerant of a prose-wrapped object. Reading whitespace is
/// collapsed to one clean paragraph. `None` when there is no non-empty reading or no coercible
/// score (fail-closed → the item backs off).
///
/// Parse strict JSON first; salvage wrapped JSON and literal string control characters.
pub fn parse_crown_reply(raw: &str) -> Option<CrownReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        let span = escape_string_controls(&trimmed[start..=end]);
        serde_json::from_str(&span).ok()
    });
    let v = parsed?;
    let score = parse_crown_score(v.get("score")?)?;
    let reading = v.get("reading")?.as_str()?.trim();
    let reading = crate::junctions::form::normalize_body(reading);
    // Served prose takes the shared scrub.
    let reading = crate::guards::clean_served_prose(&reading);
    let reading = complete_sentences(&reading)?;
    let reading = strip_output_score_recap(reading, score);
    if reading.is_empty() {
        return None;
    }
    // Fold the optional title to one line; absence never fails the generation.
    let headline = v
        .get("headline")
        .and_then(|h| h.as_str())
        .map(|h| h.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|h| !h.is_empty());
    Some(CrownReply {
        reading,
        headline,
        score,
    })
}

/// The score has its own JSON field. If the model also narrates that same value, remove only the
/// sentence carrying the duplicate transport value and leave the surrounding interpretation.
fn strip_output_score_recap(mut reading: String, score: i32) -> String {
    let needles = [
        format!("score of {score}"),
        format!("score is {score}"),
        format!("score: {score}"),
    ];
    loop {
        let lower = reading.to_ascii_lowercase();
        let Some(hit) = needles.iter().filter_map(|n| lower.find(n)).min() else {
            break;
        };
        let start = lower[..hit].rfind(['.', '!', '?']).map_or(0, |i| i + 1);
        let end = lower[hit..]
            .find(['.', '!', '?'])
            .map_or(reading.len(), |i| hit + i + 1);
        reading.replace_range(start..end, "");
        reading = reading.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    reading.trim().to_string()
}

/// Keep a complete reading when a constrained string reaches its character ceiling. A complete
/// sentence passes byte-identical; only an unfinished tail is removed. With no complete sentence,
/// parsing fails and the work item retries.
fn complete_sentences(reading: &str) -> Option<String> {
    let reading = reading.trim();
    let sentence_end = |c: char| matches!(c, '.' | '!' | '?');
    let terminal = reading.trim_end_matches(['"', '\'', '\u{2019}', '\u{201d}', ')', ']']);
    let last_word = terminal
        .trim_end_matches(sentence_end)
        .split_whitespace()
        .next_back()
        .unwrap_or_default()
        .trim_matches(|c: char| !c.is_alphanumeric());
    let clipped_word = reading.chars().count()
        >= crate::junctions::form::ORACLE_READING_MAX_CHARS.saturating_sub(8)
        && last_word.len() == 1
        && last_word
            .chars()
            .all(|c| c.is_ascii_lowercase() && !matches!(c, 'a' | 'i'));
    if terminal.chars().next_back().is_some_and(sentence_end) && !clipped_word {
        return Some(reading.to_string());
    }
    let search = if clipped_word {
        terminal.trim_end_matches(sentence_end)
    } else {
        reading
    };
    let cut = search
        .char_indices()
        .rev()
        .find_map(|(i, c)| sentence_end(c).then_some(i + c.len_utf8()))?;
    Some(reading[..cut].trim_end().to_string())
}

// Preserve paragraph breaks when an unconstrained backend emits literal newlines in JSON strings.
fn escape_string_controls(span: &str) -> String {
    let mut out = String::with_capacity(span.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in span.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if in_string && ch == '\\' {
            out.push(ch);
            escaped = true;
        } else if ch == '"' {
            out.push(ch);
            in_string = !in_string;
        } else if in_string && ch.is_control() {
            out.push_str(&format!("\\u{:04x}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out
}

// Re-exported for callers that treat it as part of the Oracle surface.
pub use crate::guards::count_sentences;

/// CrownParser is the crown stage's `Parser` plug-in behind the `Parser<T>` seam. It never returns
/// the fail-closed `Ok(None)` — the crown's only fail-closed path is the pre-model no-pillar marker;
/// an unparseable reply (no reading or no score) is a genuine failure → `Err` → the item backs off.
pub struct CrownParser;

impl Parser<CrownReply> for CrownParser {
    fn parse(&self, raw: &str) -> Result<Option<CrownReply>> {
        match parse_crown_reply(raw) {
            Some(mut r) => {
                // Global served-prose invariants fail closed and retry the item.
                if crate::guards::has_bookkeeping_citation(&r.reading) {
                    tracing::warn!(guard = "bookkeeping_citation", "reading rejected");
                    bail!("crown: reading carries a bookkeeping citation");
                }
                // Optional titles fail open: salvage or drop, never reject the reading.
                r.headline = crate::guards::settle_title("oracle", r.headline.as_deref());
                if let Some(p) = crate::guards::first_product_name(&r.reading) {
                    tracing::warn!(guard = "product_name", name = p, "reading rejected");
                    bail!("crown: reading names product {p:?}");
                }
                if crate::guards::has_foreign_script(&r.reading) {
                    tracing::warn!(guard = "foreign_script", "reading rejected");
                    bail!("crown: reading carries a foreign-script run");
                }
                Ok(Some(r))
            }
            None => bail!(
                "crown: could not parse reading+score from response (raw={:?})",
                truncate(raw, 200)
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// The core generate + the production handler.
// ---------------------------------------------------------------------------

/// persist_to_sigil_synthesis writes one crown row — the scored reading OR the no-pillar NULL
/// marker, which differ only in the bound values. One call now, so the crown's model/prompt IS
/// the voice's: voiced_score echoes the emitted score (the verdict IS the voiced score), and
/// voiced_at/voice_* are stamped only when a reading was drawn (NULL for the marker). trigger_type
/// 'periodic', trigger_payload `{}`. The moat fields route through the shared `Provenance` envelope.
async fn persist_to_sigil_synthesis(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    season: i32,
    out: &SigilOutput,
    previous_score: Option<i16>,
) -> Result<i64> {
    let prov = &out.provenance;
    let entity_id = item.entity_id_i32()?;
    let score: Option<i16> = out.score.map(|n| n as i16);
    // No directional pair leaves convergence NULL.
    let convergence: Option<i16> = out.convergence.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, input_components, input_hash,
            model_version, prompt_version, convergence,
            reading, headline, omen, voiced_score,
            voiced_at, voice_model_version, voice_prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb, $5,$6,$7::jsonb,$8, $9,$10,$11,
            $12,$13,$14,$15,
            CASE WHEN $12 IS NOT NULL THEN NOW() END,
            CASE WHEN $12 IS NOT NULL THEN $9 END,
            CASE WHEN $12 IS NOT NULL THEN $10 END)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type) // $1
    .bind(entity_id) // $2
    .bind(sport) // $3
    .bind(season) // $4
    .bind(score) // $5
    .bind(previous_score) // $6
    .bind(out.input_components_json.as_str()) // $7
    .bind(prov.input_hash.as_deref()) // $8
    .bind(prov.model_version.as_str()) // $9  (also voice_model_version when reading present)
    .bind(prov.prompt_version) // $10 (also voice_prompt_version when reading present)
    .bind(convergence) // $11
    .bind(out.reading.as_deref()) // $12
    .bind(out.headline.as_deref()) // $13
    .bind(out.omen) // $14
    .bind(score) // $15  voiced_score = the emitted score (they reconcile)
    .fetch_one(pool)
    .await
    .context("persist sigil")?;
    Ok(row.get("id"))
}

async fn write_sigil_ledger(
    pool: &PgPool,
    item: &Item,
    entity_id: i32,
    sport: &str,
    out: &SigilOutput,
    product_row_id: i64,
) {
    insert_generation_ledger_best_effort(
        pool,
        out,
        ORACLE_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id,
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::json!({}),
            product_row_ids: vec![product_row_id],
            included_evidence: serde_json::json!({
                "input_components": serde_json::from_str::<serde_json::Value>(
                    &out.input_components_json
                ).unwrap_or_else(|_| serde_json::json!({
                    "raw_input_components": &out.input_components_json
                })),
                "score": out.score,
                "convergence": out.convergence,
                "omen": out.omen,
            }),
            excluded_evidence: if out.was_called() {
                serde_json::json!([])
            } else {
                serde_json::json!([{
                    "reason": "no_narrative_rating_vibe_momentum_or_transfer_pillar"
                }])
            },
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": ORACLE_NUM_PREDICT,
            })),
            parser_outcome: if out.was_called() {
                "parsed"
            } else {
                "no_call"
            },
        },
    )
    .await;
}

/// SigilHandler drains the durable `sigil` stage. It reads the pillars season-exact,
/// skips the model call when the pillar hash is unchanged (`debounce_unchanged`), otherwise
/// calls OracleLogic and persists one `sigil_synthesis` row carrying the reading and score.
/// Terminal stage: it enqueues nothing downstream.
pub struct SigilHandler;

impl SigilHandler {
    pub fn new() -> Self {
        SigilHandler
    }
}

impl Default for SigilHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for SigilHandler {
    fn stage(&self) -> Stage {
        Stage::Sigil
    }

    // The Oracle uses at most two slots in the voice group.
    fn max_in_flight(&self) -> usize {
        2
    }
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::stage::MAC_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        // nameOf: the name lookup uses the queue's raw sport value (drainSigil → corpus lookup).
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &item.sport)
                .await?;

        let sport = item.sport.to_uppercase();
        let (season, narratives, rating, vibe, momentum, transfers) =
            load_pillars(hx, &item.entity_type, entity_id, &sport).await?;

        // No-pillar marker (no model call): no cards turned up, so the crown has nothing to read.
        if narratives.is_empty()
            && rating.is_none()
            && vibe.is_none()
            && momentum.empty()
            && transfers.is_empty()
        {
            let out = Generation::uncalled(
                SigilSynthesis {
                    score: None,
                    reading: None,
                    headline: None,
                    season,
                    input_components_json: "{}".to_string(),
                    convergence: None,
                    omen: None,
                },
                hx.router.for_role(Role::OracleLogic).model().to_string(),
                ORACLE_PROMPT_VERSION,
                Vec::new(),
                None,
            );
            // The marker carries NULL reading/voice columns — serve-latest ignores markers, so the
            // last real reading keeps serving.
            let product_row_id =
                persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, None).await?;
            write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
            return Ok(());
        }

        // Skip the model call when material pillar inputs match the latest synthesis.
        let input_components_json = build_synthesis_input_components(
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
        );
        let input_hash = hash_components(&input_components_json);
        let key = EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: Some(season),
        };
        // One round-trip to the latest synthesis row for the debounce hash + the previous-score
        // baseline. (The prior blurb is gone with the panel; prior READINGS load below as memory.)
        let (prev_score_raw, latest_hash) = hx.latest_with_hash("sigil_synthesis", &key).await?;
        if latest_hash.as_deref() == Some(input_hash.as_str()) {
            return Ok(());
        }
        let prev = prev_score_raw.map(|v| v as i32).unwrap_or(0);

        // Deterministic convergence + omen, computed BEFORE the call and handed to the model as
        // decided cards (the PEAK ScoutingDecision discipline): the crown reads them, never infers.
        let comparisons = build_pillar_divergence(rating.as_ref(), vibe.as_ref(), &momentum);
        let convergence = pillar_convergence(&comparisons);
        let omen = compute_omen(convergence, &momentum);

        // In a small context window every pillar body is capped and the output reservation
        // shrinks. The Oracle reads cards, not their underlying evidence.
        let small = crate::route::small_voice_window(hx.voice_num_ctx);
        let body_cap = small.then_some(inputs::CROWN_CARD_BODY_CAP);

        // The one crown call (OracleLogic): read the cards + the omen, then emit
        // {reading, score}. Fail-closed lives in CrownParser (unparseable → Err → the item backs off).
        // Identity card: house records, dated — degrades to absent like memory.
        let identity =
            crate::corpus::load_identity_card(&hx.pool, &item.entity_type, entity_id, &sport)
                .await
                .unwrap_or_default();
        let prompt = build_crown_prompt(
            &item.entity_type,
            &name,
            &item.sport,
            &narratives,
            rating.as_ref(),
            vibe.as_ref(),
            &momentum,
            &transfers,
            omen,
            body_cap,
            identity.as_deref(),
        );
        let opts = GenerateOptions {
            system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
            temperature: Some(ORACLE_TEMPERATURE),
            num_predict: if small {
                SMALL_WINDOW_NUM_PREDICT
            } else {
                ORACLE_NUM_PREDICT
            },
            num_ctx: hx.voice_num_ctx,
            json_mode: false,
            format_schema: Some(oracle_format_schema()),
            format_schema_raw: None,
        };
        let extracted = hx
            .extract(Role::OracleLogic, &prompt, &opts, &CrownParser)
            .await?;
        let call = GenerationCall::from(&extracted);
        let model = extracted.model.clone();
        let reply = extracted
            .value
            .ok_or_else(|| anyhow!("crown: parser returned no value"))?;
        if !crate::guards::title_names_entity(&reply.reading, &name) {
            tracing::warn!(guard = "entity_identity", "crown reading rejected");
            bail!("crown: reading does not name entity {name:?}");
        }

        let out = Generation::called(
            SigilSynthesis {
                score: Some(reply.score),
                reading: Some(reply.reading),
                headline: reply.headline,
                season,
                input_components_json,
                convergence,
                omen: Some(omen),
            },
            model,
            ORACLE_PROMPT_VERSION,
            Vec::new(),
            Some(input_hash),
            call,
        );
        let prev_score: Option<i16> = if prev > 0 { Some(prev as i16) } else { None };
        let product_row_id =
            persist_to_sigil_synthesis(&hx.pool, item, &sport, season, &out, prev_score).await?;
        write_sigil_ledger(&hx.pool, item, entity_id, &sport, &out, product_row_id).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
