//! Claim-fenced crown/marker publication and diagnostic ledger.
use super::prompt::ORACLE_NUM_PREDICT;
use super::{Prepared, SigilOutput};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::plugin::PluginOutcome;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use anyhow::{Context, Result};
use sqlx::{PgPool, Postgres, Row, Transaction};

const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v4-prose";

const ORACLE_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::oracle::manifest::MANIFEST.id.as_str(),
    stage: "sigil",
    lens: "oracle",
    role: crate::plugins::oracle::manifest::ROUTE,
    product_table: "sigil_synthesis",
    output_contract_version: ORACLE_OUTPUT_CONTRACT_VERSION,
};

async fn insert_sigil(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    sport: &str,
    output: &SigilOutput,
    previous_score: Option<i16>,
) -> Result<i64> {
    let score = output.score.map(|value| value as i16);
    let convergence = output.convergence.map(|value| value as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, input_components, input_hash,
            model_version, prompt_version, convergence,
            reading, headline, omen, voiced_score,
            voiced_at, voice_model_version, voice_prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6,$7::jsonb,$8,$9,$10,$11,
                  $12,$13,$14,$15,
                  CASE WHEN $12 IS NOT NULL THEN NOW() END,
                  CASE WHEN $12 IS NOT NULL THEN $9 END,
                  CASE WHEN $12 IS NOT NULL THEN $10 END)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(output.season)
    .bind(score)
    .bind(previous_score)
    .bind(&output.input_components_json)
    .bind(output.provenance.input_hash.as_deref())
    .bind(&output.provenance.model_version)
    .bind(output.provenance.prompt_version)
    .bind(convergence)
    .bind(output.reading.as_deref())
    .bind(output.headline.as_deref())
    .bind(output.omen)
    .bind(score)
    .fetch_one(&mut **tx)
    .await
    .context("persist sigil")?;
    Ok(row.get("id"))
}

pub(super) async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    prepared: &Prepared,
) -> Result<(PluginOutcome, Option<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, None));
    };
    let product_row_id = match prepared {
        Prepared::Debounced => None,
        Prepared::Product {
            output,
            previous_score,
        } => Some(
            insert_sigil(
                publication.transaction(),
                item,
                sport,
                output,
                *previous_score,
            )
            .await?,
        ),
    };
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_id))
}

pub(super) async fn record_ledger(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    output: &SigilOutput,
    product_row_id: i64,
) {
    insert_generation_ledger_best_effort(
        pool,
        output,
        ORACLE_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32().unwrap_or_default(),
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::json!({}),
            product_row_ids: vec![product_row_id],
            included_evidence: serde_json::json!({
                "input_components": serde_json::from_str::<serde_json::Value>(
                    &output.input_components_json
                ).unwrap_or_else(|_| serde_json::json!({
                    "raw_input_components": &output.input_components_json
                })),
                "score": output.score,
                "convergence": output.convergence,
                "omen": output.omen,
            }),
            excluded_evidence: if output.was_called() {
                serde_json::json!([])
            } else {
                serde_json::json!([{
                    "reason": "no_journalist_scout_influencer_analyst_or_insider_card"
                }])
            },
            context_budget: output.context_budget(serde_json::json!({
                "num_predict": ORACLE_NUM_PREDICT,
            })),
            parser_outcome: if output.was_called() {
                "parsed"
            } else {
                "no_call"
            },
        },
    )
    .await;
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
