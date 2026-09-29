//! One verified publisher source per product, with atomic source disposition.
use super::{persist_to_vibe_scores, record_ledger, record_vibe_completed};
use crate::application::models::ExecutionCapabilities;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::Item;
use crate::evidence::corpus::lookup_entity_name;
use crate::plugins::harvester::delivery::{
    load_for_character, validate_for_publication, SourceContext,
};
use crate::plugins::influencer::cognition::{Assignment, VibeOutput, VIBE_PROMPT_VERSION};
use crate::plugins::influencer::{cognition, memories};
use crate::plugins::meta::EntityMeta;
use crate::studio::model::Inference;
use crate::studio::plugin::PluginOutcome;
use crate::studio::Studio;
use crate::util::hash_components;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Read-only preparation shared by the worker and evaluation. No generative decisions.
pub async fn prepare_assignment(
    pool: &PgPool,
    subject: EntityMeta,
    source: &SourceContext,
    now: i64,
) -> Result<(Option<Assignment>, Value)> {
    let reason = match source.published_at_epoch {
        Some(published) => cognition::source_disposition(&source.context, published, now),
        None => cognition::source_disposition(&source.context, now, now),
    };
    if let Some(reason) = reason {
        return Ok((
            None,
            json!({"contract":VIBE_PROMPT_VERSION,"source":source,"reason":reason}),
        ));
    }
    let study = memories::load(pool, &subject, source).await?;
    let history = study
        .as_ref()
        .map(|s| memories::select(s, &subject, source))
        .transpose()?
        .unwrap_or_default();
    let input_components_json = json!({"subject":subject,"source":source,
        "history":history,"memory_study":study,"prompt_version":VIBE_PROMPT_VERSION})
    .to_string();
    let input_hash = hash_components(&input_components_json);
    Ok((
        Some(Assignment {
            subject,
            source: source.clone(),
            history,
            input_components_json,
            input_hash,
        }),
        json!({"contract":VIBE_PROMPT_VERSION}),
    ))
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
    let (assignment, mut provenance) = prepare_assignment(pool, subject, source, now()).await?;
    let output = if let Some(assignment) = assignment {
        let (output, receipt) =
            cognition::create(&Studio::new(backend), &assignment, voice_num_ctx).await?;
        provenance["articulation"] = receipt;
        output
    } else {
        None
    };
    Ok(Evaluation { output, provenance })
}
pub(super) async fn execute(
    pool: &PgPool,
    models: &ExecutionCapabilities,
    item: &Item,
) -> Result<PluginOutcome> {
    let backend = models.inference(crate::plugins::influencer::manifest::ROUTE)?;
    execute_with_backend(pool, backend.as_ref(), models.voice_num_ctx, item).await
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
