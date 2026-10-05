//! Scout evidence, model selection, publication, and durable work coordination.
//!
//! Preparation belongs to `prompt.rs`; performance reads belong to `performance.rs`.
//! This adapter owns queue policy, exact-claim publication, and diagnostic ledger writes.

use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::plugins::scout::cognition::{
    self as scout, RatingBuild, RatingOutput, MAX_STAT_FACTS, RATING_NUM_PREDICT,
    RATING_OUTPUT_CONTRACT_VERSION, RATING_TEMPERATURE,
};
use crate::plugins::scout::performance::{self, current_season};
use crate::plugins::scout::prompt::{build_rating_request, RatingReq};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::debug;

pub(crate) const RATING_COMPLETED: &str = "rating_completed";
pub(crate) const RATING_DEBOUNCED: &str = "rating_debounced";
pub(crate) const TRANSFER_IDENTITY_APPLIED: &str = "transfer_identity_applied";

pub(crate) async fn record_rating_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    has_product: bool,
) -> Result<()> {
    crate::application::queue::outbox::record(
        tx,
        item,
        crate::application::queue::outbox::NewEvent {
            kind: if has_product {
                RATING_COMPLETED
            } else {
                RATING_DEBOUNCED
            },
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            source_input_version: item.input_version.as_deref(),
        },
    )
    .await
    .context("record rating completion outbox")
}

pub(crate) struct RatingIdentityReaction {
    pool: PgPool,
}

impl RatingIdentityReaction {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl crate::application::queue::outbox::EventReaction for RatingIdentityReaction {
    fn name(&self) -> &'static str {
        "scout.rate-applied-identity"
    }

    fn kinds(&self) -> &[&'static str] {
        &[TRANSFER_IDENTITY_APPLIED]
    }

    async fn react(&self, event: &crate::application::queue::outbox::Event) -> Result<()> {
        let input_version = event
            .source_input_version
            .clone()
            .context("transfer identity rating obligation missing input version")?;
        crate::application::queue::work::enqueue(
            &self.pool,
            &Item {
                stage: crate::plugins::scout::manifest::TASK,
                entity_type: event.entity_type.clone(),
                entity_id: i64::from(event.entity_id),
                sport: event.sport.clone(),
                input_version: Some(input_version),
                attempts: 0,
                claim_token: None,
            },
        )
        .await
        .context("dispatch transfer identity rating obligation")
    }
}

pub(crate) mod harvester;

/// Execution and publication policy for an operator-started Scout run. These contexts are
/// intentionally distinct from [`Item`]: direct and historical runs own no queue claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingRunContext {
    Preview { skip_unchanged: bool },
    PublishSingle { skip_unchanged: bool },
    HistoricalBackfill,
}

/// Prepare, debounce, and create a Scout product. Publication remains a separate short transaction.
async fn generate_rating(
    pool: &sqlx::PgPool,
    models: &ExecutionCapabilities,
    req: &RatingReq,
    temperature: f64,
    skip_unchanged: bool,
    with_enrichment: bool,
) -> Result<RatingOutput> {
    let backend = models.inference(crate::plugins::scout::manifest::ROUTE)?;
    let assignment = match build_rating_request(
        pool,
        models.voice_num_ctx,
        req,
        temperature,
        with_enrichment,
    )
    .await?
    {
        RatingBuild::NoStats { season } => return Ok(scout::no_stats(season, backend.model())),
        RatingBuild::Ready(assignment) => *assignment,
    };
    if skip_unchanged
        && performance::last_commentary_input_hash(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            assignment.season,
        )
        .await?
        .as_deref()
            == Some(assignment.input_hash.as_str())
    {
        return Ok(scout::unchanged(assignment, backend.model()));
    }
    scout::create(&Studio::new(backend.as_ref()), assignment).await
}

