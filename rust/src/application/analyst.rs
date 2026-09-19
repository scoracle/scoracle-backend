//! Analyst evidence, publication, and durable work coordination.
//!
//! Studio owns creation from prepared material. This adapter owns Postgres retrieval, sourced
//! memory preparation, queue invalidation, exact-claim publication, and diagnostic ledger writes.

use crate::composition::memories::{self, MemoryRequest, Mission};
use crate::junctions::oracle::{self, SynthMomentum, SynthRating, SynthVibe};
use crate::runtime::harness::{EntityKey, Harness};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::runtime::stage::{HandleOutcome, StageHandler};
use crate::runtime::util::hash_components;
use crate::runtime::work::{self, Item, Stage};
use crate::studio::{analyst, Studio};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::debug;

use crate::studio::analyst::{
    Assignment, Form, MomentumContext, MomentumOutput, Mood, Snapshot,
    MOMENTUM_OUTPUT_CONTRACT_VERSION, MOMENTUM_STEADY_BAND,
};

const MOMENTUM_WORK_PREFIX: &str = "momentum:s";
const MOMENTUM_LEDGER: LedgerSpec = LedgerSpec {
    stage: "momentum",
    lens: "momentum",
    role: Role::MomentumLogic,
    product_table: "momentum_summaries",
    output_contract_version: MOMENTUM_OUTPUT_CONTRACT_VERSION,
};

fn form(value: &SynthRating) -> Form {
    Form {
        notability: value.notability,
        rating_trajectory: value.rating_trajectory.clone(),
        rating_trajectory_label: value.rating_trajectory_label.clone(),
    }
}
fn mood(value: &SynthVibe) -> Mood {
    Mood {
        sentiment: value.sentiment,
    }
}
fn snapshot(value: &SynthMomentum) -> Snapshot {
    Snapshot {
        vibe_slope: value.vibe_slope,
        vibe_samples: value.vibe_samples,
        rating_slope: value.rating_slope,
        rating_samples: value.rating_samples,
        momentum_score: value.momentum_score,
    }
}

/// Compatibility boundary for eval callers. Legacy pillar prose is excluded;
/// prepared sourced memory travels separately.
pub fn build_momentum_prompt_from_pillars(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    identity: Option<&str>,
) -> String {
    crate::studio::analyst::build_momentum_prompt(
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
    crate::studio::analyst::build_momentum_input_components(
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
            |(vibe_slope, vibe_samples, rating_slope, rating_samples, momentum_score)| Snapshot {
                vibe_slope,
                vibe_samples,
                rating_slope,
                rating_samples,
                momentum_score,
            },
        )
        .unwrap_or_default())
}

pub async fn load_momentum_context(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(MomentumContext, memories::Package)> {
    let season = oracle::resolve_season(&hx.pool, sport, None).await?;
    let (rating, vibe, snapshot) = tokio::try_join!(
        oracle::load_rating_pillar(&hx.pool, entity_type, entity_id, sport, Some(season)),
        oracle::load_vibe_pillar(&hx.pool, entity_type, entity_id, sport),
        load_momentum_snapshot(&hx.pool, entity_type, entity_id, sport),
    )?;
    let mut context = MomentumContext::new(
        season,
        rating.as_ref().map(form),
        vibe.as_ref().map(mood),
        snapshot,
    );
    let mut request = MemoryRequest::new(Mission::Analyst, entity_type, entity_id, sport);
    request.season = Some(season);
    let memories = memories::load(&hx.pool, request).await?;
    context.input_components_json =
        memories.with_input_components(&context.input_components_json)?;
    context.input_hash = hash_components(&context.input_components_json);
    Ok((context, memories))
}

pub async fn enqueue_momentum_if_needed(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<bool> {
    let sport = sport.to_uppercase();
    let (ctx, _) = load_momentum_context(hx, entity_type, entity_id, &sport).await?;
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
        claim_token: None,
    };
    work::enqueue(&hx.pool, &it).await?;
    Ok(true)
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

#[cfg(test)]
#[async_trait]
trait PillarHandoff: Sync {
    async fn offer(&self) -> Result<()>;
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum ApplicationOutcome<R> {
    NoMaterial,
    Published(R),
}

/// Service-free mirror of the adapter lifecycle. Production replaces these injected capabilities
/// with one exact-claim transaction and a durable outbox event.
#[cfg(test)]
async fn run_prepared<P: crate::studio::Publisher<analyst::MomentumSummary>, F: PillarHandoff>(
    studio: &Studio<'_>,
    assignment: &Assignment,
    publisher: &P,
    follow_up: &F,
) -> Result<ApplicationOutcome<P::Receipt>> {
    let outcome = match prepare(studio, assignment).await? {
        Prepared::NoMaterial => ApplicationOutcome::NoMaterial,
        Prepared::Product(output) => {
            ApplicationOutcome::Published(publisher.publish(&output).await?)
        }
    };
    follow_up.offer().await?;
    Ok(outcome)
}

async fn record_ledger(
    hx: &Harness,
    item: &Item,
    sport: &str,
    context: &MomentumContext,
    product_row_id: i64,
    out: &MomentumOutput,
) -> Result<()> {
    insert_generation_ledger_best_effort(
        &hx.pool,
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
                "has_rating": context.rating.is_some(),
                "has_vibe": context.vibe.is_some(),
                "has_momentum_snapshot": !context.snapshot.empty(),
            }),
            excluded_evidence: serde_json::json!({"empty_context": context.empty()}),
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": analyst::generation_options(hx.voice_num_ctx).num_predict,
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
) -> Result<(HandleOutcome, Option<i64>)> {
    let mut tx = pool.begin().await.context("begin momentum publication")?;
    if !work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close superseded momentum publication")?;
        return Ok((HandleOutcome::Superseded, None));
    }

    let product_row_id = match prepared {
        Prepared::NoMaterial => None,
        Prepared::Product(output) => {
            Some(persist_momentum_summary(&mut tx, item, sport, output).await?)
        }
    };
    crate::application::outbox::record_momentum_completed(&mut tx, item).await?;
    if !work::complete_in_transaction(&mut tx, item).await? {
        bail!("momentum claim changed while its publication transaction held the row lock");
    }
    tx.commit().await.context("commit momentum publication")?;
    Ok((HandleOutcome::Completed, product_row_id))
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
        self.run_claimed(hx, item).await?;
        Ok(())
    }

    async fn handle_claimed(&self, hx: &Harness, item: &Item) -> Result<HandleOutcome> {
        self.run_claimed(hx, item).await
    }
}

impl MomentumHandler {
    async fn run_claimed(&self, hx: &Harness, item: &Item) -> Result<HandleOutcome> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let name = crate::evidence::corpus::lookup_entity_name(
            &hx.pool,
            &item.entity_type,
            entity_id,
            &item.sport,
        )
        .await?;
        let (context, memories) =
            load_momentum_context(hx, &item.entity_type, entity_id, &sport).await?;
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
            voice_num_ctx: hx.voice_num_ctx,
        };
        let model = hx.router.for_role(Role::MomentumLogic);
        let prepared = prepare(&Studio::new(model.as_ref()), &assignment).await?;
        if matches!(prepared, Prepared::NoMaterial) {
            debug!(entity_type = %item.entity_type, entity_id, sport = %item.sport, "momentum: skipped empty context");
        }
        let (outcome, product_row_id) = commit_claimed(&hx.pool, item, &sport, &prepared).await?;
        if let (Some(product_row_id), Prepared::Product(output)) = (product_row_id, &prepared) {
            record_ledger(
                hx,
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
