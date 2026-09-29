//! Journalist evidence preparation, model selection, publication, and durable work coordination.
//!
//! The plugin owns source preparation and publication. Studio supplies inference only.

use crate::application::models::ExecutionCapabilities;
use crate::application::products::EntityKey;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::trajectory::DEFAULT_TRAJECTORY;
use crate::plugins::journalist::cognition::{
    self as journalist, Assignment, CorpusItem, NarrativesOutput,
    NARRATIVES_OUTPUT_CONTRACT_VERSION,
};
use crate::plugins::meta::EntityMeta;
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};

pub(crate) const NARRATIVES_COMPLETED: &str = "narratives_completed";

pub(crate) async fn record_narratives_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    crate::application::queue::outbox::record(
        tx,
        item,
        crate::application::queue::outbox::NewEvent {
            kind: NARRATIVES_COMPLETED,
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32()?,
            source_input_version: item.input_version.as_deref(),
        },
    )
    .await
    .context("record narratives completion outbox")
}

const NARRATIVES_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
    stage: "narratives",
    lens: "narratives",
    role: crate::plugins::journalist::manifest::ROUTE,
    product_table: "news_summaries",
    output_contract_version: NARRATIVES_OUTPUT_CONTRACT_VERSION,
};

pub struct NarrativesMaterial {
    pub assignment: Assignment,
    pub sources: Vec<crate::plugins::harvester::delivery::SourceContext>,
}