/// Invoke Scout outside the durable worker, preserving the established direct/backfill
/// publication rules without manufacturing a live queue claim.
pub async fn invoke_rating(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    req: &RatingReq,
    context: RatingRunContext,
) -> Result<RatingOutput> {
    let (skip_unchanged, publish, enqueue_momentum) = match context {
        RatingRunContext::Preview { skip_unchanged } => (skip_unchanged, false, false),
        RatingRunContext::PublishSingle { skip_unchanged } => (skip_unchanged, true, true),
        RatingRunContext::HistoricalBackfill => (false, true, false),
    };
    let output =
        generate_rating(pool, models, req, RATING_TEMPERATURE, skip_unchanged, true).await?;
    if publish && !output.skipped_unchanged {
        persist_stat_summary(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            &req.trigger_type,
            &serde_json::json!({}),
            &output,
        )
        .await?;
        if enqueue_momentum {
            crate::plugins::analyst::adapter::enqueue_momentum_if_needed(
                pool,
                &req.entity_type,
                req.entity_id,
                &req.sport,
            )
            .await?;
        }
    }
    Ok(output)
}

const RATING_WORK_PREFIX: &str = "rating:s";
const RATING_WORK_TRANSFER_MARK: &str = "xfer";
const RATING_WORK_AVAIL_MARK: &str = "avail";
const PACKET_WORK_PREFIX: &str = "pk:";

const RATING_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::scout::manifest::MANIFEST.id.as_str(),
    stage: "rating",
    lens: "rating",
    role: crate::plugins::scout::manifest::ROUTE,
    product_table: "stat_summaries",
    output_contract_version: RATING_OUTPUT_CONTRACT_VERSION,
};

/// Durable queue fingerprint for a rating-card demand. The input hash already includes the prompt
/// version, so the queue needs only season and hash (or an explicit marker).
pub fn rating_work_input_version(season: i32, input_hash: Option<&str>) -> String {
    format!(
        "{RATING_WORK_PREFIX}{season}:{}",
        input_hash.filter(|s| !s.is_empty()).unwrap_or("no-stats")
    )
}

/// Version for a rating opened by an adjudicated transfer. The application ID reopens the work
/// row even though stats did not move; the handler also bypasses the stats-only debounce.
pub fn rating_work_input_version_for_transfer(season: i32, application_id: i64) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_WORK_TRANSFER_MARK}{application_id}")
}

/// Version for a rating opened by an applied injury or suspension. Keying by event day reopens
/// unchanged stats while collapsing multiple same-day events into one work row.
pub fn rating_work_input_version_for_availability(season: i32, day: &str) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_WORK_AVAIL_MARK}{day}")
}

fn rating_work_mark(input_version: Option<&str>) -> Option<&str> {
    input_version
        .and_then(|raw| raw.strip_prefix(RATING_WORK_PREFIX))
        .and_then(|rest| rest.rsplit_once(':'))
        .map(|(_, mark)| mark)
}

pub(crate) fn rating_work_is_transfer_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_TRANSFER_MARK))
}

pub(crate) fn rating_work_is_availability_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_AVAIL_MARK))
}

fn rating_work_is_packet_triggered(input_version: Option<&str>) -> bool {
    input_version.is_some_and(|raw| raw.starts_with(PACKET_WORK_PREFIX))
}

pub(crate) fn rating_trigger_type(input_version: Option<&str>) -> &'static str {
    if rating_work_is_packet_triggered(input_version) {
        return "availability";
    }
    match rating_work_mark(input_version) {
        Some(h) if h.starts_with(RATING_WORK_TRANSFER_MARK) => "transfer",
        Some(h) if h.starts_with(RATING_WORK_AVAIL_MARK) => "availability",
        _ => "periodic",
    }
}

pub(crate) fn rating_work_bypasses_debounce(input_version: Option<&str>) -> bool {
    rating_work_is_transfer_triggered(input_version)
        || rating_work_is_availability_triggered(input_version)
        || rating_work_is_packet_triggered(input_version)
}

pub(crate) fn rating_work_season(input_version: Option<&str>) -> Option<i32> {
    let raw = input_version?;
    let rest = raw.strip_prefix(RATING_WORK_PREFIX)?;
    let (season, _) = rest.split_once(':')?;
    season.parse::<i32>().ok().filter(|s| *s > 0)
}

