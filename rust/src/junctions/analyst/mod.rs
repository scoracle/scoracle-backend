//! Transitional queue and Postgres adapter for the Analyst's Studio assignment.
//! Retrieval, persistence, and durable scheduling live here; creation lives in `studio/`.

use crate::composition::memories::{self, MemoryRequest, Mission};
use crate::junctions::oracle::{self, SynthMomentum, SynthRating, SynthVibe};
use crate::runtime::harness::{EntityKey, Harness};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::runtime::stage::StageHandler;
use crate::runtime::util::hash_components;
use crate::runtime::work::{self, Item, Stage};
use crate::studio::{Outcome, Publisher, Studio};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tracing::debug;

pub use crate::studio::analyst::{
    generation_options, momentum_conviction_from_score, momentum_direction_from_score,
    parse_momentum_reply, prompt, Assignment, Form, MomentumContext, MomentumOutput,
    MomentumParser, MomentumReply, MomentumSummary, Mood, Snapshot, MOMENTUM_NUM_PREDICT,
    MOMENTUM_OUTPUT_CONTRACT_VERSION, MOMENTUM_PROMPT_VERSION, MOMENTUM_STEADY_BAND,
    MOMENTUM_SYSTEM_PROMPT, MOMENTUM_TEMPERATURE,
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
pub fn build_momentum_prompt(
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
fn build_momentum_input_components(
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
    .fetch_one(pool)
    .await
    .context("persist momentum summary")?;
    Ok(row.get("id"))
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
        let publisher = MomentumPublisher {
            pool: &hx.pool,
            item,
            sport,
            context: &assignment.context,
            voice_num_ctx: hx.voice_num_ctx,
        };
        if matches!(
            crate::studio::analyst::run(&Studio::new(model.as_ref()), &assignment, &publisher)
                .await?,
            Outcome::NoMaterial
        ) {
            debug!(entity_type = %item.entity_type, entity_id, sport = %item.sport, "momentum: skipped empty context");
        }
        Ok(())
    }
}

struct MomentumPublisher<'a> {
    pool: &'a PgPool,
    item: &'a Item,
    sport: String,
    context: &'a MomentumContext,
    voice_num_ctx: i32,
}

#[async_trait]
impl Publisher<MomentumSummary> for MomentumPublisher<'_> {
    type Receipt = i64;
    async fn publish(&self, out: &MomentumOutput) -> Result<i64> {
        let ctx = self.context;
        let product_row_id =
            persist_momentum_summary(self.pool, self.item, &self.sport, out).await?;
        insert_generation_ledger_best_effort(
            self.pool,
            out,
            MOMENTUM_LEDGER,
            LedgerEvent {
                entity_type: &self.item.entity_type,
                entity_id: self.item.entity_id_i32()?,
                sport: &self.sport,
                pair_entity: None,
                trigger_type: "periodic",
                trigger_payload: serde_json::json!({}),
                product_row_ids: vec![product_row_id],
                included_evidence: serde_json::json!({
                    "input_components": serde_json::from_str::<serde_json::Value>(
                        &ctx.input_components_json
                    ).unwrap_or_else(|_| serde_json::json!({
                        "raw_input_components": ctx.input_components_json
                    })),
                    "has_rating": ctx.rating.is_some(),
                    "has_vibe": ctx.vibe.is_some(),
                    "has_momentum_snapshot": !ctx.snapshot.empty(),
                }),
                excluded_evidence: serde_json::json!({"empty_context": ctx.empty()}),
                context_budget: out.context_budget(serde_json::json!({
                    "num_predict": generation_options(self.voice_num_ctx).num_predict,
                    "decided_direction": out.direction,
                    "steady_band": MOMENTUM_STEADY_BAND,
                    "computed_conviction": out.score,
                })),
                parser_outcome: "scored",
            },
        )
        .await;
        Ok(product_row_id)
    }
}

#[cfg(test)]
mod tests;
