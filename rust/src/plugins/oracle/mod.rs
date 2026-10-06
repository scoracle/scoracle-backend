//! Oracle execution, completion barrier and SQL-derived crown fields.
pub mod manifest;
mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::products::EntityKey;
use crate::harness::queue::work::{self, Item, TaskKey};
use crate::harness::{Generation, GenerationCall, Studio};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use parser::ReadingParser;
use prompt::{Assignment, Cards, Readiness, ORACLE_PROMPT_VERSION};
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
    /// Deterministic convergence (1-100) from SQL pillar agreement — NOT model-emitted. `None`
    /// for the marker and when no directional pillar pair exists. NOT part of the `input_hash`.
    pub convergence: Option<i32>,
    /// The computed omen the reading was drawn under by SQL. `None` for the marker.
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
impl crate::harness::queue::outbox::EventReaction for OracleBarrierReaction {
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

    async fn react(&self, event: &crate::harness::queue::outbox::Event) -> Result<()> {
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
        let (previous_score, latest_hash) = crate::harness::products::latest_with_hash(
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
    let output = create(pool, &Studio::new(backend.as_ref()), &assignment).await?;
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

/// Create one crown from the selected finished cards and SQL-derived metrics.
pub async fn create(
    pool: &PgPool,
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<SigilOutput> {
    let metrics = if assignment.cards.readiness() == Readiness::Empty {
        None
    } else {
        Some(load_metrics(pool, &assignment.cards).await?)
    };
    articulate(studio, assignment, metrics).await
}

async fn load_metrics(pool: &PgPool, cards: &Cards) -> Result<(i32, Option<i32>, &'static str)> {
    // Preserve total_cmp selection (including signed zero/NaN); SQL owns the math.
    let impact = cards
        .narratives
        .iter()
        .max_by(|a, b| a.impact.total_cmp(&b.impact))
        .map(|n| n.impact);
    let (score, convergence, omen): (i32, Option<i32>, String) =
        sqlx::query_as(include_str!("metrics.sql"))
            .bind(cards.rating.as_ref().map(|r| r.notability))
            .bind(cards.vibe.as_ref().map(|v| v.sentiment))
            .bind(impact)
            .bind(cards.insider.as_ref().map(|i| i.score))
            .bind(cards.momentum.direction.as_deref())
            .fetch_one(pool)
            .await
            .context("calculate Oracle crown metrics")?;
    let omen = match omen.as_str() {
        "ascendant" => "ascendant",
        "waning" => "waning",
        "steady" => "steady",
        "crossroads" => "crossroads",
        _ => bail!("Oracle SQL returned an unknown omen"),
    };
    Ok((score, convergence, omen))
}

async fn articulate(
    studio: &Studio<'_>,
    assignment: &Assignment,
    metrics: Option<(i32, Option<i32>, &'static str)>,
) -> Result<SigilOutput> {
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
    let (score, convergence, omen) = metrics.context("Oracle nonempty cards need SQL metrics")?;
    let prompt = prompt::assemble(&assignment.subject, cards);
    let options = assignment.options.clone();
    let extracted = studio
        .extract(
            &prompt,
            &options,
            &ReadingParser,
            crate::harness::session::structured_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reading = extracted
        .value
        .ok_or_else(|| anyhow!("crown: parser returned no value"))?;
    crate::tools::form::validate_body(&reading)?;
    if !crate::tools::guards::title_names_entity(&reading, &assignment.subject.entity_name) {
        tracing::warn!(guard = "entity_identity", "crown reading rejected");
        bail!(
            "crown: reading does not name entity {:?}",
            assignment.subject.entity_name
        );
    }

    Ok(Generation::called(
        SigilSynthesis {
            score: Some(score),
            reading: Some(reading),
            headline: crate::tools::guards::settle_title(
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

#[cfg(test)]
mod tests;
