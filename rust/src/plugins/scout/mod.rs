//! Scout creation, queue policy, and durable execution.
//!
//! Preparation belongs to `prompt.rs`; performance reads belong to `performance.rs`.
//! Publication transactions and receipts belong to `publish.rs`.

pub(crate) mod delivery;
pub mod manifest;
pub mod memories;
pub mod parser;
pub mod performance;
pub mod prompt;
mod publish;
pub mod voice;

use crate::application::models::ExecutionCapabilities;
use crate::application::queue::work::Item;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::{Context, Result};
use async_trait::async_trait;
use parser::RatingRequestParser;
use performance::{current_season, RatingExclusions};
use prompt::{
    build_rating_request, Assignment, RatingBuild, RatingReq, RATING_PROMPT_VERSION,
    RATING_TEMPERATURE,
};
use publish::{
    commit_claimed, persist_stat_summary, prepare, record_ledger, LedgerSubject, Prepared,
};
use sqlx::{PgPool, Postgres, Transaction};
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
        RatingBuild::NoStats { season } => return Ok(no_stats(season, backend.model())),
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
        return Ok(unchanged(assignment, backend.model()));
    }
    create(&Studio::new(backend.as_ref()), assignment).await
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
            crate::plugins::analyst::enqueue_momentum_if_needed(
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
            let backend = self.models.inference(manifest::ROUTE)?;
            return delivery::execute_with_backend(
                &self.pool,
                backend.as_ref(),
                self.models.voice_num_ctx,
                item,
            )
            .await;
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

/// The un-persisted result of one Scout creation.
#[derive(Clone, Debug)]
pub struct RatingProduct {
    pub season: i32,
    pub skipped_no_stats: bool,
    /// The model was called and explicitly chose not to publish prose.
    pub abstained: bool,
    pub skipped_unchanged: bool,
    pub body: Option<String>,
    pub headline: Option<String>,
    pub notability: Option<i32>,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: Option<String>,
    pub rating_trajectory_label: Option<String>,
    pub rating_trajectory_components: serde_json::Value,
    pub input_components: String,
    pub exclusions: RatingExclusions,
}

pub type RatingOutput = Generation<RatingProduct>;

/// A missing usable profile is a real uncalled product marker, not model abstention.
pub fn no_stats(season: i32, configured_model: impl Into<String>) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            season,
            skipped_no_stats: true,
            abstained: false,
            skipped_unchanged: false,
            body: None,
            headline: None,
            notability: None,
            notability_components: serde_json::json!({}),
            rating_trajectory: None,
            rating_trajectory_label: None,
            rating_trajectory_components: serde_json::json!({}),
            input_components: "{}".to_string(),
            exclusions: RatingExclusions::default(),
        },
        configured_model.into(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        None,
    )
}

/// An unchanged prepared assignment completes without a call or new product row.
pub fn unchanged(assignment: Assignment, configured_model: impl Into<String>) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            season: assignment.season,
            skipped_no_stats: false,
            abstained: false,
            skipped_unchanged: true,
            body: None,
            headline: None,
            notability: None,
            notability_components: serde_json::json!({}),
            rating_trajectory: Some(assignment.rating_trajectory.key),
            rating_trajectory_label: assignment.rating_trajectory.label,
            rating_trajectory_components: assignment.rating_trajectory.components,
            input_components: assignment.input_components,
            exclusions: assignment.exclusions,
        },
        configured_model.into(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash),
    )
}

/// Articulate the prepared world and attach deterministic product fields.
pub async fn create(studio: &Studio<'_>, assignment: Assignment) -> Result<RatingOutput> {
    let directions = assignment.parts.comparison_directions();
    let bands = assignment.parts.measurement_bands();
    let extracted = studio
        .extract(
            &assignment.built_prompt,
            &assignment.opts,
            &RatingRequestParser::new(&assignment.built_prompt, &directions, &bands),
            crate::plugins::scout::prompt::correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let Some(reply) = extracted.value else {
        // A called pass is a complete response: the model was asked, and
        // declined. That is distinct from a missing usable profile, which is an
        // uncalled marker, and distinct from a malformed card, which is an error.
        return Ok(Generation::called(
            RatingProduct {
                season: assignment.season,
                skipped_no_stats: false,
                abstained: true,
                skipped_unchanged: false,
                body: None,
                headline: None,
                notability: Some(assignment.notability),
                notability_components: assignment.notability_components,
                rating_trajectory: Some(assignment.rating_trajectory.key),
                rating_trajectory_label: assignment.rating_trajectory.label,
                rating_trajectory_components: assignment.rating_trajectory.components,
                input_components: assignment.input_components,
                exclusions: assignment.exclusions,
            },
            extracted.model,
            RATING_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash),
            call,
        ));
    };
    // The title is the plugin's own, not the model's: it names the entity and the
    // kind of read, which is a fact rather than something to articulate.
    let headline = crate::plugins::support::guards::settle_title(
        "scout",
        Some(&format!(
            "{}: measured profile",
            assignment.subject.entity_name
        )),
    );

    Ok(Generation::called(
        RatingProduct {
            season: assignment.season,
            skipped_no_stats: false,
            abstained: false,
            skipped_unchanged: false,
            body: Some(reply.body),
            headline,
            notability: Some(assignment.notability),
            notability_components: assignment.notability_components,
            rating_trajectory: Some(assignment.rating_trajectory.key),
            rating_trajectory_label: assignment.rating_trajectory.label,
            rating_trajectory_components: assignment.rating_trajectory.components,
            input_components: assignment.input_components,
            exclusions: assignment.exclusions,
        },
        extracted.model,
        RATING_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash),
        call,
    ))
}

#[cfg(test)]
mod tests;
