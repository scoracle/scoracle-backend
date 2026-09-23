//! Analyst evidence, publication, and durable work coordination.
//!
//! Studio owns creation from prepared material. This adapter owns Postgres retrieval, sourced
//! memory preparation, queue invalidation, exact-claim publication, and diagnostic ledger writes.

use crate::application::models::Models;
use crate::application::products::EntityKey;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::{self, Item, Stage};
use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::plugins::analyst::cognition as analyst;
use crate::plugins::oracle::adapter as oracle;
use crate::plugins::oracle::cognition::{SynthMomentum, SynthRating, SynthVibe};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use crate::util::hash_components;
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::debug;

use crate::plugins::analyst::cognition::{
    Assignment, Form, MomentumContext, MomentumOutput, Mood, Snapshot,
    MOMENTUM_OUTPUT_CONTRACT_VERSION, MOMENTUM_STEADY_BAND,
};

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";
type ScoutReadingRow = (String, Option<String>, Option<i32>, String, Option<String>);
type InfluencerReadingRow = (String, Option<String>, Option<i16>, String, Option<String>);
const MOMENTUM_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::analyst::manifest::MANIFEST.id.as_str(),
    stage: "momentum",
    lens: "momentum",
    role: Role::MomentumLogic,
    product_table: "momentum_summaries",
    output_contract_version: MOMENTUM_OUTPUT_CONTRACT_VERSION,
};

fn form(value: &SynthRating) -> Form {
    Form {
        body: value.body.clone(),
        headline: None,
        season: None,
        generated_at: None,
        input_hash: None,
    }
}
fn mood(value: &SynthVibe) -> Mood {
    Mood {
        body: value.prompt.clone(),
        headline: None,
        sentiment: Some(value.sentiment),
        generated_at: None,
        input_hash: None,
    }
}
fn snapshot(value: &SynthMomentum) -> Snapshot {
    Snapshot {
        vibe_slope: value.vibe_slope,
        vibe_samples: value.vibe_samples,
        vibe_window_start: None,
        vibe_window_end: None,
        rating_slope: value.rating_slope,
        rating_samples: value.rating_samples,
        rating_window_start: None,
        rating_window_end: None,
        momentum_score: value.momentum_score,
        generated_at: None,
    }
}

/// Compatibility boundary for retained callers. The finished pillar prose is the material;
/// production additionally supplies card dates and the dated study window.
pub fn build_momentum_prompt_from_pillars(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    identity: Option<&str>,
) -> String {
    crate::plugins::analyst::cognition::build_momentum_prompt(
        entity_type,
        entity_name,
        sport,
        rating.map(form).as_ref(),
        vibe.map(mood).as_ref(),
        &snapshot(mom),
        identity,
    )
}

#[cfg(test)]
fn build_momentum_input_components_from_pillars(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    crate::plugins::analyst::cognition::build_momentum_input_components(
        rating.map(form).as_ref(),
        vibe.map(mood).as_ref(),
        &snapshot(mom),
    )
}

pub async fn load_momentum_snapshot(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Snapshot> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<f64>,
        i32,
        Option<String>,
        Option<String>,
        Option<f64>,
        i32,
        Option<String>,
        Option<String>,
        Option<f64>,
        String,
    )> = sqlx::query_as(
        r#"
        SELECT vibe_slope::float8, vibe_samples,
               vibe_window_start::date::text, vibe_window_end::date::text,
               rating_slope::float8, rating_samples,
               rating_window_start::date::text, rating_window_end::date::text,
               momentum_score::float8, generated_at::date::text
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
            |(
                vibe_slope,
                vibe_samples,
                vibe_window_start,
                vibe_window_end,
                rating_slope,
                rating_samples,
                rating_window_start,
                rating_window_end,
                momentum_score,
                generated_at,
            )| Snapshot {
                vibe_slope,
                vibe_samples,
                vibe_window_start,
                vibe_window_end,
                rating_slope,
                rating_samples,
                rating_window_start,
                rating_window_end,
                momentum_score,
                generated_at: Some(generated_at),
            },
        )
        .unwrap_or_default())
}