/// One source-only path for every trigger, including old queue revisions. A
/// trigger cannot select an Editor packet fallback or upgrade a source receipt.
pub async fn load_narratives_material(
    pool: &PgPool,
    subject: EntityMeta,
    now: i64,
) -> Result<NarrativesMaterial> {
    let sources = crate::plugins::harvester::delivery::load_for_character(
        pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    let fresh = sources.iter().map(CorpusItem::from).collect::<Vec<_>>();
    let memory = super::memories::load_for_assignment(pool, &subject, &fresh, now).await?;
    let assignment = journalist::prepare(subject, fresh, &memory, now)?;
    let sources = sources
        .into_iter()
        .filter(|s| !assignment.deferred_ids.contains(&s.article_id))
        .collect();
    Ok(NarrativesMaterial {
        assignment,
        sources,
    })
}

async fn insert_narratives(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    output: &NarrativesOutput,
) -> Result<Vec<i64>> {
    const INSERT: &str = r#"
        INSERT INTO news_summaries (
            entity_type, entity_id, sport, trigger_type, trigger_payload,
            narrative_title, body, impact, impact_components,
            input_news_ids,
            narrative_updated_at, source_count, source_names, source_latest_at, source_oldest_at,
            trajectory, trajectory_components,
            model_version, prompt_version, input_hash, storyline_id,
            card_score, headline, generated_at
        ) VALUES (
            $1,$2,$3,$4,$5::jsonb, $6,$7,$8,$9::jsonb, $10,
            COALESCE(to_timestamp($11::double precision), NOW()), $12, $13,
            to_timestamp($14::double precision), to_timestamp($15::double precision),
            $16, $17::jsonb,
            $18,$19,$20,$21,
            $22,$23,NOW()
        )
        RETURNING id"#;

    anyhow::ensure!(
        !output.narratives.is_empty(),
        "empty editions do not publish a product"
    );
    let provenance = &output.provenance;
    let trigger_json = trigger_payload.to_string();
    let mut product_row_ids = Vec::with_capacity(output.narratives.len());
    for narrative in &output.narratives {
        let inserted = sqlx::query(INSERT)
            .bind(entity_type)
            .bind(entity_id)
            .bind(sport)
            .bind(trigger_type)
            .bind(&trigger_json)
            .bind(Some(narrative.title.as_str()))
            .bind(Some(narrative.body.as_str()))
            .bind(Some(narrative.impact as i16))
            .bind(narrative.impact_components.to_string())
            .bind(&narrative.input_news_ids)
            .bind(narrative.source_latest_epoch)
            .bind(narrative.source_count)
            .bind(&narrative.source_names)
            .bind(narrative.source_latest_epoch)
            .bind(narrative.source_oldest_epoch)
            .bind(DEFAULT_TRAJECTORY)
            .bind(json!({"reason":"independent_source_report"}).to_string())
            .bind(provenance.model_version.as_str())
            .bind(provenance.prompt_version)
            .bind(provenance.input_hash.as_deref())
            .bind(None::<i64>)
            .bind(output.card_score)
            .bind(output.headline.as_deref())
            .fetch_one(&mut **tx)
            .await
            .context("persist source report")?;
        product_row_ids.push(inserted.get("id"));
    }
    Ok(product_row_ids)
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
    product_row_ids: Vec<i64>,
    output: &NarrativesOutput,
) -> Result<()> {
    let narratives = output
        .narratives
        .iter()
        .map(|narrative| {
            json!({
                "title": &narrative.title,
                "input_news_ids": &narrative.input_news_ids,
                "source_count": narrative.source_count,
                "source_names": &narrative.source_names,
                "impact": narrative.impact,
            })
        })
        .collect::<Vec<_>>();
    let included_evidence = json!({
        "input_news_ids": &output.provenance.input_ids,
        "narratives": narratives,
        "memory": output.memory_provenance,
    });
    let num_ctx = output
        .request_body()
        .and_then(|body| body.pointer("/options/num_ctx"))
        .and_then(|value| value.as_i64())
        .unwrap_or(crate::studio::model::VOICE_NUM_CTX_PACKET as i64) as i32;
    let mut excluded = Vec::new();
    if !output.budget_truncated_ids.is_empty() {
        excluded.push(json!({"reason":"deferred_context_budget",
            "dropped_news_ids": output.budget_truncated_ids}));
    }
    insert_generation_ledger_best_effort(
        pool,
        output,
        NARRATIVES_LEDGER,
        LedgerEvent {
            entity_type: subject.entity_type,
            entity_id: subject.entity_id,
            sport: subject.sport,
            pair_entity: None,
            trigger_type: subject.trigger_type,
            trigger_payload: subject.trigger_payload.clone(),
            product_row_ids,
            included_evidence,
            excluded_evidence: json!(excluded),
            context_budget: output.context_budget(json!({
                "num_predict": output.request_body()
                    .and_then(|body| body.pointer("/options/num_predict"))
                    .and_then(|value| value.as_i64())
                    .unwrap_or(journalist::NUM_PREDICT as i64),
                "num_ctx": num_ctx,
            })),
            parser_outcome: if !output.was_called() {
                "no_call"
            } else if output.narratives.is_empty() {
                "parsed_empty"
            } else {
                "parsed"
            },
        },
    )
    .await;
    Ok(())
}

enum Prepared<'a> {
    Debounced,
    Product(&'a NarrativesOutput),
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    prepared: &Prepared<'_>,
    harvester_sources: &[crate::plugins::harvester::delivery::SourceContext],
    dispositions: &[journalist::Disposition],
) -> Result<(PluginOutcome, Vec<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, Vec::new()));
    };
    crate::plugins::harvester::delivery::validate_for_publication(
        publication.transaction(),
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        &item.entity_type,
        item.entity_id_i32()?,
        sport,
        harvester_sources,
    )
    .await?;
    let product_row_ids = match prepared {
        Prepared::Debounced => Vec::new(),
        Prepared::Product(output) => {
            insert_narratives(
                publication.transaction(),
                &item.entity_type,
                item.entity_id_i32()?,
                sport,
                trigger_type,
                trigger_payload,
                output,
            )
            .await?
        }
    };
    if !harvester_sources.is_empty() {
        let cited: std::collections::HashSet<i64> = match prepared {
            Prepared::Debounced => std::collections::HashSet::new(),
            Prepared::Product(output) => output
                .narratives
                .iter()
                .flat_map(|narrative| narrative.input_news_ids.iter().copied())
                .collect(),
        };
        for source in harvester_sources {
            let reason = dispositions
                .iter()
                .find(|d| d.article_id == source.article_id)
                .map(|d| d.reason);
            let status = if reason == Some("already_reported_exact_text")
                || (reason.is_none() && matches!(prepared, Prepared::Debounced))
            {
                "redundant"
            } else if cited.contains(&source.article_id) {
                "used"
            } else {
                "abstained"
            };
            let row_refs = match prepared {
                Prepared::Product(output) => output
                    .narratives
                    .iter()
                    .zip(&product_row_ids)
                    .filter(|(n, _)| n.input_news_ids.contains(&source.article_id))
                    .map(|(_, id)| *id)
                    .collect::<Vec<_>>(),
                Prepared::Debounced => Vec::new(),
            };
            let changed = sqlx::query(
                "UPDATE public.harvester_assignments SET status=$3, reason=$4, \
                 product_ref=$5, updated_at=NOW() \
                 WHERE classification_id=$1 AND plugin_id=$2 AND status='pending' \
                   AND reason IS DISTINCT FROM 'delivery_held'",
            )
            .bind(source.classification_id)
            .bind(crate::plugins::journalist::manifest::MANIFEST.id.as_str())
            .bind(status)
            .bind(reason)
            .bind(serde_json::json!({"news_summary_ids": row_refs}))
            .execute(&mut **publication.transaction())
            .await?;
            anyhow::ensure!(
                changed.rows_affected() == 1,
                "Journalist source assignment changed during call"
            );
        }
    }
    if !harvester_sources.is_empty() {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.harvester_assignments d \
             JOIN public.harvester_classifications c ON c.id=d.classification_id \
             WHERE d.plugin_id=$1 AND d.status='pending' AND d.reason IS DISTINCT FROM $5 \
               AND c.entity_type=$2 \
               AND c.entity_id=$3 AND c.sport=$4",
        )
        .bind(crate::plugins::journalist::manifest::MANIFEST.id.as_str())
        .bind(&item.entity_type)
        .bind(item.entity_id_i32()?)
        .bind(sport)
        .bind(crate::plugins::harvester::adapter::DELIVERY_HELD_REASON)
        .fetch_one(&mut **publication.transaction())
        .await?;
        if remaining > 0 {
            publication.commit_progress().await?;
            return Ok((
                PluginOutcome::deferred(
                    format!("{remaining} Harvester sources remain for Journalist"),
                    std::time::Duration::from_secs(1),
                ),
                product_row_ids,
            ));
        }
    }
    record_narratives_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_ids))
}

