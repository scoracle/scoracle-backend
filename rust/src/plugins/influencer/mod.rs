//! One daily fresh reporting package, with atomic publication.
pub mod fresh;
pub mod manifest;
mod parser;
pub mod prompt;
mod publish;
pub mod voice;

use crate::harness::model::Inference;
use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::{Generation, GenerationCall};
use crate::plugins::classifier::delivery::{load_for_character, validate_for_publication};
use crate::tools::meta::lookup_entity_name;
use crate::tools::meta::EntityMeta;
use crate::tools::source::SourceContext;
use anyhow::{ensure, Context, Result};
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
    let world = assignment.parts.assemble();
    anyhow::ensure!(
        world.len() <= prompt::SOURCE_BUDGET_BYTES + 2400,
        "Influencer packet exceeds reading budget"
    );
    let input_hash = prompt::request_hash(backend, assignment, num_ctx)?;
    let options = prompt::generation_options(VIBE_TEMPERATURE, num_ctx, VIBE_NUM_PREDICT);
    let mut receipt = json!({"model_version":backend.model(),"prompt_version":VIBE_PROMPT_VERSION,
        "scoring_version":prompt::SCORE_VERSION,"input_hash":input_hash,
        "packet":world,"input_components":assignment.parts,
        "request_body":backend.request_body(&world,&options)});
    if let Some(reason) = assignment.parts.hold_reason() {
        receipt["reason"] = json!("incomplete_reporting_day");
        receipt["hold_reason"] = json!(reason);
        return Ok((None, receipt));
    }
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
            receipt["context_exceeded"] =
                json!(error.is::<crate::harness::model::ContextExceeded>());
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
    receipt["response_model"] = json!(generated.model);
    match output_from_receipt(assignment, &receipt) {
        Ok(output) => Ok((output, receipt)),
        Err(error) => {
            receipt["error"] = json!(format!("{error:#}"));
            Ok((None, receipt))
        }
    }
}

fn output_from_receipt(assignment: &Assignment, receipt: &Value) -> Result<Option<VibeOutput>> {
    use crate::harness::Parser;
    ensure!(receipt.get("error").is_none(), "failed generation receipt");
    if receipt["reason"] == "no_fresh_evidence" && assignment.parts.sources.is_empty() {
        return Ok(None);
    }
    let Some(reply) =
        VibeParser.parse(receipt["raw_response"].as_str().context("saved response")?)?
    else {
        return Ok(None);
    };
    let mut source_ids: Vec<i64> = assignment
        .parts
        .sources
        .iter()
        .map(|s| s.article_id)
        .collect();
    source_ids.sort_unstable();
    source_ids.dedup();
    Ok(Some(Generation::called(
        VibeScore {
            sentiment: Some(reply.score),
            hook: Some(reply.headline),
            vibe_prompt: Some(reply.body),
            parts: assignment.parts.clone(),
        },
        receipt["response_model"]
            .as_str()
            .or_else(|| receipt["model_version"].as_str())
            .context("saved model")?
            .into(),
        VIBE_PROMPT_VERSION,
        source_ids,
        Some(
            receipt["input_hash"]
                .as_str()
                .context("saved input hash")?
                .into(),
        ),
        GenerationCall {
            built_prompt: receipt["packet"].as_str().context("saved packet")?.into(),
            request_body: receipt["request_body"].clone(),
            eval_count: receipt["eval_count"].as_i64().map(|n| n as i32),
            wall_ms: receipt["wall_ms"].as_u64(),
        },
    )))
}