async fn insert_stat_summary(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &RatingOutput,
) -> Result<i64> {
    let season: Option<i32> = (out.season > 0).then_some(out.season);
    let notability: Option<i16> = out.notability.map(|n| n as i16);
    let prov = &out.provenance;
    let trigger_json = trigger_payload.to_string();
    let ncomp_json = out.notability_components.to_string();
    let trajectory_components_json = out.rating_trajectory_components.to_string();

    let row = sqlx::query(
        r#"
        INSERT INTO stat_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            body, headline, notability, notability_components, input_components, input_hash,
            model_version, prompt_version, generated_at,
            rating_trajectory, rating_trajectory_label, rating_trajectory_components
        ) VALUES ($1,$2,$3,$4,$5,$6::jsonb, $7,$8,$9,$10::jsonb,$11::jsonb,$12, $13,$14,NOW(),
                  $15,$16,$17::jsonb)
        RETURNING id
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .bind(trigger_type)
    .bind(&trigger_json)
    .bind(out.body.as_deref())
    .bind(out.headline.as_deref())
    .bind(notability)
    .bind(&ncomp_json)
    .bind(&out.input_components)
    .bind(prov.input_hash.as_deref())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(out.rating_trajectory.as_deref())
    .bind(out.rating_trajectory_label.as_deref())
    .bind(&trajectory_components_json)
    .fetch_one(&mut **tx)
    .await
    .context("persist stat summary")?;
    Ok(row.get("id"))
}

struct LedgerSubject<'a> {
    entity_type: &'a str,
    entity_id: i32,
    sport: &'a str,
    trigger_type: &'a str,
    trigger_payload: &'a serde_json::Value,
}

async fn record_ledger(
    pool: &PgPool,
    subject: &LedgerSubject<'_>,
    product_row_id: i64,
    out: &RatingOutput,
) -> Result<()> {
    let included_evidence = serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&out.input_components)
            .unwrap_or_else(|_| serde_json::json!({
                "raw_input_components": &out.input_components
            })),
        "notability": out.notability,
        "notability_components": &out.notability_components,
        "rating_trajectory": &out.rating_trajectory,
        "rating_trajectory_label": &out.rating_trajectory_label,
        "rating_trajectory_components": &out.rating_trajectory_components,
    });
    let mut excluded = Vec::new();
    if out.abstained {
        excluded.push(serde_json::json!({"reason": "model_abstained"}));
    }
    if out.skipped_no_stats {
        excluded.push(serde_json::json!({"reason": "no_usable_rating_profile"}));
    }
    if out.skipped_unchanged {
        excluded.push(serde_json::json!({"reason": "input_hash_unchanged"}));
    }
    if !out.exclusions.budget_truncated_stat_labels.is_empty() {
        let labels = &out.exclusions.budget_truncated_stat_labels;
        excluded.push(serde_json::json!({
            "reason": "budget_truncated_stat_facts",
            "dropped_count": labels.len(),
            "dropped_stat_labels": labels,
            "limit": MAX_STAT_FACTS,
        }));
    }
    for (reason, labels) in [
        (
            "off_facet_position_mismatch",
            &out.exclusions.off_facet_stat_labels,
        ),
        (
            "degenerate_zero_usage_artifact",
            &out.exclusions.degenerate_zero_stat_labels,
        ),
        (
            "display_tier_retired_from_equation",
            &out.exclusions.display_tier_stat_labels,
        ),
        (
            "thin_sample_omitted_in_selection",
            &out.exclusions.thin_sample_omitted_stat_labels,
        ),
    ] {
        if !labels.is_empty() {
            excluded.push(serde_json::json!({
                "reason": reason,
                "dropped_count": labels.len(),
                "dropped_stat_labels": labels,
            }));
        }
    }
    insert_generation_ledger_best_effort(
        pool,
        out,
        RATING_LEDGER,
        LedgerEvent {
            entity_type: subject.entity_type,
            entity_id: subject.entity_id,
            sport: subject.sport,
            pair_entity: None,
            trigger_type: subject.trigger_type,
            trigger_payload: subject.trigger_payload.clone(),
            product_row_ids: vec![product_row_id],
            included_evidence,
            excluded_evidence: serde_json::json!(excluded),
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": out.request_body()
                    .and_then(|body| body.pointer("/options/num_predict"))
                    .and_then(|value| value.as_i64())
                    .unwrap_or(RATING_NUM_PREDICT as i64),
            })),
            parser_outcome: if out.abstained {
                "abstained"
            } else if out.skipped_no_stats {
                "no_call"
            } else {
                "parsed"
            },
        },
    )
    .await;
    Ok(())
}

