//! Scout publication and durable work coordination.
//!
//! The existing Scout evidence/creation core remains in `junctions::scout` for this bounded
//! publication slice. This adapter owns queue versions, rerun triggers, exact-claim publication,
//! standalone persistence, required follow-up intent, and the diagnostic ledger.

use crate::junctions::scout::{
    generate_rating, RatingOutput, RatingReq, MAX_STAT_FACTS, RATING_NUM_PREDICT,
    RATING_OUTPUT_CONTRACT_VERSION, RATING_TEMPERATURE,
};
use crate::runtime::harness::Harness;
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::runtime::stage::{HandleOutcome, StageHandler};
use crate::runtime::work::{self, Item, Stage};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::{debug, warn};

const RATING_WORK_PREFIX: &str = "rating:s";
const RATING_WORK_TRANSFER_MARK: &str = "xfer";
const RATING_WORK_AVAIL_MARK: &str = "avail";
const PACKET_WORK_PREFIX: &str = "pk:";

const RATING_LEDGER: LedgerSpec = LedgerSpec {
    stage: "rating",
    lens: "rating",
    role: Role::StatsLogic,
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

async fn current_season(pool: &PgPool, sport: &str) -> Result<i32> {
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("current season {sport}"))
}

/// Best-effort Scout trigger when a transfer becomes a roster fact. Offer the player and both
/// clubs; the nightly batch remains the backstop.
pub async fn enqueue_rating_for_applied_transfer(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    old_team_id: Option<i32>,
    new_team_id: Option<i32>,
    application_id: i64,
) -> Result<()> {
    let sport = sport.to_uppercase();
    let season = current_season(pool, &sport).await?;
    let input_version = rating_work_input_version_for_transfer(season, application_id);

    let mut targets: Vec<(&str, i64)> = vec![("player", i64::from(player_id))];
    for team in [old_team_id, new_team_id].into_iter().flatten() {
        targets.push(("team", i64::from(team)));
    }

    for (entity_type, entity_id) in targets {
        let item = Item {
            stage: Stage::Rating,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.clone(),
            input_version: Some(input_version.clone()),
            attempts: 0,
            claim_token: None,
        };
        if let Err(error) = work::enqueue(pool, &item).await {
            warn!(
                application_id,
                entity_type,
                entity_id,
                sport = %sport,
                "rating: could not enqueue on applied transfer: {error:#}"
            );
        }
    }
    Ok(())
}

/// Best-effort Scout trigger when reported unavailability becomes a roster fact. The stored
/// `event_day` must be supplied as the database DATE text (`YYYY-MM-DD`).
pub async fn enqueue_rating_for_applied_availability(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: Option<i32>,
    event_day: &str,
) -> Result<()> {
    let sport = sport.to_uppercase();
    let season = current_season(pool, &sport).await?;
    let input_version = rating_work_input_version_for_availability(season, event_day);

    let mut targets: Vec<(&str, i64)> = vec![("player", i64::from(player_id))];
    if let Some(team) = team_id {
        targets.push(("team", i64::from(team)));
    }

    for (entity_type, entity_id) in targets {
        let item = Item {
            stage: Stage::Rating,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.clone(),
            input_version: Some(input_version.clone()),
            attempts: 0,
            claim_token: None,
        };
        if let Err(error) = work::enqueue(pool, &item).await {
            warn!(
                event_day,
                entity_type,
                entity_id,
                sport = %sport,
                "rating: could not enqueue on applied availability: {error:#}"
            );
        }
    }
    Ok(())
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
            parser_outcome: if out.skipped_no_stats {
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
pub async fn persist_stat_summary(
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
) -> Result<(HandleOutcome, Option<i64>)> {
    let mut tx = pool.begin().await.context("begin rating publication")?;
    if !work::lock_claim(&mut tx, item).await? {
        tx.rollback()
            .await
            .context("close superseded rating publication")?;
        return Ok((HandleOutcome::Superseded, None));
    }

    let product_row_id = match prepared {
        Prepared::Debounced => None,
        Prepared::Product(output) => Some(
            insert_stat_summary(
                &mut tx,
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
    crate::application::outbox::record_rating_completed(
        &mut tx,
        item,
        matches!(prepared, Prepared::Product(_)),
    )
    .await?;
    if !work::complete_in_transaction(&mut tx, item).await? {
        bail!("rating claim changed while its publication transaction held the row lock");
    }
    tx.commit().await.context("commit rating publication")?;
    Ok((HandleOutcome::Completed, product_row_id))
}

/// Queue-owned Scout adapter. Creation remains outside the short publication transaction.
pub struct RatingHandler;

impl RatingHandler {
    pub fn new() -> Self {
        Self
    }

    async fn run_claimed(&self, hx: &Harness, item: &Item) -> Result<HandleOutcome> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let season = match rating_work_season(item.input_version.as_deref()) {
            Some(season) => season,
            None => current_season(&hx.pool, &sport).await?,
        };
        let name = crate::evidence::corpus::lookup_entity_name(
            &hx.pool,
            &item.entity_type,
            entity_id,
            &sport,
        )
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

        let output = generate_rating(hx, &req, RATING_TEMPERATURE, !bypass, true).await?;
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
            &hx.pool,
            item,
            &sport,
            trigger_type,
            &trigger_payload,
            &prepared,
        )
        .await?;
        if let (Some(product_row_id), Prepared::Product(output)) = (product_row_id, prepared) {
            record_ledger(
                &hx.pool,
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

impl Default for RatingHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for RatingHandler {
    fn stage(&self) -> Stage {
        Stage::Rating
    }

    fn max_in_flight(&self) -> usize {
        2
    }

    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::runtime::stage::ARCHBOX_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        self.run_claimed(hx, item).await?;
        Ok(())
    }

    async fn handle_claimed(&self, hx: &Harness, item: &Item) -> Result<HandleOutcome> {
        self.run_claimed(hx, item).await
    }
}

#[cfg(test)]
mod tests;
