//! Native source acquisition, replaceable model calls and claim-fenced receipts.
use super::{emotional_world, hash, prompt, qualify, Record, Source};
use crate::harness::model::{Inference, ResponseFailure};
use crate::harness::models::ExecutionCapabilities;
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::{
    publication::ClaimPublication,
    work::{self, Item},
};
use crate::harness::tools::WebBroker;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;

pub struct AcquireHandler {
    pool: PgPool,
    web: Arc<WebBroker>,
}
pub struct ClassifierHandler {
    pool: PgPool,
    models: ExecutionCapabilities,
}
impl AcquireHandler {
    pub fn new(pool: PgPool, web: Arc<WebBroker>) -> Self {
        Self { pool, web }
    }
}
impl ClassifierHandler {
    pub fn new(pool: PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

// The existing Go RSS collector owns this discovery table. Reading its candidate
// provenance does not call or import the retired Harvester/Editor plugins.
async fn discovery(pool: &PgPool, item: &Item) -> Result<Source> {
    ensure!(
        item.entity_type == "article",
        "Classifier requires an article claim"
    );
    let article = sqlx::query("SELECT url,title,COALESCE(source,'') AS source,published_at::text AS published_at,full_text,duplicate_of FROM public.news_articles WHERE id=$1")
        .bind(item.entity_id).fetch_one(pool).await.context("load Classifier source")?;
    let queries = sqlx::query("SELECT p.entity_type,p.entity_id,p.sport,p.feed_rank,t.name FROM public.harvester_query_provenance p LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport WHERE p.article_id=$1 AND p.sport=$2 ORDER BY p.entity_type,p.entity_id")
        .bind(item.entity_id).bind(&item.sport).fetch_all(pool).await?;
    ensure!(
        !queries.is_empty(),
        "Classifier requires explicit RSS candidate provenance"
    );
    for row in &queries {
        ensure!(
            row.get::<String, _>("entity_type") == "team"
                && row
                    .get::<Option<String>, _>("name")
                    .is_some_and(|name| !name.trim().is_empty()),
            "unsupported or unresolved RSS candidate identity"
        );
    }
    Ok(Source {
        article_id: item.entity_id,
        body: article
            .get::<Option<String>, _>("full_text")
            .unwrap_or_default(),
        source: article.get("source"),
        published_at: article.get("published_at"),
        query_entities: queries
            .iter()
            .map(|row| {
                json!({"entity_type":row.get::<String,_>("entity_type"),
            "entity_id":row.get::<i32,_>("entity_id"),"sport":row.get::<String,_>("sport"),
            "name":row.get::<Option<String>,_>("name"),"feed_rank":row.get::<Option<i32>,_>("feed_rank")})
            })
            .collect(),
        provenance: [
            ("url".into(), json!(article.get::<String, _>("url"))),
            ("title".into(), json!(article.get::<String, _>("title"))),
            (
                "duplicate_of".into(),
                json!(article.get::<Option<i64>, _>("duplicate_of")),
            ),
        ]
        .into(),
    })
}

fn input_hash(source: &Source) -> Result<String> {
    Ok(hash(&serde_json::to_string(source)?))
}

async fn fence_source(
    pool: &PgPool,
    item: &Item,
    expected: &Source,
    publication: &mut ClaimPublication<'_>,
) -> Result<()> {
    // RSS locks articles before enqueueing; publication locks its claim first.
    // Fail and retry a busy source instead of deadlocking the RSS transaction.
    sqlx::query("SELECT id FROM public.news_articles WHERE id=$1 FOR UPDATE NOWAIT")
        .bind(item.entity_id)
        .fetch_one(&mut **publication.transaction())
        .await?;
    // Lock candidate provenance and names too; acquisition and inference never hold these locks.
    sqlx::query("SELECT p.article_id FROM public.harvester_query_provenance p JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport WHERE p.article_id=$1 AND p.sport=$2 FOR SHARE OF p,t NOWAIT")
        .bind(item.entity_id).bind(&item.sport).fetch_all(&mut **publication.transaction()).await?;
    ensure!(
        input_hash(&discovery(pool, item).await?)? == input_hash(expected)?,
        "Classifier source or candidate provenance changed during work"
    );
    Ok(())
}

#[async_trait]
impl StudioPlugin for AcquireHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &super::manifest::ACQUIRE_MANIFEST
    }
    fn scheduled_operations(&self) -> Vec<Arc<dyn crate::harness::plugin::ScheduledOperation>> {
        vec![Arc::new(Replay(self.pool.clone()))]
    }
    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        ensure!(
            item.stage == super::manifest::ACQUIRE_TASK,
            "wrong Classifier acquisition task"
        );
        let before = discovery(&self.pool, item).await?;
        let key = input_hash(&before)?;
        let mut source = before.clone();
        if source.body.trim().is_empty() {
            let cached: Option<String> = sqlx::query_scalar("SELECT source::text FROM public.classifier_sources WHERE article_id=$1 AND sport=$2 AND source->>'url'=$3 ORDER BY id DESC LIMIT 1")
                .bind(item.entity_id).bind(&item.sport).bind(before.provenance["url"].as_str()).fetch_optional(&self.pool).await?;
            if let Some(cached) = cached {
                let cached: Source = serde_json::from_str(&cached)?;
                source.body = cached.body;
                for key in ["publisher_url", "publisher_domain"] {
                    if let Some(value) = cached.provenance.get(key) {
                        source.provenance.insert(key.into(), value.clone());
                    }
                }
            } else {
                let url = before.provenance["url"].as_str().context("publisher URL")?;
                let fetched = self
                    .web
                    .scope(&self.pool, self.manifest())
                    .fetch_curated_article(url)
                    .await?;
                source.body = fetched.text.replace('\0', "");
                source
                    .provenance
                    .insert("publisher_url".into(), json!(fetched.final_url));
                source
                    .provenance
                    .insert("publisher_domain".into(), json!(fetched.final_domain));
            }
        }
        ensure!(
            !source.body.trim().is_empty(),
            "publisher acquisition yielded no source text"
        );
        let identities =
            super::identity::candidates(&mut *self.pool.acquire().await?, &item.sport, &source)
                .await?;
        for candidate in identities.as_array().unwrap() {
            if !source.query_entities.iter().any(|q| {
                q["entity_type"] == candidate["entity_type"]
                    && q["entity_id"] == candidate["entity_id"]
            }) {
                source.query_entities.push(candidate.clone());
            }
        }
        source
            .provenance
            .insert("identity_candidates".into(), identities);
        let identity_hash = hash(&source.provenance["identity_candidates"].to_string());
        let Some(mut publication) = ClaimPublication::begin(&self.pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        super::identity::validate(publication.transaction(), &item.sport, &source).await?;
        fence_source(&self.pool, item, &before, &mut publication).await?;
        let source_id: i64 = sqlx::query_scalar("INSERT INTO public.classifier_sources(article_id,sport,input_hash,body_sha256,source,discovery_version,identity_hash) VALUES($1,$2,$3,$4,$5::jsonb,public.classifier_discovery_version($1,$2),$6) ON CONFLICT(article_id,sport,input_hash,body_sha256,identity_hash) DO UPDATE SET discovery_version=EXCLUDED.discovery_version RETURNING id")
            .bind(item.entity_id).bind(&item.sport).bind(&key).bind(hash(&source.body))
            .bind(serde_json::to_string(&source)?).bind(identity_hash).fetch_one(&mut **publication.transaction()).await?;
        sqlx::query("DELETE FROM public.news_article_entities WHERE article_id=$1 AND sport=$2")
            .bind(item.entity_id)
            .bind(&item.sport)
            .execute(&mut **publication.transaction())
            .await?;
        sqlx::query("INSERT INTO public.news_article_entities(article_id,entity_type,entity_id,sport,classifier_source_id)
            SELECT $1, c->>'entity_type', (c->>'entity_id')::integer,$2,$3
            FROM jsonb_array_elements($4::jsonb) c")
            .bind(item.entity_id).bind(&item.sport).bind(source_id).bind(&source.provenance["identity_candidates"])
            .execute(&mut **publication.transaction()).await?;
        work::enqueue(
            &mut **publication.transaction(),
            &Item {
                stage: super::manifest::TASK,
                input_version: Some(source_id.to_string()),
                claim_token: None,
                attempts: 0,
                ..item.clone()
            },
        )
        .await?;
        // Source-bound investigation nominations need acquired text, not calibrated scores.
        work::enqueue(
            &mut **publication.transaction(),
            &Item {
                stage: crate::plugins::graph::manifest::TASK,
                input_version: Some(format!("classifier-source:{source_id}")),
                claim_token: None,
                attempts: 0,
                ..item.clone()
            },
        )
        .await?;
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}

struct Replay(PgPool);
#[async_trait]
impl crate::harness::plugin::ScheduledOperation for Replay {
    fn name(&self) -> &'static str {
        "classifier.acquisition-replay"
    }
    async fn run(&self, cause: &'static str) -> Duration {
        match sqlx::query_scalar::<_, i64>("SELECT public.classifier_replay_acquisition(1000)")
            .fetch_one(&self.0)
            .await
        {
            Ok(queued) => {
                tracing::info!(cause, queued, "Classifier acquisition backlog reconciled");
                if queued == 1000 {
                    return Duration::from_secs(5);
                }
            }
            Err(error) => tracing::error!(cause, %error, "Classifier acquisition replay failed"),
        }
        Duration::from_secs(300)
    }
}

/// One stable untrusted-model boundary; neither storage nor consumers interpret provider output.
fn request_hash(
    model: &dyn Inference,
    source: &Source,
    target: &Value,
    revision: &Option<String>,
    request: &Value,
) -> Result<String> {
    Ok(hash(
        &json!({"contract":super::CONTRACT,"prompt_version":prompt::VERSION,"model":model.model(),
        "request":request,
        "source_snapshot_hash":input_hash(source)?,
        "source_identity":prompt::request(source,target)?["source_identity"],
        "revision":revision})
        .to_string(),
    ))
}

pub async fn measure(model: &dyn Inference, source: &Source, target: &Value) -> Result<Value> {
    let (built, options) = prompt::prepare(source, target)?;
    let identity = model.revision().await;
    let revision = identity.as_ref().ok().cloned().flatten();
    let request = model.request_body(&built, &options);
    let key = request_hash(model, source, target, &revision, &request)?;
    let mut receipt = json!({"contract":super::CONTRACT,"prompt_version":prompt::VERSION,
        "model":model.model(),"model_revision":revision,"request_hash":key,
        "source":source,"target":target,"request":request,"status":"error",
        "production_eligible":false,"calibration_status":"unassessed",
        "input_coverage":"not_confirmed","tokenizer_coverage_verified":false,
        "preflight":null,"measurements":super::measurements(source,target,None)?,"qualification":null,"selection":null,"raw_response":null,"error":null});
    let result = async {
        let revision = identity?;
        let prepared = model.prepare(&built, &options).await?;
        receipt["request"] = prepared.request.clone();
        receipt["request_hash"] = json!(request_hash(
            model,
            source,
            target,
            &revision,
            &prepared.request
        )?);
        receipt["preflight"] = serde_json::to_value(&prepared.coverage)?;
        prompt::check_budget(&built, &options, &prepared)?;
        receipt["input_coverage"] = json!("complete_source_submitted");
        let (generated, sent) = model.generate_prepared(&built, &options, &prepared).await?;
        receipt["raw_response"] = json!(generated.raw_response_body);
        receipt["response"] = json!(generated.response);
        receipt["thinking"] = json!(generated.thinking);
        receipt["prompt_eval_count"] = json!(generated.prompt_eval_count);
        receipt["eval_count"] = json!(generated.eval_count);
        receipt["wall_ms"] = json!(generated.total_duration.as_millis());
        receipt["completion_reason"] = json!(generated.completion_reason);
        ensure!(
            sent == prepared.request,
            "Classifier transport request changed after preparation"
        );
        ensure!(
            generated.prompt_eval_count as usize
                == prepared.coverage.as_ref().unwrap().token_ids.len(),
            "Classifier input token count differs from preflight"
        );
        receipt["tokenizer_coverage_verified"] = json!(true);
        receipt["input_coverage"] = json!("complete_source_verified");
        ensure!(
            generated.model == model.model(),
            "Classifier provider answered with another model"
        );
        ensure!(
            matches!(generated.completion_reason.as_deref(), Some("stop" | "eos")),
            "Classifier requires explicit complete output"
        );
        ensure!(
            revision == model.revision().await?,
            "Classifier model artifact changed during inference"
        );
        let qualified = qualify(source, target, &generated.response)?;
        let proposed: super::Proposal = serde_json::from_str(&generated.response)?;
        let measured = super::measurements(source, target, proposed.measurements)?;
        receipt["selection"] = emotional_world(source, &qualified)?;
        receipt["qualification"] = serde_json::to_value(qualified)?;
        receipt["measurements"] = serde_json::to_value(measured)?;
        receipt["status"] = json!("source_bound_provisional");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = result {
        if let Some(failure) = error.downcast_ref::<ResponseFailure>() {
            receipt["raw_response"] = json!(failure.raw_response_body);
        }
        receipt["error"] = json!(format!("{error:#}"));
        if receipt["raw_response"].is_null() {
            receipt["input_coverage"] = json!("not_confirmed");
        }
    }
    Ok(receipt)
}

#[async_trait]
impl StudioPlugin for ClassifierHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &super::manifest::MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let model = self.models.inference(prompt::MODEL)?;
        execute_model(&self.pool, item, model.as_ref()).await
    }
}

