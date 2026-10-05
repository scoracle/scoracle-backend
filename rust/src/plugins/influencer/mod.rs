//! One verified publisher source per product, with atomic source disposition.
pub mod manifest;
pub mod memories;
pub mod prompt;
mod publish;
pub mod voice;

use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::corpus::lookup_entity_name;
use crate::plugins::harvester::delivery::{
    load_for_character, validate_for_publication, SourceContext,
};
use crate::plugins::meta::EntityMeta;
pub use crate::plugins::support::form::ObservationParser as VibeParser;
use crate::studio::model::Inference;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::{ensure, Result};
use async_trait::async_trait;
use prompt::{Assignment, VIBE_NUM_PREDICT, VIBE_PROMPT_VERSION, VIBE_TEMPERATURE};
pub(crate) use publish::VIBE_COMPLETED;
use publish::{persist_to_vibe_scores, record_ledger, record_vibe_completed};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct VibeScore {
    pub sentiment: Option<i32>,
    pub vibe_prompt: Option<String>,
    pub hook: Option<String>,
    pub input_components_json: String,
}
pub type VibeOutput = Generation<VibeScore>;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub struct VibeHandler {
    pool: PgPool,
    models: ExecutionCapabilities,
}
impl VibeHandler {
    pub fn new(pool: PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}
#[async_trait]
impl StudioPlugin for VibeHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::influencer::manifest::MANIFEST
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

pub async fn create(
    studio: &Studio<'_>,
    assignment: &Assignment,
    num_ctx: i32,
) -> Result<(Option<VibeOutput>, Value)> {
    let extracted = studio
        .extract(
            &prompt::assembled_prompt(assignment),
            &prompt::generation_options(VIBE_TEMPERATURE, num_ctx, VIBE_NUM_PREDICT),
            &VibeParser,
            |_| None,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let receipt = json!({"model_version":extracted.model,"prompt_version":VIBE_PROMPT_VERSION,
        "input_hash":assignment.input_hash,"raw_response":extracted.raw_response,
        "eval_count":extracted.eval_count,"wall_ms":extracted.wall_ms,
        "input_components":serde_json::from_str::<Value>(&assignment.input_components_json)?,
        "request_body":extracted.request_body});
    let Some(reply) = extracted.value else {
        return Ok((None, receipt));
    };
    Ok((
        Some(Generation::called(
            VibeScore {
                sentiment: None,
                hook: Some(title(&assignment.source, &assignment.subject.name)),
                vibe_prompt: reply.body,
                input_components_json: assignment.input_components_json.clone(),
            },
            extracted.model,
            VIBE_PROMPT_VERSION,
            vec![assignment.source.article_id],
            Some(assignment.input_hash.clone()),
            call,
        )),
        receipt,
    ))
}

/// Source-owned title; articulation cannot invent a headline claim.
fn title(source: &SourceContext, entity_name: &str) -> String {
    let headline = source.headline.trim();
    if !headline.is_empty()
        && headline.chars().count() <= crate::plugins::support::form::HOOK_MAX_CHARS
    {
        headline.to_string()
    } else {
        entity_name.to_string()
    }
}

struct Evaluation {
    output: Option<VibeOutput>,
    provenance: Value,
}
async fn articulate_one(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    source: &SourceContext,
) -> Result<Evaluation> {
    let subject = EntityMeta {
        name: lookup_entity_name(pool, &item.entity_type, item.entity_id_i32()?, &item.sport)
            .await?,
        entity_type: item.entity_type.clone(),
        entity_id: item.entity_id_i32()?,
        sport: item.sport.to_uppercase(),
    };
    let (assignment, mut provenance) =
        prompt::prepare_assignment(pool, subject, source, now()).await?;
    let output = if let Some(assignment) = assignment {
        let (output, receipt) = create(&Studio::new(backend), &assignment, voice_num_ctx).await?;
        provenance["articulation"] = receipt;
        output
    } else {
        None
    };
    Ok(Evaluation { output, provenance })
}
pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let entity_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let plugin_id = crate::plugins::influencer::manifest::MANIFEST.id.as_str();
    let sources = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    // Process oldest first so the latest published reading reflects the freshest
    // source after a multi-source backlog drains.
    let Some(source) = sources.last() else {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        record_vibe_completed(publication.transaction(), item).await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    };
    let evaluation = articulate_one(pool, backend, voice_num_ctx, item, source).await?;
    let output = evaluation.output.as_ref();
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    validate_for_publication(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
        std::slice::from_ref(source),
    )
    .await?;
    let product_row_id = if let Some(output) = output {
        Some(persist_to_vibe_scores(publication.transaction(), item, &sport, output).await?)
    } else {
        None
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_assignments SET status=$3, reason=$4, product_ref=$5, updated_at=NOW() \
         WHERE classification_id=$1 AND plugin_id=$2 AND status='pending' \
           AND reason IS DISTINCT FROM 'delivery_held'",
    )
    .bind(source.classification_id)
    .bind(plugin_id)
    .bind(if output.is_some() { "used" } else { "abstained" })
    .bind(if output.is_some() { None } else { Some("Influencer passed on this source") })
    .bind({
        let mut provenance = evaluation.provenance;
        if let Some(id) = product_row_id {
            provenance["vibe_score_id"] = json!(id);
        }
        provenance
    })
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "Influencer source assignment changed during call"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_assignments d \
         JOIN public.harvester_classifications c ON c.id=d.classification_id \
         WHERE d.plugin_id=$1 AND d.status='pending' AND d.reason IS DISTINCT FROM $5 \
           AND c.entity_type=$2 AND c.entity_id=$3 AND c.sport=$4",
    )
    .bind(plugin_id)
    .bind(&item.entity_type)
    .bind(entity_id)
    .bind(&sport)
    .bind(crate::plugins::harvester::adapter::DELIVERY_HELD_REASON)
    .fetch_one(&mut **publication.transaction())
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        if let (Some(id), Some(output)) = (product_row_id, output) {
            record_ledger(pool, item, &sport, id, output).await?;
        }
        return Ok(PluginOutcome::deferred(
            format!("{remaining} Harvester source contexts remain for Influencer"),
            Duration::from_secs(1),
        ));
    }
    record_vibe_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    if let (Some(id), Some(output)) = (product_row_id, output) {
        record_ledger(pool, item, &sport, id, output).await?;
    }
    Ok(PluginOutcome::Committed)
}

#[cfg(test)]
mod tests;