pub struct NarrativesHandler {
    pool: sqlx::PgPool,
    models: ExecutionCapabilities,
}

impl NarrativesHandler {
    pub fn new(pool: sqlx::PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for NarrativesHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::journalist::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let now = now_unix();
        let subject = EntityMeta {
            name: crate::evidence::corpus::lookup_entity_name(
                &self.pool,
                &item.entity_type,
                item.entity_id_i32()?,
                &item.sport,
            )
            .await?,
            entity_type: item.entity_type.clone(),
            entity_id: item.entity_id_i32()?,
            sport: item.sport.to_uppercase(),
        };
        let material = load_narratives_material(&self.pool, subject, now).await?;
        let payload = json!({"source":"harvester"});
        if material.sources.is_empty() {
            return Ok(commit_claimed(
                &self.pool,
                item,
                &item.sport.to_uppercase(),
                "periodic",
                &payload,
                &Prepared::Debounced,
                &[],
                &[],
            )
            .await?
            .0);
        }
        let unchanged = crate::application::products::debounce_unchanged(
            &self.pool,
            "news_summaries",
            &EntityKey {
                entity_type: item.entity_type.clone(),
                entity_id: item.entity_id_i32()?,
                sport: item.sport.to_uppercase(),
                season: None,
            },
            &material.assignment.input_hash,
        )
        .await?;
        if unchanged && !material.assignment.selected.is_empty() {
            return Ok(commit_claimed(
                &self.pool,
                item,
                &item.sport.to_uppercase(),
                "periodic",
                &payload,
                &Prepared::Debounced,
                &material.sources,
                &material.assignment.dispositions,
            )
            .await?
            .0);
        }
        let backend = self
            .models
            .inference(crate::plugins::journalist::manifest::ROUTE)?;
        let output = journalist::create(
            &Studio::new(backend.as_ref()),
            &material.assignment,
            now,
            self.models.voice_num_ctx,
        )
        .await?;
        // No fresh reporting completes dispositions without overwriting the last
        // useful edition with an artificial quiet/zero product.
        let prepared = if output.narratives.is_empty() {
            Prepared::Debounced
        } else {
            Prepared::Product(&output)
        };
        let (outcome, rows) = commit_claimed(
            &self.pool,
            item,
            &item.sport.to_uppercase(),
            "periodic",
            &payload,
            &prepared,
            &material.sources,
            &material.assignment.dispositions,
        )
        .await?;
        if matches!(
            outcome,
            PluginOutcome::Committed | PluginOutcome::Deferred { .. }
        ) {
            record_ledger(
                &self.pool,
                &LedgerSubject {
                    entity_type: &item.entity_type,
                    entity_id: item.entity_id_i32()?,
                    sport: &item.sport.to_uppercase(),
                    trigger_type: "periodic",
                    trigger_payload: &payload,
                },
                rows,
                &output,
            )
            .await?;
        }
        Ok(outcome)
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
