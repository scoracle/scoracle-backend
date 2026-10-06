//! Journalist execution: articulate prepared reports and publish under the queue claim.
mod activity;
pub mod manifest;
pub mod memories;
mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::application::models::ExecutionCapabilities;
use crate::application::products::EntityKey;
use crate::application::queue::work::Item;
use crate::plugins::meta::EntityMeta;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::Result;
use async_trait::async_trait;
use parser::EditionParser;
use prompt::{Assignment, NARRATIVES_PROMPT_VERSION};
pub(crate) use publish::NARRATIVES_COMPLETED;
use publish::{commit_claimed, record_ledger, LedgerSubject, Prepared};
use serde_json::json;

/// One grounded storyline with deterministic impact and evidence provenance.
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub impact_components: serde_json::Value,
    pub input_news_ids: Vec<i64>,
    pub source_count: i32,
    pub source_names: Vec<String>,
    pub source_latest_epoch: Option<i64>,
    pub source_oldest_epoch: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NarrativesProduct {
    pub memory_provenance: serde_json::Value,
    pub narratives: Vec<Narrative>,
    pub budget_truncated_ids: Vec<i64>,
    pub card_score: Option<i16>,
    pub headline: Option<String>,
}
pub type NarrativesOutput = Generation<NarrativesProduct>;

pub async fn create(
    pool: &sqlx::PgPool,
    studio: &Studio<'_>,
    assignment: &Assignment,
    now: i64,
    num_ctx: i32,
) -> Result<NarrativesOutput> {
    if assignment.selected.is_empty() {
        return Ok(Generation::uncalled(
            NarrativesProduct {
                memory_provenance: json!({"receipt":assignment.memory_receipt,"selected":assignment.memories}),
                narratives: Vec::new(),
                budget_truncated_ids: assignment.deferred_ids.clone(),
                card_score: None,
                headline: None,
            },
            studio.model_name().to_string(),
            NARRATIVES_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash.clone()),
        ));
    }
    let activity = activity::load(pool, &assignment.selected, now).await?;
    let output = studio
        .extract(
            &prompt::prompt(assignment),
            &prompt::generation_options(assignment, num_ctx),
            &EditionParser {
                assignment,
                activity: &activity,
            },
            |_| None,
        )
        .await?;
    let call = GenerationCall::from(&output);
    Ok(Generation::called(
        output
            .value
            .ok_or_else(|| anyhow::anyhow!("missing edition"))?,
        output.model,
        NARRATIVES_PROMPT_VERSION,
        assignment.selected.iter().map(|s| s.id).collect(),
        Some(assignment.input_hash.clone()),
        call,
    ))
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
        let material = prompt::load_narratives_material(&self.pool, subject, now).await?;
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
        let output = create(
            &self.pool,
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
