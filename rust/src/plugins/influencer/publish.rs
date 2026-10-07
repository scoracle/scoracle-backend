//! Influencer source publication and completion coordination.
use super::{prompt::VIBE_NUM_PREDICT, VibeOutput};
use crate::harness::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::harness::queue::work::Item;
use anyhow::{Context, Result};
use sqlx::{Postgres, Row, Transaction};

pub(crate) const VIBE_COMPLETED: &str = "vibe_completed";
pub(crate) async fn record_vibe_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    crate::harness::queue::outbox::record(
        tx,
        item,
        crate::harness::queue::outbox::NewEvent {
            kind: VIBE_COMPLETED,
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            source_input_version: item.input_version.as_deref(),
        },
    )
    .await
    .context("record vibe completion outbox")
}
pub const VIBE_OUTPUT_CONTRACT_VERSION: &str = "vibe-reader-card-v4-scored-period";
const VIBE_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::influencer::manifest::MANIFEST.id.as_str(),
    stage: "vibe",
    lens: "vibe",
    role: crate::plugins::influencer::manifest::ROUTE,
    product_table: "vibe_scores",
    output_contract_version: VIBE_OUTPUT_CONTRACT_VERSION,
};
pub(super) async fn persist_to_vibe_scores(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    sport: &str,
    out: &VibeOutput,
) -> Result<i64> {
    // Route the moat fields through the shared Provenance envelope — input_hash included
    // since F2 (mig 147); the typed INSERT stays the stage's own (Postgres-as-serializer).
    let entity_id = item.entity_id_i32()?;
    let prov = &out.provenance;
    let parts: super::prompt::Parts = serde_json::from_str(&out.input_components_json)?;
    let sentiment: Option<i16> = out.sentiment.map(|n| n as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO vibe_scores (
            entity_type, entity_id, sport,
            trigger_type, trigger_payload,
            sentiment, prompt, hook, input_news_ids,
            model_version, prompt_version, input_hash, week_season, week_no,
            reporting_start, reporting_end, evidence_cutoff, scoring_version, source_references
        ) VALUES ($1,$2,$3,'periodic','null'::jsonb,$4,$5,$6,$7,$8,$9,$10,$11,$12,
            to_timestamp($13::double precision),to_timestamp($14::double precision),
            to_timestamp($15::double precision),$16,
            (SELECT COALESCE(jsonb_agg(jsonb_build_object('id',a.id,'publisher',a.source,'url',a.url,
                'published_at',a.published_at) ORDER BY a.id),'[]'::jsonb)
             FROM news_articles a WHERE a.id=ANY($7)))
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(sentiment)
    .bind(out.vibe_prompt.as_deref())
    .bind(out.hook.as_deref())
    .bind(prov.input_ids.as_slice())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(prov.input_hash.as_deref())
    .bind(parts.period.season)
    .bind(parts.period.week)
    .bind(parts.period.start)
    .bind(parts.period.end)
    .bind(parts.period.cutoff)
    .bind(super::prompt::SCORE_VERSION)
    .fetch_one(&mut **tx)
    .await
    .context("persist vibe")?;
    Ok(row.get("id"))
}

pub(super) async fn record_ledger(
    pool: &sqlx::PgPool,
    item: &Item,
    sport: &str,
    product_row_id: i64,
    out: &VibeOutput,
) -> Result<()> {
    insert_generation_ledger_best_effort(
        pool,
        out,
        VIBE_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::Value::Null,
            product_row_ids: vec![product_row_id],
            included_evidence: serde_json::json!({
                "input_components": serde_json::from_str::<serde_json::Value>(
                    &out.input_components_json
                ).unwrap_or_else(|_| serde_json::json!({
                    "raw_input_components": out.input_components_json
                })),
                "sentiment": out.sentiment,
                "vibe_prompt": &out.vibe_prompt,
                "hook": &out.hook,
            }),
            excluded_evidence: serde_json::json!([]),
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": VIBE_NUM_PREDICT,
            })),
            parser_outcome: "parsed",
        },
    )
    .await;
    Ok(())
}
