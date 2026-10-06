//! Scout product SQL, provenance receipts and claim-fenced publication.
use super::parser::RATING_OUTPUT_CONTRACT_VERSION;
use super::performance::MAX_STAT_FACTS;
use super::prompt::RATING_NUM_PREDICT;
use super::{record_rating_completed, RatingOutput};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use anyhow::{Context, Result};
use sqlx::{PgPool, Postgres, Row, Transaction};

const RATING_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::scout::manifest::MANIFEST.id.as_str(),
    stage: "rating",
    lens: "rating",
    role: crate::plugins::scout::manifest::ROUTE,
    product_table: "stat_summaries",
    output_contract_version: RATING_OUTPUT_CONTRACT_VERSION,
};

pub(super) async fn insert_stat_summary(
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

pub(super) struct LedgerSubject<'a> {
    pub(super) entity_type: &'a str,
    pub(super) entity_id: i32,
    pub(super) sport: &'a str,
    pub(super) trigger_type: &'a str,
    pub(super) trigger_payload: &'a serde_json::Value,
}

pub(super) async fn record_ledger(
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
pub(super) async fn persist_stat_summary(
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

pub(super) enum Prepared<'a> {
    Debounced,
    Product(&'a RatingOutput),
}

pub(super) fn prepare(out: &RatingOutput) -> Prepared<'_> {
    if out.skipped_unchanged {
        Prepared::Debounced
    } else {
        Prepared::Product(out)
    }
}

pub(super) async fn commit_claimed(
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

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
