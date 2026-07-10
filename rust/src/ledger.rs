//! Durable diagnostic ledger for cognition lens calls (Multi-Lens Phase 2).
//!
//! Product tables stay authoritative for served data. The ledger is diagnostic: exact model request,
//! evidence included/excluded, parser outcome, and the product row ids produced. Writes are
//! best-effort from production stages so a schema/deployment issue in diagnostics cannot break the
//! user-facing news rail.

use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{PgPool, Row};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct CognitionLedgerEntry {
    pub stage: String,
    pub lens: String,
    pub role: String,
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    pub pair_entity_type: Option<String>,
    pub pair_entity_id: Option<i32>,
    pub trigger_type: String,
    pub trigger_payload: Value,
    pub product_table: String,
    pub product_row_ids: Vec<i64>,
    pub model_version: String,
    pub prompt_version: String,
    pub output_contract_version: String,
    pub input_ids: Vec<i64>,
    pub input_hash: Option<String>,
    pub request_body: Option<Value>,
    pub built_prompt: Option<String>,
    pub included_evidence: Value,
    pub excluded_evidence: Value,
    pub context_budget: Value,
    pub parser_outcome: String,
}

pub async fn insert_cognition_ledger(pool: &PgPool, entry: &CognitionLedgerEntry) -> Result<i64> {
    let row = sqlx::query(
        r#"
        INSERT INTO public.cognition_ledger (
            stage, lens, role, entity_type, entity_id, sport,
            pair_entity_type, pair_entity_id,
            trigger_type, trigger_payload,
            product_table, product_row_ids,
            model_version, prompt_version, output_contract_version,
            input_ids, input_hash, request_body, built_prompt,
            included_evidence, excluded_evidence, context_budget, parser_outcome
        ) VALUES (
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11,$12,$13,$14,$15,$16,$17,$18::jsonb,$19,
            $20::jsonb,$21::jsonb,$22::jsonb,$23
        )
        RETURNING id
        "#,
    )
    .bind(&entry.stage)
    .bind(&entry.lens)
    .bind(&entry.role)
    .bind(&entry.entity_type)
    .bind(entry.entity_id)
    .bind(&entry.sport)
    .bind(entry.pair_entity_type.as_deref())
    .bind(entry.pair_entity_id)
    .bind(&entry.trigger_type)
    .bind(&entry.trigger_payload)
    .bind(&entry.product_table)
    .bind(entry.product_row_ids.as_slice())
    .bind(&entry.model_version)
    .bind(&entry.prompt_version)
    .bind(&entry.output_contract_version)
    .bind(entry.input_ids.as_slice())
    .bind(entry.input_hash.as_deref())
    .bind(entry.request_body.as_ref())
    .bind(entry.built_prompt.as_deref())
    .bind(&entry.included_evidence)
    .bind(&entry.excluded_evidence)
    .bind(&entry.context_budget)
    .bind(&entry.parser_outcome)
    .fetch_one(pool)
    .await
    .context("insert cognition ledger")?;
    Ok(row.get("id"))
}

pub async fn insert_cognition_ledger_best_effort(pool: &PgPool, entry: CognitionLedgerEntry) {
    if let Err(e) = insert_cognition_ledger(pool, &entry).await {
        warn!(
            stage = %entry.stage,
            lens = %entry.lens,
            entity_type = %entry.entity_type,
            entity_id = entry.entity_id,
            sport = %entry.sport,
            error = %e,
            "cognition ledger write failed"
        );
    }
}
