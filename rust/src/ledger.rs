//! Durable diagnostic ledger for cognition lens calls (Multi-Lens Phase 2).
//!
//! Product tables stay authoritative for served data. The ledger is diagnostic: exact model request,
//! evidence included/excluded, parser outcome, and the product row ids produced. Writes are
//! best-effort from production stages so a schema/deployment issue in diagnostics cannot break the
//! user-facing news rail.

use crate::harness::Generation;
use crate::route::Role;
use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{PgPool, Row};
use tracing::warn;

struct CognitionLedgerEntry {
    stage: String,
    lens: String,
    role: String,
    entity_type: String,
    entity_id: i32,
    sport: String,
    pair_entity_type: Option<String>,
    pair_entity_id: Option<i32>,
    trigger_type: String,
    trigger_payload: Value,
    product_table: String,
    product_row_ids: Vec<i64>,
    model_version: String,
    prompt_version: String,
    output_contract_version: String,
    input_ids: Vec<i64>,
    input_hash: Option<String>,
    request_body: Option<Value>,
    built_prompt: Option<String>,
    included_evidence: Value,
    excluded_evidence: Value,
    context_budget: Value,
    parser_outcome: String,
}

/// Static identity of one generated product in the cognition ledger.
#[derive(Clone, Copy, Debug)]
pub struct LedgerSpec {
    pub stage: &'static str,
    pub lens: &'static str,
    pub role: Role,
    pub product_table: &'static str,
    pub output_contract_version: &'static str,
}

/// Per-item ledger fields that are not already carried by [`Generation`].
#[derive(Debug)]
pub struct LedgerEvent<'a> {
    pub entity_type: &'a str,
    pub entity_id: i32,
    pub sport: &'a str,
    pub pair_entity: Option<(&'a str, i32)>,
    pub trigger_type: &'a str,
    pub trigger_payload: Value,
    pub product_row_ids: Vec<i64>,
    pub included_evidence: Value,
    pub excluded_evidence: Value,
    pub context_budget: Value,
    pub parser_outcome: &'a str,
}

async fn insert_cognition_ledger(pool: &PgPool, entry: &CognitionLedgerEntry) -> Result<i64> {
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

/// Write the shared generation envelope plus the item-specific ledger event.
pub async fn insert_generation_ledger_best_effort<T>(
    pool: &PgPool,
    generation: &Generation<T>,
    spec: LedgerSpec,
    event: LedgerEvent<'_>,
) {
    let (pair_entity_type, pair_entity_id) = event
        .pair_entity
        .map(|(kind, id)| (Some(kind.to_string()), Some(id)))
        .unwrap_or((None, None));
    let call = generation.call.as_ref();
    let entry = CognitionLedgerEntry {
        stage: spec.stage.to_string(),
        lens: spec.lens.to_string(),
        role: spec.role.as_str().to_string(),
        entity_type: event.entity_type.to_string(),
        entity_id: event.entity_id,
        sport: event.sport.to_string(),
        pair_entity_type,
        pair_entity_id,
        trigger_type: event.trigger_type.to_string(),
        trigger_payload: event.trigger_payload,
        product_table: spec.product_table.to_string(),
        product_row_ids: event.product_row_ids,
        model_version: generation.provenance.model_version.clone(),
        prompt_version: generation.provenance.prompt_version.to_string(),
        output_contract_version: spec.output_contract_version.to_string(),
        input_ids: generation.provenance.input_ids.clone(),
        input_hash: generation.provenance.input_hash.clone(),
        request_body: call.map(|call| call.request_body.clone()),
        built_prompt: call.map(|call| call.built_prompt.clone()),
        included_evidence: event.included_evidence,
        excluded_evidence: event.excluded_evidence,
        context_budget: event.context_budget,
        parser_outcome: event.parser_outcome.to_string(),
    };
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
