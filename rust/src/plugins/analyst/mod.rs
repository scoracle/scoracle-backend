//! Analyst execution, deterministic product fields and queue invalidation.
pub mod manifest;
pub mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::application::models::ExecutionCapabilities;
use crate::application::products::EntityKey;
use crate::application::queue::work::{self, Item};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use parser::MomentumParser;
use prompt::{Assignment, MOMENTUM_PROMPT_VERSION};
use publish::{commit_claimed, record_ledger};
use sqlx::{PgPool, Postgres, Transaction};
use tracing::debug;

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";
/// Scores within this band are steady; values at or beyond it rise or fall by sign.
const MOMENTUM_STEADY_BAND: f64 = 10.0;

pub(crate) const MOMENTUM_COMPLETED: &str = "momentum_completed";

pub(crate) async fn record_momentum_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    crate::application::queue::outbox::record(
        tx,
        item,
        crate::application::queue::outbox::NewEvent {
            kind: MOMENTUM_COMPLETED,
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            source_input_version: item.input_version.as_deref(),
        },
    )
    .await
    .context("record momentum completion outbox")
}

#[derive(Clone, Debug)]
pub struct MomentumSummary {
    pub direction: String,
    pub score: i32,
    pub blurb: String,
    /// Model-emitted card title.
    pub headline: Option<String>,
    pub season: i32,
    pub input_components_json: String,
}

pub type MomentumOutput = Generation<MomentumSummary>;

/// Deterministic direction from the ±100-scale signed slope average. No snapshot means steady.
#[cfg(test)]
pub fn momentum_direction_from_score(momentum_score: Option<f64>) -> &'static str {
    match momentum_score {
        Some(s) if s >= MOMENTUM_STEADY_BAND => "rising",
        Some(s) if s <= -MOMENTUM_STEADY_BAND => "falling",
        _ => "steady",
    }
}

/// Deterministic ±5 conviction from `momentum_score`. The steady band maps to zero or a one-point
/// lean; larger absolute scores step through the remaining bands. No snapshot maps to zero.
#[cfg(test)]
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

async fn momentum_metrics(pool: &PgPool, score: Option<f64>) -> Result<(String, i32)> {
    sqlx::query_as(include_str!("metrics.sql"))
        .bind(score)
        .fetch_one(pool)
        .await
        .context("calculate Analyst direction and conviction")
}

pub async fn create(
    pool: &PgPool,
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<Option<MomentumOutput>> {
    let ctx = &assignment.context;
    if ctx.empty() {
        return Ok(None);
    }
    let metrics = momentum_metrics(pool, ctx.snapshot.momentum_score).await?;
    articulate(studio, assignment, metrics).await
}

async fn articulate(
    studio: &Studio<'_>,
    assignment: &Assignment,
    (direction, score): (String, i32),
) -> Result<Option<MomentumOutput>> {
    let ctx = &assignment.context;
    let subject = crate::plugins::meta::EntityMeta {
        name: assignment.entity_name.clone(),
        entity_type: assignment.entity_type.clone(),
        entity_id: assignment.entity_id,
        sport: assignment.sport.clone(),
    };
    let prompt = prompt::assemble(
        &subject,
        ctx.rating.as_ref(),
        ctx.vibe.as_ref(),
        &ctx.snapshot,
    );
    let opts = prompt::generation_options(assignment.voice_num_ctx);
    let extracted = studio
        .extract(
            &prompt,
            &opts,
            &MomentumParser,
            crate::studio::session::structured_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;
    let blurb = reply.blurb;
    let headline = crate::tools::guards::settle_title(
        "analyst",
        Some(&format!("{}: current momentum", assignment.entity_name)),
    );
    Ok(Some(Generation::called(
        MomentumSummary {
            direction,
            score,
            blurb,
            headline,
            season: ctx.season,
            input_components_json: ctx.input_components_json.clone(),
        },
        model,
        MOMENTUM_PROMPT_VERSION,
        Vec::new(),
        Some(ctx.input_hash.clone()),
        call,
    )))
}

pub async fn enqueue_momentum_if_needed(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let ctx = prompt::load_momentum_context(pool, entity_type, entity_id, &sport).await?;
    if ctx.empty() {
        return Ok(false);
    }
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.clone(),
        season: Some(ctx.season),
    };
    if crate::application::products::debounce_unchanged(
        pool,
        "momentum_summaries",
        &key,
        &ctx.input_hash,
    )
    .await?
    {
        return Ok(false);
    }
    let it = Item {
        stage: crate::plugins::analyst::manifest::TASK,
        entity_type: entity_type.to_string(),
        entity_id: i64::from(entity_id),
        sport,
        input_version: Some(momentum_work_input_version(ctx.season, &ctx.input_hash)),
        attempts: 0,
        claim_token: None,
    };
    work::enqueue(pool, &it).await?;
    Ok(true)
}

pub(crate) struct MomentumReaction {
    pool: PgPool,
}

impl MomentumReaction {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl crate::application::queue::outbox::EventReaction for MomentumReaction {
    fn name(&self) -> &'static str {
        "analyst.enqueue-momentum"
    }

    fn kinds(&self) -> &[&'static str] {
        &[
            crate::plugins::influencer::VIBE_COMPLETED,
            crate::plugins::scout::RATING_COMPLETED,
        ]
    }

    async fn react(&self, event: &crate::application::queue::outbox::Event) -> Result<()> {
        if !enqueue_momentum_if_needed(
            &self.pool,
            &event.entity_type,
            event.entity_id,
            &event.sport,
        )
        .await?
        {
            debug!(
                entity_type = %event.entity_type,
                entity_id = event.entity_id,
                sport = %event.sport,
                kind = %event.kind,
                "publication outbox: momentum enqueue skipped unchanged/empty context"
            );
        }
        Ok(())
    }
}

pub fn momentum_work_input_version(season: i32, input_hash: &str) -> String {
    format!("{MOMENTUM_WORK_PREFIX}{season}:{input_hash}")
}

pub struct MomentumHandler {
    pool: sqlx::PgPool,
    models: ExecutionCapabilities,
}

impl MomentumHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for MomentumHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::analyst::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let name = crate::evidence::corpus::lookup_entity_name(
            pool,
            &item.entity_type,
            entity_id,
            &item.sport,
        )
        .await?;
        let context =
            prompt::load_momentum_context(pool, &item.entity_type, entity_id, &sport).await?;
        let assignment = Assignment {
            entity_id,
            entity_type: item.entity_type.clone(),
            entity_name: name,
            sport: item.sport.clone(),
            context,
            voice_num_ctx: models.voice_num_ctx,
        };
        let model = models.inference(crate::plugins::analyst::manifest::ROUTE)?;
        let prepared = create(pool, &Studio::new(model.as_ref()), &assignment).await?;
        if prepared.is_none() {
            debug!(entity_type = %item.entity_type, entity_id, sport = %item.sport, "momentum: skipped empty context");
        }
        let (outcome, product_row_id) =
            commit_claimed(pool, item, &sport, prepared.as_ref()).await?;
        if let (Some(product_row_id), Some(output)) = (product_row_id, &prepared) {
            record_ledger(
                pool,
                item,
                &sport,
                &assignment.context,
                product_row_id,
                output,
            )
            .await?;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests;