async fn reusable_attempt(
    pool: &PgPool,
    input_hash: &str,
    model: &str,
) -> Result<Option<(i64, Option<i64>, String, Value)>> {
    Ok(sqlx::query_as(
        "SELECT id,product_id,outcome,receipt FROM vibe_card_attempts WHERE input_hash=$1 AND model_version=$2
         AND outcome IN ('generated','published','abstained') ORDER BY id DESC LIMIT 1",
    ).bind(input_hash).bind(model).fetch_optional(pool).await?)
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
    let unresolved = |source: &SourceContext| {
        !source.published_at_epoch.is_some_and(|date| date <= now())
            || source
                .classifier_world
                .as_ref()
                .is_some_and(|world| world["target_identity_resolved"] == false)
    };
    let held: Vec<_> = pending.iter().filter(|s| unresolved(s)).cloned().collect();
    if !held.is_empty() {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        validate_for_publication(
            publication.transaction(),
            plugin_id,
            &item.entity_type,
            entity_id,
            &sport,
            &held,
        )
        .await?;
        for source in &held {
            let reason = if source.published_at_epoch.is_none() {
                "unknown_publication_time"
            } else if source.published_at_epoch.is_some_and(|date| date > now()) {
                "future_publication_time"
            } else {
                "unresolved_target_identity"
            };
            crate::plugins::classifier::delivery::settle(
                publication.transaction(),
                plugin_id,
                &[source.classification_id],
                "held",
                Some(reason),
                json!({}),
            )
            .await?;
        }
        if held.len() == pending.len() {
            record_vibe_completed(publication.transaction(), item).await?;
            publication.commit_final().await?;
            return Ok(PluginOutcome::Committed);
        }
        publication.commit_progress().await?;
    }
    let pending: Vec<_> = pending.into_iter().filter(|s| !unresolved(s)).collect();
    let Some(_) = pending.last() else {
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
    let (mut assignment, disposition) = fresh::prepare_assignment(pool, subject, now()).await?;
    if assignment.parts.hold_reason().is_some() {
        return hold_daily(pool, item, &assignment, &pending, &disposition).await;
    }
    let mut input_hash = prompt::request_hash(backend, &assignment, voice_num_ctx)?;
    // A publication rollback must not discard an already validated model response.
    let mut previous = reusable_attempt(pool, &input_hash, backend.model()).await?;
    if previous.is_none() && !assignment.parts.memories.is_empty() {
        let mut parts = assignment.parts.clone();
        parts.memories.clear();
        let without_history = Assignment::from_parts(parts)?;
        let hash = prompt::request_hash(backend, &without_history, voice_num_ctx)?;
        previous = reusable_attempt(pool, &hash, backend.model()).await?;
        if previous.is_some() {
            assignment = without_history;
            input_hash = hash;
        }
    }
    let (output, attempt_id) = if let Some((id, _, outcome, receipt)) = &previous {
        (
            if outcome == "generated" {
                output_from_receipt(&assignment, receipt)?
            } else {
                None
            },
            *id,
        )
    } else {
        let (mut output, mut receipt) = create(backend, &assignment, voice_num_ctx).await?;
        if receipt["context_exceeded"] == true && !assignment.parts.memories.is_empty() {
            assignment.parts.memories.clear();
            assignment = Assignment::from_parts(assignment.parts)?;
            input_hash = prompt::request_hash(backend, &assignment, voice_num_ctx)?;
            (output, receipt) = create(backend, &assignment, voice_num_ctx).await?;
        }
        if receipt["context_exceeded"] == true {
            return hold_daily(pool, item, &assignment, &pending, &receipt).await;
        }
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
    fresh::validate(publication.transaction(), &assignment.parts).await?;
    let product_id = if let Some(output) = &output {
        Some(persist_to_vibe_scores(publication.transaction(), item, &sport, output).await?)
    } else {
        previous.as_ref().and_then(|p| p.1)
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
        .filter(|e| e["reason"] == "known_copy" && e["in_day"] == true)
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
    let product_ref = json!({"vibe_score_id":product_id,"attempt_id":attempt_id,"input_hash":input_hash,
        "reused_attempt":previous.is_some(),"disposition":disposition});
    let (duplicates, selected): (Vec<_>, Vec<_>) =
        ids.into_iter().partition(|id| copies.contains(id));
    crate::plugins::classifier::delivery::settle(
        publication.transaction(),
        plugin_id,
        &selected,
        if product_id.is_some() {
            "used"
        } else {
            "abstained"
        },
        None,
        product_ref.clone(),
    )
    .await?;
    crate::plugins::classifier::delivery::settle(
        publication.transaction(),
        plugin_id,
        &duplicates,
        "redundant",
        None,
        product_ref,
    )
    .await?;
    if previous.as_ref().is_none_or(|p| p.2 == "generated") {
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
    let remaining = crate::plugins::classifier::delivery::undelivered_count(
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
            format!("{remaining} sources await another day packet"),
            Duration::from_secs(1),
        )
    } else {
        PluginOutcome::Committed
    })
}

#[cfg(test)]
mod tests;

async fn hold_daily(
    pool: &PgPool,
    item: &Item,
    assignment: &Assignment,
    pending: &[SourceContext],
    disposition: &Value,
) -> Result<PluginOutcome> {
    let plugin_id = manifest::MANIFEST.id.as_str();
    let entity_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let held: Vec<_> = pending
        .iter()
        .filter(|source| {
            assignment.parts.sources.contains(source)
                || assignment.parts.excluded.iter().any(|entry| {
                    entry["in_day"] == true
                        && entry["classification_id"] == source.classification_id
                })
        })
        .cloned()
        .collect();
    ensure!(
        !held.is_empty(),
        "Influencer daily hold covers no pending work"
    );
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    validate_for_publication(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
        &held,
    )
    .await?;
    let ids: Vec<_> = held.iter().map(|s| s.classification_id).collect();
    crate::plugins::classifier::delivery::settle(
        publication.transaction(),
        plugin_id,
        &ids,
        "held",
        Some(if disposition["context_exceeded"] == true {
            "daily_package_exceeds_token_budget"
        } else {
            "incomplete_reporting_day"
        }),
        json!({"model_called":false,"disposition":disposition}),
    )
    .await?;
    let remaining = crate::plugins::classifier::delivery::undelivered_count(
        publication.transaction(),
        plugin_id,
        &item.entity_type,
        entity_id,
        &sport,
    )
    .await?;
    if remaining > 0 {
        publication.commit_progress().await?;
        return Ok(PluginOutcome::deferred(
            "other daily source obligations remain",
            Duration::from_secs(1),
        ));
    }
    record_vibe_completed(publication.transaction(), item).await?;
    publication.commit_final().await?;
    return Ok(PluginOutcome::Committed);
}
