//! Oracle execution, completion barrier and deterministic crown fields.
pub mod manifest;
mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::application::models::ExecutionCapabilities;
use crate::application::products::EntityKey;
use crate::application::queue::work::{self, Item, TaskKey};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use parser::ReadingParser;
use prompt::{
    Assignment, Cards, Readiness, SynthMomentum, SynthRating, SynthVibe, ORACLE_PROMPT_VERSION,
};
use publish::{commit_claimed, record_ledger};
use sqlx::PgPool;
use tracing::debug;

/// Complete `sigil_synthesis` row before persistence. The plugin supplies every claim and score.
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

const PILLAR_STAGES: [TaskKey; 5] = [
    crate::plugins::journalist::manifest::TASK,
    crate::plugins::scout::manifest::TASK,
    crate::plugins::influencer::manifest::TASK,
    crate::plugins::analyst::manifest::TASK,
    crate::plugins::insider::manifest::TASK,
];

/// True when no pillar stage still owes this entity work. Failed pillars count as
/// settled at every attempt level, preserving the existing partial-read policy.
pub async fn pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
) -> Result<bool> {
    let stages: Vec<&str> = PILLAR_STAGES.iter().map(|stage| stage.as_str()).collect();
    let settled = sqlx::query_scalar(
        r#"
        SELECT NOT EXISTS (
            SELECT 1
              FROM pipeline_work
             WHERE entity_type = $1
               AND entity_id   = $2
               AND sport       = $3
               AND stage       = ANY($4)
               AND status <> 'failed'
        )
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(&stages)
    .fetch_one(pool)
    .await
    .with_context(|| format!("pillars_settled {entity_type}/{entity_id}"))?;
    Ok(settled)
}

/// Existing Oracle barrier policy: pending/running pillars block, while failed pillars count as
/// settled at every attempt level. A later successful retry offers Oracle again.
pub async fn enqueue_oracle_if_pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
    input_version: Option<String>,
) -> Result<bool> {
    if !pillars_settled(pool, entity_type, entity_id, sport).await? {
        debug!(%entity_type, entity_id, %sport, "oracle barrier: pillars still outstanding");
        return Ok(false);
    }
    work::enqueue(
        pool,
        &Item {
            stage: crate::plugins::oracle::manifest::TASK,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            input_version,
            attempts: 0,
            claim_token: None,
        },
    )
    .await?;
    debug!(%entity_type, entity_id, %sport, "oracle barrier: enqueued sigil");
    Ok(true)
}

pub(crate) struct OracleBarrierReaction {
    pool: PgPool,
}

impl OracleBarrierReaction {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl crate::application::queue::outbox::EventReaction for OracleBarrierReaction {
    fn name(&self) -> &'static str {
        "oracle.completion-barrier"
    }

    fn kinds(&self) -> &[&'static str] {
        &[
            crate::plugins::influencer::VIBE_COMPLETED,
            crate::plugins::scout::RATING_COMPLETED,
            crate::plugins::analyst::MOMENTUM_COMPLETED,
            crate::plugins::scout::RATING_DEBOUNCED,
            crate::plugins::journalist::NARRATIVES_COMPLETED,
            crate::plugins::insider::TRANSFER_PUBLISHED,
        ]
    }

    async fn react(&self, event: &crate::application::queue::outbox::Event) -> Result<()> {
        enqueue_oracle_if_pillars_settled(
            &self.pool,
            &event.entity_type,
            i64::from(event.entity_id),
            &event.sport,
            event.source_input_version.clone(),
        )
        .await?;
        Ok(())
    }
}

enum Prepared {
    Debounced,
    Product {
        output: Box<SigilOutput>,
        previous_score: Option<i16>,
    },
}

