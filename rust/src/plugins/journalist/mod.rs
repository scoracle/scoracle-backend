//! Journalist execution: articulate prepared reports and publish under the queue claim.
mod activity;
pub mod fresh;
pub mod manifest;
mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::products::EntityKey;
use crate::harness::queue::work::Item;
use crate::harness::{Generation, GenerationCall, Studio};
use crate::tools::meta::EntityMeta;
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
                memory_provenance: json!({"receipt":null,"selected":[]}),
                narratives: Vec::new(),
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
            crate::harness::session::publishing_correction,
        )
        .await?;
    let call = GenerationCall::from(&output);
    let mut product = output
        .value
        .ok_or_else(|| anyhow::anyhow!("missing edition"))?;
    for narrative in &mut product.narratives {
        let evidence: Vec<_> = assignment
            .selected
            .iter()
            .filter(|r| narrative.input_news_ids.contains(&r.id))
            .cloned()
            .collect();
        let measured = activity::load(pool, &evidence, now).await?;
        narrative.impact = measured.summary.score;
        narrative.impact_components = measured.summary.components;
    }
    Ok(Generation::called(
        product,
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
        let backend = self.models.inference(manifest::ROUTE)?;
        execute_with_backend(
            &self.pool,
            backend.as_ref(),
            self.models.voice_num_ctx,
            item,
        )
        .await
    }
}

pub(crate) async fn execute_with_backend(
    pool: &sqlx::PgPool,
    backend: &dyn crate::harness::model::Inference,
    num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let now = now_unix();
    let subject = EntityMeta {
        name: crate::tools::meta::lookup_entity_name(
            pool,
            &item.entity_type,
            item.entity_id_i32()?,
            &item.sport,
        )
        .await?,
        entity_type: item.entity_type.clone(),
        entity_id: item.entity_id_i32()?,
        sport: item.sport.to_uppercase(),
    };
    let mut material = fresh::load_narratives_material(pool, subject, now).await?;
    let payload = json!({"source":"classifier"});
    let debounced = material.sources.is_empty()
        || (crate::harness::products::debounce_unchanged(
            pool,
            "news_summaries",
            &EntityKey {
                entity_type: item.entity_type.clone(),
                entity_id: item.entity_id_i32()?,
                sport: item.sport.to_uppercase(),
                season: None,
            },
            &material.assignment.input_hash,
        )
        .await?
            && !material.assignment.selected.is_empty());
    if debounced {
        return Ok(commit_claimed(
            pool,
            item,
            &item.sport.to_uppercase(),
            "classifier",
            &payload,
            &Prepared::Debounced,
            &material.sources,
            &material.assignment.dispositions,
        )
        .await?
        .0);
    }
    let output =
        loop {
            match create(
                pool,
                &Studio::new(backend),
                &material.assignment,
                now,
                num_ctx,
            )
            .await
            {
                Err(error) if error.is::<crate::harness::model::ContextExceeded>() => {
                    if material.assignment.memories.is_empty() {
                        material.assignment.dispositions.extend(
                            material.assignment.selected.drain(..).map(|report| {
                                prompt::Disposition {
                                    article_id: report.id,
                                    reason: "daily_package_exceeds_token_budget",
                                }
                            }),
                        );
                    } else {
                        material.assignment.memories.clear();
                    }
                    material.assignment.input_hash = crate::util::hash_components(&format!(
                        "{}:{}",
                        material.assignment.input_hash,
                        prompt::prompt(&material.assignment)
                    ));
                }
                result => break result?,
            }
        };
    // No fresh reporting completes dispositions without overwriting the last
    // useful edition with an artificial quiet/zero product.
    let mut dispositions = material.assignment.dispositions.clone();
    if output.was_called() && output.narratives.is_empty() {
        dispositions.extend(material.assignment.selected.iter().map(|report| {
            prompt::Disposition {
                article_id: report.id,
                reason: "no_target_reporting",
            }
        }));
    }
    let prepared = if output.narratives.is_empty() {
        Prepared::Debounced
    } else {
        Prepared::Product(&output)
    };
    let (outcome, rows) = commit_claimed(
        pool,
        item,
        &item.sport.to_uppercase(),
        "classifier",
        &payload,
        &prepared,
        &material.sources,
        &dispositions,
    )
    .await?;
    if matches!(
        outcome,
        PluginOutcome::Committed | PluginOutcome::Deferred { .. }
    ) {
        record_ledger(
            pool,
            &LedgerSubject {
                entity_type: &item.entity_type,
                entity_id: item.entity_id_i32()?,
                sport: &item.sport.to_uppercase(),
                trigger_type: "classifier",
                trigger_payload: &payload,
            },
            rows,
            &output,
        )
        .await?;
    }
    Ok(outcome)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
