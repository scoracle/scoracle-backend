//! Verified fresh reporting and attributed history, with atomic publication.
pub mod manifest;
pub mod memories;
mod parser;
pub mod prompt;
mod publish;

use crate::harness::model::Inference;
use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::{Generation, GenerationCall};
use crate::plugins::harvester::delivery::{
    load_for_character, validate_for_publication, SourceContext,
};
use crate::tools::meta::lookup_entity_name;
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Result};
use async_trait::async_trait;
pub use parser::VibeParser;
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
    pub parts: prompt::Parts,
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

/// One model call. Retain malformed responses too; publication decides success.
pub async fn create(
    backend: &dyn Inference,
    assignment: &Assignment,
    num_ctx: i32,
) -> Result<(Option<VibeOutput>, Value)> {
    use crate::harness::Parser;
    let world = assignment.parts.assemble();
    anyhow::ensure!(
        world.len() <= prompt::SOURCE_BUDGET_BYTES + prompt::HISTORY_BUDGET_BYTES + 2400,
        "Influencer packet exceeds reading budget"
    );
    let input_hash = prompt::request_hash(backend, assignment, num_ctx)?;
    let options = prompt::generation_options(VIBE_TEMPERATURE, num_ctx, VIBE_NUM_PREDICT);
    let mut receipt = json!({"model_version":backend.model(),"prompt_version":VIBE_PROMPT_VERSION,
        "scoring_version":prompt::SCORE_VERSION,"input_hash":input_hash,
        "packet":world,"input_components":assignment.parts,
        "request_body":backend.request_body(&world,&options)});
    if assignment.parts.sources.is_empty() {
        receipt["reason"] = json!("no_fresh_evidence");
        return Ok((None, receipt));
    }
    let (generated, request) = match backend.generate(&world, &options).await {
        Ok(result) => result,
        Err(error) => {
            if let Some(failure) = error.downcast_ref::<crate::harness::model::ResponseFailure>() {
                receipt["raw_response_body"] = json!(failure.raw_response_body);
            }
            receipt["error"] = json!(format!("{error:#}"));
            return Ok((None, receipt));
        }
    };
    receipt["request_body"] = request.clone();
    receipt["raw_response"] = json!(generated.response);
    receipt["raw_response_body"] = json!(generated.raw_response_body);
    receipt["eval_count"] = json!(generated.eval_count);
    receipt["prompt_eval_count"] = json!(generated.prompt_eval_count);
    receipt["wall_ms"] = json!(generated.total_duration.as_millis() as u64);
    let reply = match VibeParser.parse(&generated.response) {
        Ok(reply) => reply,
        Err(error) => {
            receipt["error"] = json!(format!("{error:#}"));
            return Ok((None, receipt));
        }
    };
    let Some(reply) = reply else {
        return Ok((None, receipt));
    };
    let mut source_ids: Vec<i64> = assignment
        .parts
        .sources
        .iter()
        .chain(&assignment.parts.history)
        .map(|s| s.article_id)
        .collect();
    source_ids.sort_unstable();
    source_ids.dedup();
    Ok((
        Some(Generation::called(
            VibeScore {
                sentiment: Some(reply.score),
                hook: Some(reply.headline),
                vibe_prompt: Some(reply.body),
                parts: assignment.parts.clone(),
            },
            generated.model,
            VIBE_PROMPT_VERSION,
            source_ids,
            Some(input_hash),
            GenerationCall {
                built_prompt: world,
                request_body: request,
                eval_count: Some(generated.eval_count),
                wall_ms: Some(generated.total_duration.as_millis() as u64),
            },
        )),
        receipt,
    ))
}

pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let entity_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let plugin_id = manifest::MANIFEST.id.as_str();
    let pending = load_for_character(pool, plugin_id, &item.entity_type, entity_id, &sport).await?;
    let Some(anchor) = pending.last() else {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        record_vibe_completed(publication.transaction(), item).await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    };
    let subject = EntityMeta {
        name: lookup_entity_name(pool, &item.entity_type, entity_id, &sport).await?,
        entity_type: item.entity_type.clone(),
        entity_id,
        sport: sport.clone(),
    };
    let (assignment, disposition) =
        prompt::prepare_assignment(pool, subject, anchor, now()).await?;
    let input_hash = prompt::request_hash(backend, &assignment, voice_num_ctx)?;
    // Reuse a completed attempt only for identical evidence, model and contract.
    let previous: Option<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT id,product_id FROM vibe_card_attempts WHERE input_hash=$1 AND model_version=$2
         AND outcome IN ('published','abstained') ORDER BY id DESC LIMIT 1",
    )
    .bind(&input_hash)
    .bind(backend.model())
    .fetch_optional(pool)
    .await?;
    let (output, attempt_id) = if let Some((id, _)) = previous {
        (None, id)
    } else {
        let (output, receipt) = create(backend, &assignment, voice_num_ctx).await?;
        let attempt_id:i64=sqlx::query_scalar(
            "INSERT INTO vibe_card_attempts(entity_type,entity_id,sport,input_hash,model_version,receipt,outcome)
             VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
            .bind(&item.entity_type).bind(entity_id).bind(&sport).bind(&input_hash)
            .bind(backend.model()).bind(&receipt).bind(if receipt.get("error").is_some(){"failed"}else{"generated"})
            .fetch_one(pool).await?;
        ensure!(
            receipt.get("error").is_none(),
            "Influencer attempt {attempt_id} failed: {}",
            receipt["error"]
        );
        (output, attempt_id)
    };
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    memories::validate(publication.transaction(), &assignment.parts).await?;
    let product_id = if let Some(output) = &output {
        Some(persist_to_vibe_scores(publication.transaction(), item, &sport, output).await?)
    } else {
        previous.and_then(|p| p.1)
    };
    let selected: Vec<i64> = assignment
        .parts
        .sources
        .iter()
        .map(|s| s.classification_id)
        .collect();
    let copies: Vec<i64> = assignment
        .parts
        .excluded
        .iter()
        .filter(|e| e["reason"] == "known_copy" && e["in_period"] == true)
        .filter_map(|e| e["classification_id"].as_i64())
        .collect();
    let covered: Vec<SourceContext> = pending
        .iter()
        .filter(|s| {
            selected.contains(&s.classification_id) || copies.contains(&s.classification_id)
        })
        .cloned()
        .collect();
    ensure!(
        !covered.is_empty(),
        "Influencer packet covers no pending work"
    );
    validate_for_publication(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
        &covered,
    )
    .await?;
    let ids: Vec<i64> = covered.iter().map(|s| s.classification_id).collect();
    let changed=sqlx::query(
        "UPDATE harvester_assignments SET status=CASE WHEN classification_id=ANY($5) THEN 'redundant' ELSE $3 END,
         reason=NULL,product_ref=$4,updated_at=NOW() WHERE classification_id=ANY($1)
         AND plugin_id=$2 AND status='pending' AND reason IS DISTINCT FROM 'delivery_held'")
        .bind(&ids).bind(plugin_id).bind(if product_id.is_some(){"used"}else{"abstained"})
        .bind(json!({"vibe_score_id":product_id,"attempt_id":attempt_id,"input_hash":input_hash,
            "reused_attempt":previous.is_some(),"disposition":disposition}))
        .bind(copies).execute(&mut **publication.transaction()).await?;
    ensure!(
        changed.rows_affected() == ids.len() as u64,
        "Influencer assignments changed during generation"
    );
    // Older receipts for the same covered article are superseded work, not new evidence.
    sqlx::query(
        "UPDATE harvester_assignments d SET status='relevant_but_unused',
        reason='superseded_source_receipt',updated_at=NOW()
        FROM harvester_classifications old WHERE old.id=d.classification_id AND d.plugin_id=$1
        AND d.status='pending' AND d.reason IS DISTINCT FROM 'delivery_held'
        AND EXISTS (SELECT 1 FROM harvester_classifications covered WHERE covered.id=ANY($2)
            AND covered.article_id=old.article_id AND covered.sport=old.sport
            AND covered.entity_type=old.entity_type AND covered.entity_id=old.entity_id
            AND (covered.created_at,covered.id)>(old.created_at,old.id))",
    )
    .bind(plugin_id)
    .bind(&ids)
    .execute(&mut **publication.transaction())
    .await?;
    if previous.is_none() {
        sqlx::query("UPDATE vibe_card_attempts SET product_id=$2,outcome=$3 WHERE id=$1")
            .bind(attempt_id)
            .bind(product_id)
            .bind(if product_id.is_some() {
                "published"
            } else {
                "abstained"
            })
            .execute(&mut **publication.transaction())
            .await?;
    }
    let remaining = crate::plugins::harvester::delivery::undelivered_count(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
    )
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
    } else {
        record_vibe_completed(publication.transaction(), item).await?;
        publication.commit_final().await?;
    }
    if let (Some(id), Some(output)) = (product_id, output.as_ref()) {
        record_ledger(pool, item, &sport, id, output).await?;
    }
    Ok(if remaining > 0 {
        PluginOutcome::deferred(
            format!("{remaining} sources await another period packet"),
            Duration::from_secs(1),
        )
    } else {
        PluginOutcome::Committed
    })
}

#[cfg(test)]
mod tests;