async fn prepare(pool: &PgPool, models: &ExecutionCapabilities, item: &Item) -> Result<Prepared> {
    let assignment = prompt::load_assignment(pool, item, models.voice_num_ctx).await?;
    let previous_score = if assignment.cards.readiness() == Readiness::Empty {
        None
    } else {
        let (previous_score, latest_hash) = crate::application::products::latest_with_hash(
            pool,
            "sigil_synthesis",
            &EntityKey {
                entity_type: item.entity_type.clone(),
                entity_id: item.entity_id_i32()?,
                sport: item.sport.to_uppercase(),
                season: Some(assignment.season),
            },
        )
        .await?;
        if latest_hash.as_deref() == Some(assignment.input_hash.as_str()) {
            return Ok(Prepared::Debounced);
        }
        previous_score
    };
    let backend = models.inference(manifest::ROUTE)?;
    let output = create(&Studio::new(backend.as_ref()), &assignment).await?;
    Ok(Prepared::Product {
        output: Box::new(output),
        previous_score,
    })
}

pub struct SigilHandler {
    pool: sqlx::PgPool,
    models: ExecutionCapabilities,
}

impl SigilHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for SigilHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::oracle::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let sport = item.sport.to_uppercase();
        let prepared = prepare(pool, models, item).await?;
        let (outcome, product_row_id) = commit_claimed(pool, item, &sport, &prepared).await?;
        if let (PluginOutcome::Committed, Some(product_row_id), Prepared::Product { output, .. }) =
            (&outcome, product_row_id, &prepared)
        {
            record_ledger(pool, item, &sport, output, product_row_id).await;
        }
        Ok(outcome)
    }
}

/// Create one crown from an explicit, service-free assignment.
pub async fn create(studio: &Studio<'_>, assignment: &Assignment) -> Result<SigilOutput> {
    if assignment.cards.readiness() == Readiness::Empty {
        return Ok(Generation::uncalled(
            SigilSynthesis {
                score: None,
                reading: None,
                headline: None,
                season: assignment.season,
                input_components_json: "{}".to_string(),
                convergence: None,
                omen: None,
            },
            studio.model_name().to_string(),
            ORACLE_PROMPT_VERSION,
            Vec::new(),
            None,
        ));
    }

    let cards = &assignment.cards;
    let comparisons =
        build_pillar_divergence(cards.rating.as_ref(), cards.vibe.as_ref(), &cards.momentum);
    let convergence = pillar_convergence(&comparisons);
    let omen = compute_omen(convergence, &cards.momentum);
    let prompt = prompt::assemble(&assignment.subject, cards);
    let options = assignment.options.clone();
    let extracted = studio
        .extract(
            &prompt,
            &options,
            &ReadingParser,
            crate::plugins::support::prompt::structured_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reading = extracted
        .value
        .ok_or_else(|| anyhow!("crown: parser returned no value"))?;
    crate::plugins::support::form::validate_body(&reading)?;
    if !crate::plugins::support::guards::title_names_entity(
        &reading,
        &assignment.subject.entity_name,
    ) {
        tracing::warn!(guard = "entity_identity", "crown reading rejected");
        bail!(
            "crown: reading does not name entity {:?}",
            assignment.subject.entity_name
        );
    }

    Ok(Generation::called(
        SigilSynthesis {
            score: Some(crown_score(cards)),
            reading: Some(reading),
            headline: crate::plugins::support::guards::settle_title(
                "oracle",
                Some(&format!(
                    "{}: the current picture",
                    assignment.subject.entity_name
                )),
            ),
            season: assignment.season,
            input_components_json: assignment.input_components_json.clone(),
            convergence,
            omen: Some(omen),
        },
        model,
        ORACLE_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash.clone()),
        call,
    ))
}

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

fn crown_score(cards: &Cards) -> i32 {
    let mut signals = Vec::new();
    if let Some(rating) = &cards.rating {
        signals.push(rating.notability.clamp(1, 100));
    }
    if let Some(vibe) = &cards.vibe {
        signals.push(vibe.sentiment.clamp(1, 100));
    }
    if let Some(narrative) = cards
        .narratives
        .iter()
        .max_by(|a, b| a.impact.total_cmp(&b.impact))
    {
        signals.push(narrative.impact.round().clamp(1.0, 100.0) as i32);
    }
    if let Some(insider) = &cards.insider {
        signals.push(insider.score.clamp(1, 100));
    }
    if signals.is_empty() {
        50
    } else {
        (signals.iter().sum::<i32>() as f64 / signals.len() as f64).round() as i32
    }
}

#[cfg(test)]
mod tests;
