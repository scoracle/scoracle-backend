//! Scout evidence, model selection, publication, and durable work coordination.
//!
//! Studio owns creation from prepared material. This adapter owns concrete Postgres retrieval,
//! assignment preparation, queue policy, exact-claim publication, and diagnostic ledger writes.

use crate::application::models::Models;
use crate::application::queue::stage::{HandleOutcome, WorkHandler};
use crate::application::queue::work::{self, Item, Stage};
use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::model::GenerateOptions;
use crate::studio::scout::{
    self, Assignment, RatingBuild, RatingExclusions, RatingOutput, Subject, MAX_STAT_FACTS,
    RATING_NUM_PREDICT, RATING_OUTPUT_CONTRACT_VERSION, RATING_SYSTEM_PROMPT, RATING_TEMPERATURE,
};
use crate::studio::Studio;
use crate::util::hash_components;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::{debug, warn};

mod evidence;
mod materials;
pub use materials::{render_personnel_block, render_scout_reports};

/// Durable subject and invocation policy used to prepare a Studio Scout assignment.
#[derive(Clone, Debug)]
pub struct RatingReq {
    pub entity_type: String,
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,
    pub season: Option<i32>,
    pub trigger_type: String,
}

/// Prepare the Scout's complete assignment without calling a model.
pub async fn build_rating_request(
    pool: &sqlx::PgPool,
    models: &Models,
    req: &RatingReq,
    temperature: f64,
    with_enrichment: bool,
) -> Result<RatingBuild> {
    let Some(mut profile) = evidence::load_rating_profile(
        pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        req.season,
    )
    .await?
    else {
        return Ok(RatingBuild::NoStats {
            season: req.season.unwrap_or(0),
        });
    };

    let off_facet_stat_labels = scout::drop_off_facet_datapoints(&mut profile);
    let degenerate_zero_stat_labels = scout::drop_degenerate_zero_datapoints(&mut profile);
    let display_tier_stat_labels = scout::drop_display_tier_datapoints(&mut profile);
    if profile.composite_score.is_none() && profile.breakdown.is_empty() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
        });
    }

    let base_components = scout::input_components(&profile);
    let mut memory_request =
        MemoryRequest::new(Mission::Scout, &req.entity_type, req.entity_id, &req.sport);
    memory_request.season = Some(profile.season);
    let memories = memories::load(pool, memory_request).await?;
    let supports_cross_season = scout::supports_cross_season_comparison(&profile);
    let model_memories = if supports_cross_season {
        memories.clone()
    } else {
        memories.current_snapshot_view()?
    };
    let input_components = model_memories.with_input_components(&base_components)?;
    let (notability, notability_components) = scout::compute_notability(&profile);
    let exclusions = RatingExclusions {
        budget_truncated_stat_labels: scout::budget_truncated_stat_labels(&profile.breakdown),
        off_facet_stat_labels,
        degenerate_zero_stat_labels,
        display_tier_stat_labels,
    };
    let rating_trajectory = evidence::load_rating_trajectory(
        pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &profile,
    )
    .await?;

    let personnel = if with_enrichment && !memories.historical {
        let (changes, total) = match crate::evidence::personnel::load_personnel_changes(
            pool,
            &req.sport,
            &req.entity_type,
            req.entity_id,
        )
        .await
        {
            Ok(loaded) => loaded,
            Err(error) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    %error,
                    "rating: personnel-change load failed (continuing without the block)"
                );
                (Vec::new(), 0)
            }
        };
        let (availability, availability_total) =
            match crate::evidence::personnel::load_availability_changes(
                pool,
                &req.sport,
                &req.entity_type,
                req.entity_id,
            )
            .await
            {
                Ok(loaded) => loaded,
                Err(error) => {
                    tracing::warn!(
                        entity_type = %req.entity_type,
                        entity_id = req.entity_id,
                        sport = %req.sport,
                        %error,
                        "rating: availability load failed (continuing without those lines)"
                    );
                    (Vec::new(), 0)
                }
            };
        render_personnel_block(
            &req.entity_type,
            req.entity_id,
            &changes,
            total,
            &availability,
            availability_total,
        )
    } else {
        None
    };

    let current_reports = if with_enrichment && !memories.historical {
        match crate::evidence::personnel::load_scout_reports(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
        )
        .await
        {
            Ok(claims) => render_scout_reports(&claims),
            Err(error) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    %error,
                    "rating: current-report load failed (continuing without the block)"
                );
                None
            }
        }
    } else {
        None
    };

    let comparisons = if with_enrichment && supports_cross_season {
        match evidence::load_rating_profile(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            Some(profile.season - 1),
        )
        .await
        {
            Ok(Some(mut prior)) => {
                let _ = scout::drop_off_facet_datapoints(&mut prior);
                let _ = scout::drop_degenerate_zero_datapoints(&mut prior);
                let _ = scout::drop_display_tier_datapoints(&mut prior);
                Some(scout::build_skill_changes(&profile, &prior))
            }
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    %error,
                    "rating: prior-season profile load failed (continuing without movement lines)"
                );
                None
            }
        }
    } else {
        None
    };
    let comparison_directions = scout::comparison_directions(&profile, comparisons.as_ref());
    let prompt_profile =
        scout::model_prompt_profile(&profile, supports_cross_season, comparisons.as_ref());
    let measurement_bands = scout::measurement_bands(&prompt_profile);
    let form_trend = if with_enrichment {
        rating_trajectory.label.as_ref().map(|label| {
            format!(
                "{label}; {} scored events",
                rating_trajectory.components["sample_size"]
            )
        })
    } else {
        None
    };
    let identity = model_memories.render_for_model()?;
    let mut components: serde_json::Value = serde_json::from_str(&input_components)?;
    components["skill_changes"] = serde_json::json!(comparisons);
    components["personnel"] = serde_json::json!(personnel);
    components["current_reports"] = serde_json::json!(current_reports);
    components["recent_form"] = serde_json::json!(form_trend);
    let input_components = components.to_string();
    let input_hash = hash_components(&input_components);
    let subject = Subject {
        entity_type: req.entity_type.clone(),
        entity_name: req.entity_name.clone(),
        sport: req.sport.clone(),
    };
    let built_prompt = scout::build_stat_prompt(
        &subject,
        &prompt_profile,
        personnel.as_deref(),
        comparisons.as_ref(),
        form_trend.as_deref(),
        current_reports.as_deref(),
        Some(&identity),
    );
    let opts = GenerateOptions {
        system: Some(RATING_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: RATING_NUM_PREDICT,
        num_ctx: models.voice_num_ctx,
        json_mode: false,
        format_schema: Some(crate::studio::form::card_schema(false)),
        format_schema_raw: None,
    };

    Ok(RatingBuild::Ready(Box::new(Assignment {
        subject,
        season: profile.season,
        comparison_directions,
        measurement_bands,
        notability,
        notability_components,
        rating_trajectory,
        input_components,
        input_hash,
        exclusions,
        opts,
        built_prompt,
    })))
}

/// Prepare, debounce, and create a Scout product. Publication remains a separate short transaction.
pub async fn generate_rating(
    pool: &sqlx::PgPool,
    models: &Models,
    req: &RatingReq,
    temperature: f64,
    skip_unchanged: bool,
    with_enrichment: bool,
) -> Result<RatingOutput> {
    let backend = models.router.for_role(Role::StatsLogic);
    let assignment =
        match build_rating_request(pool, models, req, temperature, with_enrichment).await? {
            RatingBuild::NoStats { season } => return Ok(scout::no_stats(season, backend.model())),
            RatingBuild::Ready(assignment) => *assignment,
        };
    if skip_unchanged
        && evidence::last_commentary_input_hash(
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
    crate::application::queue::outbox::record_rating_completed(
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
pub struct RatingHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

impl RatingHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl WorkHandler for RatingHandler {
    fn stage(&self) -> Stage {
        Stage::Rating
    }

    fn max_in_flight(&self) -> usize {
        2
    }

    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::application::queue::stage::ARCHBOX_SLOTS)
    }

    async fn handle(&self, item: &Item) -> Result<HandleOutcome> {
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
