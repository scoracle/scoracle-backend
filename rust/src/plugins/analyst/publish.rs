//! Claim-fenced product persistence, completion and diagnostic ledger.
use super::prompt::{MomentumContext, MOMENTUM_NUM_PREDICT};
use super::{record_momentum_completed, MomentumOutput, MOMENTUM_STEADY_BAND};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use anyhow::{Context, Result};
use sqlx::{PgPool, Postgres, Row, Transaction};

const MOMENTUM_OUTPUT_CONTRACT_VERSION: &str = "momentum-summary-v3-prose";

const MOMENTUM_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::analyst::manifest::MANIFEST.id.as_str(),
    stage: "momentum",
    lens: "momentum",
    role: crate::plugins::analyst::manifest::ROUTE,
    product_table: "momentum_summaries",
    output_contract_version: MOMENTUM_OUTPUT_CONTRACT_VERSION,
};

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

pub(super) async fn record_ledger(
    pool: &sqlx::PgPool,
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
                "num_predict": MOMENTUM_NUM_PREDICT,
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

pub(super) async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    output: Option<&MomentumOutput>,
) -> Result<(PluginOutcome, Option<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, None));
    };

    let product_row_id = match output {
        None => None,
        Some(output) => {
            Some(persist_momentum_summary(publication.transaction(), item, sport, output).await?)
        }
    };
    record_momentum_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_id))
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