pub(crate) async fn execute_model(
    pool: &PgPool,
    item: &Item,
    model: &dyn Inference,
) -> Result<PluginOutcome> {
    ensure!(
        item.stage == super::manifest::TASK && item.entity_type == "article",
        "wrong Classifier task"
    );
    let source_id: i64 = item
        .input_version
        .as_deref()
        .context("Classifier requires acquired source revision")?
        .parse()?;
    let row = sqlx::query("SELECT source::text,input_hash FROM public.classifier_sources WHERE id=$1 AND article_id=$2 AND sport=$3")
            .bind(source_id).bind(item.entity_id).bind(&item.sport).fetch_one(pool).await?;
    let source: Source = serde_json::from_str(row.get::<&str, _>(0))?;
    let before = discovery(pool, item).await?;
    if input_hash(&before)? != row.get::<String, _>(1) {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        fence_source(pool, item, &before, &mut publication).await?;
        let revision: String =
            sqlx::query_scalar("SELECT public.classifier_discovery_version($1,$2)")
                .bind(item.entity_id)
                .bind(&item.sport)
                .fetch_one(&mut **publication.transaction())
                .await?;
        work::enqueue(
            &mut **publication.transaction(),
            &Item {
                stage: super::manifest::ACQUIRE_TASK,
                input_version: Some(revision),
                claim_token: None,
                attempts: 0,
                ..item.clone()
            },
        )
        .await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }

    for target in &source.query_entities {
        let revision = model.revision().await.unwrap_or(None);
        let reused = if revision.is_some() {
            let (built, options) = prompt::prepare(&source, target)?;
            match model.prepare(&built, &options).await {
                Ok(prepared) if prompt::check_budget(&built, &options, &prepared).is_ok() => {
                    let key = request_hash(model, &source, target, &revision, &prepared.request)?;
                    sqlx::query_scalar::<_,i64>("SELECT id FROM public.classifier_measurements WHERE article_id=$1 AND sport=$2 AND request_hash=$3 AND status='source_bound_provisional' AND receipt->>'tokenizer_coverage_verified'='true' ORDER BY id DESC LIMIT 1")
                        .bind(item.entity_id).bind(&item.sport).bind(&key).fetch_optional(pool).await?
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(id) = reused {
            ensure!(
                revision == model.revision().await?,
                "Classifier model artifact changed during reuse"
            );
            let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
                return Ok(PluginOutcome::Superseded);
            };
            fence_source(pool, item, &before, &mut publication).await?;
            super::identity::validate(publication.transaction(), &item.sport, &source).await?;
            super::delivery::record(publication.transaction(), id).await?;
            publication.commit_progress().await?;
            continue;
        }
        let mut receipt = measure(model, &source, target).await?;
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        let fence = async {
            fence_source(pool, item, &before, &mut publication).await?;
            super::identity::validate(publication.transaction(), &item.sport, &source).await
        }
        .await;
        if let Err(error) = fence {
            // Retain the attempt against its immutable historical source, never current evidence.
            receipt["status"] = json!("error");
            receipt["error"] = json!(format!("{error:#}; previous result: {}", receipt["error"]));
            receipt["qualification"] = Value::Null;
            receipt["selection"] = Value::Null;
            receipt["measurements"] =
                serde_json::to_value(super::measurements(&source, target, None)?)?;
        }
        let id:i64=sqlx::query_scalar("INSERT INTO public.classifier_measurements(source_id,article_id,sport,request_hash,status,receipt) VALUES($1,$2,$3,$4,$5,$6::jsonb) RETURNING id")
                .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(receipt["request_hash"].as_str())
                .bind(receipt["status"].as_str()).bind(receipt.to_string()).fetch_one(&mut **publication.transaction()).await?;
        if receipt["status"] == "source_bound_provisional" {
            super::delivery::record(publication.transaction(), id).await?;
        }
        publication.commit_progress().await?;
        ensure!(
            receipt["status"] == "source_bound_provisional",
            "{}",
            receipt["error"].as_str().unwrap_or("Classifier failed")
        );
    }
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    fence_source(pool, item, &before, &mut publication).await?;
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}

/// Consumers read the same typed source/claim contract regardless of the model implementation.
/// This exposes provisional evidence for plumbing/review; it does not publish character cards.
pub async fn load_measurement(
    pool: &PgPool,
    id: i64,
) -> Result<(Source, Record, super::Measurements)> {
    load_measurement_on(&mut *pool.acquire().await?, id).await
}

pub(crate) async fn load_measurement_on(
    connection: &mut sqlx::PgConnection,
    id: i64,
) -> Result<(Source, Record, super::Measurements)> {
    let row = sqlx::query("SELECT m.receipt::text AS receipt,m.article_id,m.sport,s.source::text AS source,s.body_sha256 FROM public.classifier_measurements m JOIN public.classifier_sources s ON s.id=m.source_id AND s.article_id=m.article_id AND s.sport=m.sport WHERE m.id=$1 AND m.status='source_bound_provisional'")
        .bind(id).fetch_one(connection).await?;
    let raw: &str = row.get("receipt");
    let receipt: Value = serde_json::from_str(&raw)?;
    let source: Source = serde_json::from_str(row.get("source"))?;
    ensure!(
        serde_json::to_value(&source)? == receipt["source"]
            && source.article_id == row.get::<i64, _>("article_id")
            && hash(&source.body) == row.get::<String, _>("body_sha256")
            && receipt["target"]["sport"] == row.get::<String, _>("sport")
            && receipt["status"] == "source_bound_provisional"
            && receipt["tokenizer_coverage_verified"] == true,
        "measurement receipt differs from acquired source or verified input"
    );
    let record: Record = serde_json::from_value(receipt["qualification"].clone())?;
    let response = receipt["response"].as_str().context("measurement reply")?;
    let qualified = qualify(&source, &receipt["target"], response)?;
    ensure!(
        serde_json::to_value(&qualified)? == serde_json::to_value(&record)?,
        "qualification differs from retained model reply"
    );
    let proposal: super::Proposal = serde_json::from_str(response)?;
    let measured = super::measurements(&source, &record.target, proposal.measurements)?;
    ensure!(
        serde_json::to_value(&measured)? == receipt["measurements"],
        "measurement envelope drift"
    );
    Ok((source, record, measured))
}