/// Standalone/backfill persistence keeps its historical caller-controlled scheduling behavior.
/// The product commits first; the optional cognition ledger remains best-effort afterward.
async fn persist_stat_summary(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &RatingOutput,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin standalone stat publication")?;
    let product_row_id = insert_stat_summary(
        &mut tx,
        entity_type,
        entity_id,
        sport,
        trigger_type,
        trigger_payload,
        out,
    )
    .await?;
    tx.commit()
        .await
        .context("commit standalone stat publication")?;
    record_ledger(
        pool,
        &LedgerSubject {
            entity_type,
            entity_id,
            sport,
            trigger_type,
            trigger_payload,
        },
        product_row_id,
        out,
    )
    .await
}

enum Prepared<'a> {
    Debounced,
    Product(&'a RatingOutput),
}

fn prepare(out: &RatingOutput) -> Prepared<'_> {
    if out.skipped_unchanged {
        Prepared::Debounced
    } else {
        Prepared::Product(out)
    }
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    prepared: &Prepared<'_>,
) -> Result<(PluginOutcome, Option<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, None));
    };

    let product_row_id = match prepared {
        Prepared::Debounced => None,
        Prepared::Product(output) => Some(
            insert_stat_summary(
                publication.transaction(),
                &item.entity_type,
                item.entity_id_i32()?,
                sport,
                trigger_type,
                trigger_payload,
                output,
            )
            .await?,
        ),
    };
    record_rating_completed(
        publication.transaction(),
        item,
        matches!(prepared, Prepared::Product(_)),
    )
    .await?;
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_id))
}

/// Queue-owned Scout adapter. Creation remains outside the short publication transaction.
pub struct RatingHandler {
    pool: sqlx::PgPool,
    models: ExecutionCapabilities,
}

impl RatingHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for RatingHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::scout::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        if item.input_version.as_deref().is_some_and(|version| {
            version.starts_with(crate::plugins::harvester::context::CONTRACT)
        }) {
            return harvester::execute(&self.pool, &self.models, item).await;
        }
        let pool = &self.pool;
        let models = &self.models;
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let season = match rating_work_season(item.input_version.as_deref()) {
            Some(season) => season,
            None => current_season(pool, &sport).await?,
        };
        let name =
            crate::evidence::corpus::lookup_entity_name(pool, &item.entity_type, entity_id, &sport)
                .await?;
        let bypass = rating_work_bypasses_debounce(item.input_version.as_deref());
        let trigger_type = rating_trigger_type(item.input_version.as_deref());
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            trigger_type: trigger_type.to_string(),
            season: Some(season),
        };

        let output = generate_rating(pool, models, &req, RATING_TEMPERATURE, !bypass, true).await?;
        let prepared = prepare(&output);
        if matches!(prepared, Prepared::Debounced) {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                season = output.season,
                "rating: skipped unchanged rating input"
            );
        }
        let trigger_payload = serde_json::json!({});
        let (outcome, product_row_id) = commit_claimed(
            pool,
            item,
            &sport,
            trigger_type,
            &trigger_payload,
            &prepared,
        )
        .await?;
        if let (Some(product_row_id), Prepared::Product(output)) = (product_row_id, prepared) {
            record_ledger(
                pool,
                &LedgerSubject {
                    entity_type: &item.entity_type,
                    entity_id,
                    sport: &sport,
                    trigger_type,
                    trigger_payload: &trigger_payload,
                },
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