async fn load_scout_reading(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<Form>> {
    let row: Option<ScoutReadingRow> = sqlx::query_as(
        r#"
        SELECT body, headline, season, generated_at::date::text, input_hash
          FROM (
            SELECT body, headline, season, generated_at, input_hash
              FROM public.stat_summaries
             WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
             ORDER BY generated_at DESC, id DESC
             LIMIT 1
          ) latest
         WHERE body IS NOT NULL AND btrim(body) <> ''
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load Analyst Scout reading {entity_type}/{entity_id}"))?;
    Ok(
        row.map(|(body, headline, season, generated_at, input_hash)| Form {
            body,
            headline,
            season,
            generated_at: Some(generated_at),
            input_hash,
        }),
    )
}

async fn load_influencer_reading(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<Mood>> {
    let row: Option<InfluencerReadingRow> = sqlx::query_as(
        r#"
        SELECT prompt, hook, sentiment, generated_at::date::text, input_hash
          FROM public.vibe_scores
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND prompt IS NOT NULL AND btrim(prompt) <> ''
         ORDER BY generated_at DESC, id DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load Analyst Influencer reading {entity_type}/{entity_id}"))?;
    Ok(row.map(
        |(body, headline, sentiment, generated_at, input_hash)| Mood {
            body,
            headline,
            sentiment: sentiment.map(i32::from),
            generated_at: Some(generated_at),
            input_hash,
        },
    ))
}

pub async fn load_momentum_context(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(MomentumContext, memories::Package)> {
    let season = oracle::resolve_season(pool, sport, None).await?;
    let (rating, vibe, snapshot) = tokio::try_join!(
        load_scout_reading(pool, entity_type, entity_id, sport, season),
        load_influencer_reading(pool, entity_type, entity_id, sport),
        load_momentum_snapshot(pool, entity_type, entity_id, sport),
    )?;
    let mut context = MomentumContext::new(season, rating, vibe, snapshot);
    let mut request = MemoryRequest::new(Mission::Analyst, entity_type, entity_id, sport);
    request.season = Some(season);
    let memories = memories::load(pool, request).await?;
    context.input_components_json =
        memories.with_input_components(&context.input_components_json)?;
    context.input_hash = hash_components(&context.input_components_json);
    Ok((context, memories))
}

pub async fn enqueue_momentum_if_needed(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let (ctx, _) = load_momentum_context(pool, entity_type, entity_id, &sport).await?;
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
        stage: Stage::Momentum,
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
            crate::application::queue::outbox::VIBE_COMPLETED,
            crate::application::queue::outbox::RATING_COMPLETED,
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

async fn persist_momentum_summary(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    sport: &str,
    out: &MomentumOutput,
) -> Result<i64> {
    let trigger_payload = serde_json::json!({});
    let row = sqlx::query(
        r#"
        INSERT INTO public.momentum_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            direction, score, blurb, headline, input_components, input_hash,
            model_version, prompt_version, generated_at
        ) VALUES ($1,$2,$3,$4,'periodic',$5::jsonb,$6,$7,$8,$9,$10::jsonb,$11,$12,$13,NOW())
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
    .bind(&out.headline)
    .bind(&out.input_components_json)
    .bind(out.provenance.input_hash.as_deref())
    .bind(&out.provenance.model_version)
    .bind(out.provenance.prompt_version)
    .fetch_one(&mut **tx)
    .await
    .context("persist momentum summary")?;
    Ok(row.get("id"))
}

enum Prepared {
    NoMaterial,
    Product(Box<MomentumOutput>),
}

async fn prepare(studio: &Studio<'_>, assignment: &Assignment) -> Result<Prepared> {
    Ok(match analyst::create(studio, assignment).await? {
        Some(output) => Prepared::Product(Box::new(output)),
        None => Prepared::NoMaterial,
    })
}

async fn record_ledger(
    pool: &sqlx::PgPool,
    models: &Models,
    item: &Item,
    sport: &str,
    context: &MomentumContext,
    product_row_id: i64,
    out: &MomentumOutput,
) -> Result<()> {
    insert_generation_ledger_best_effort(
        pool,
        out,
        MOMENTUM_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::json!({}),
            product_row_ids: vec![product_row_id],
            included_evidence: serde_json::json!({
                "input_components": serde_json::from_str::<serde_json::Value>(
                    &context.input_components_json
                ).unwrap_or_else(|_| serde_json::json!({
                    "raw_input_components": context.input_components_json
                })),
                "has_scout_reading": context.rating.is_some(),
                "has_influencer_reading": context.vibe.is_some(),
                "has_momentum_snapshot": !context.snapshot.empty(),
            }),
            excluded_evidence: serde_json::json!({"empty_context": context.empty()}),
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": analyst::generation_options(models.voice_num_ctx).num_predict,
                "decided_direction": out.direction,
                "steady_band": MOMENTUM_STEADY_BAND,
                "computed_conviction": out.score,
            })),
            parser_outcome: "scored",
        },
    )
    .await;
    Ok(())
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    prepared: &Prepared,
) -> Result<(PluginOutcome, Option<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, None));
    };

    let product_row_id = match prepared {
        Prepared::NoMaterial => None,
        Prepared::Product(output) => {
            Some(persist_momentum_summary(publication.transaction(), item, sport, output).await?)
        }
    };
    crate::application::queue::outbox::record_momentum_completed(publication.transaction(), item)
        .await?;
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_id))
}

pub struct MomentumHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

impl MomentumHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
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
        let (context, memories) =
            load_momentum_context(pool, &item.entity_type, entity_id, &sport).await?;
        let assignment = Assignment {
            entity_type: item.entity_type.clone(),
            entity_name: name,
            sport: item.sport.clone(),
            memory: if context.empty() {
                None
            } else {
                Some(memories.render_for_model()?)
            },
            context,
            voice_num_ctx: models.voice_num_ctx,
        };
        let model = models.router.for_role(Role::MomentumLogic);
        let prepared = prepare(&Studio::new(model.as_ref()), &assignment).await?;
        if matches!(prepared, Prepared::NoMaterial) {
            debug!(entity_type = %item.entity_type, entity_id, sport = %item.sport, "momentum: skipped empty context");
        }
        let (outcome, product_row_id) = commit_claimed(pool, item, &sport, &prepared).await?;
        if let (Some(product_row_id), Prepared::Product(output)) = (product_row_id, &prepared) {
            record_ledger(
                pool,
                models,
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
