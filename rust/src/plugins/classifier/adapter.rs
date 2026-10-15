//! Native source acquisition, replaceable model calls and claim-fenced receipts.
use super::{hash, Source};
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::{
    publication::ClaimPublication,
    work::{self, Item},
};
use crate::harness::tools::WebBroker;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;

pub struct AcquireHandler {
    pool: PgPool,
    web: Arc<WebBroker>,
}
pub struct ClassifierHandler {
    pool: PgPool,
    decision: super::decision::Decision,
}
impl AcquireHandler {
    pub fn new(pool: PgPool, web: Arc<WebBroker>) -> Self {
        Self { pool, web }
    }
}
impl ClassifierHandler {
    pub fn new(pool: PgPool, decision: super::decision::Decision) -> Self {
        Self { pool, decision }
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
        acquisition_replay(
            &self.pool,
            std::env::var("CLASSIFIER_REPLAY_BACKLOG").ok().as_deref(),
        )
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
            let url = before.provenance["url"].as_str().context("publisher URL")?;
            // Budgeted fetch: per-domain spacing, 429/Retry-After holds and a circuit breaker,
            // instead of hammering a publisher that is throttling us.
            let fetched = self
                .web
                .scope(&self.pool, self.manifest())
                .fetch_for_class(
                    crate::harness::tools::DomainClass::CuratedArticles,
                    url,
                    &crate::tools::fetch::FetchPolicy::default(),
                )
                .await?;
            source.body =
                crate::tools::fetch::extract_article_text(&fetched.body).replace('\0', "");
            source
                .provenance
                .insert("publisher_url".into(), json!(fetched.final_url));
            source
                .provenance
                .insert("publisher_domain".into(), json!(fetched.domain));
        }
        if source.body.trim_start().starts_with('<') {
            source.body = crate::tools::fetch::extract_article_text(&source.body).replace('\0', "");
        }
        if before.body.trim().is_empty() {
            ensure!(
                !source
                    .provenance
                    .get("publisher_domain")
                    .is_some_and(|domain| domain == "news.google.com")
                    && crate::tools::fetch::count_words(&source.body)
                        >= crate::tools::fetch::ARTICLE_MIN_WORDS,
                "publisher acquisition requires article prose, not a wrapper or unavailable page"
            );
        }
        ensure!(
            !source.body.trim().is_empty()
                && !crate::tools::fetch::article_unavailable(&source.body),
            "publisher acquisition yielded no available article text"
        );
        let identities =
            super::identity::candidates(&mut *self.pool.acquire().await?, &item.sport, &source)
                .await?;
        for candidate in identities.as_array().unwrap() {
            if matches!(candidate["entity_type"].as_str(), Some("team" | "player"))
                && !source.query_entities.iter().any(|q| {
                    q["entity_type"] == candidate["entity_type"]
                        && q["entity_id"] == candidate["entity_id"]
                })
            {
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
        sqlx::query("INSERT INTO public.classifier_source_heads(article_id,sport,source_id) VALUES($1,$2,$3)
            ON CONFLICT(article_id,sport) DO UPDATE SET source_id=EXCLUDED.source_id")
            .bind(item.entity_id).bind(&item.sport).bind(source_id)
            .execute(&mut **publication.transaction()).await?;
        // The revision's canonical names must see the newly selected publisher body.
        sqlx::query("UPDATE classifier_sources SET discovery_version=public.classifier_discovery_version($1,$2) WHERE id=$3")
            .bind(item.entity_id).bind(&item.sport).bind(source_id)
            .execute(&mut **publication.transaction()).await?;
        sqlx::query("UPDATE public.classifier_spectrum_deliveries d SET status='superseded',updated_at=now()
            FROM public.classifier_spectra x WHERE x.id=d.spectrum_id AND x.article_id=$1 AND x.sport=$2
            AND x.source_id<>$3 AND d.status='pending'")
            .bind(item.entity_id).bind(&item.sport).bind(source_id)
            .execute(&mut **publication.transaction()).await?;
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
        // Source-bound investigation nominations need acquired text, not calibrated scores.
        for (stage, version) in [
            (super::manifest::TASK, source_id.to_string()),
            (
                crate::plugins::graph::manifest::TASK,
                format!("classifier-source:{source_id}"),
            ),
        ] {
            work::enqueue(
                &mut **publication.transaction(),
                &Item {
                    stage,
                    input_version: Some(version),
                    claim_token: None,
                    attempts: 0,
                    ..item.clone()
                },
            )
            .await?;
        }
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}

// Controlled calibration runs must not silently enqueue the whole retained corpus.
pub(super) fn acquisition_replay(
    pool: &PgPool,
    setting: Option<&str>,
) -> Vec<Arc<dyn crate::harness::plugin::ScheduledOperation>> {
    if setting == Some("true") {
        vec![Arc::new(Replay(pool.clone()))]
    } else {
        vec![]
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

#[async_trait]
impl StudioPlugin for ClassifierHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &super::manifest::MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        execute_spectrum(&self.pool, item, &self.decision).await
    }
}

/// Classifier stage 1: score the spectrum with the System-1 decision model, scrub,
/// gate and retain one article spectrum plus RSS-subject delivery projections. One model call per article.
pub async fn execute_spectrum(
    pool: &PgPool,
    item: &Item,
    decision: &super::decision::Decision,
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
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT source_id FROM classifier_source_heads WHERE article_id=$1 AND sport=$2",
    )
    .bind(item.entity_id)
    .bind(&item.sport)
    .fetch_optional(pool)
    .await?;
    if current != Some(source_id) {
        let Some(publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    if input_hash(&before)? != row.get::<String, _>(1)
        || source.body.trim_start().starts_with('<')
        || crate::tools::fetch::article_unavailable(&source.body)
        || source
            .provenance
            .get("publisher_domain")
            .is_some_and(|domain| domain == "news.google.com")
    {
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

    let scrubbed = crate::tools::source::scrub(&source.body);
    ensure!(!scrubbed.text.trim().is_empty(), "empty scrubbed source");
    let clean_sha256 = hash(&scrubbed.text);
    let model = decision.identity().await?;
    let request = super::decision::request(&scrubbed.text, &item.sport)?;
    let request_hash = hash(&request.to_string());
    let article_target =
        json!({"entity_type":"article","entity_id":source.article_id,"sport":item.sport});
    let retained: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM classifier_spectra
        WHERE source_id=$1 AND target=$2::jsonb AND model=$3 AND criteria_version=$4
        AND scrub_version='scrub-v2-publisher' AND request_hash=$5)",
    )
    .bind(source_id)
    .bind(article_target.to_string())
    .bind(&model)
    .bind(super::decision::CRITERIA_VERSION)
    .bind(&request_hash)
    .fetch_one(pool)
    .await?;
    if retained {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        fence_source(pool, item, &before, &mut publication).await?;
        let head: i64 = sqlx::query_scalar("SELECT source_id FROM classifier_source_heads WHERE article_id=$1 AND sport=$2 FOR SHARE")
            .bind(item.entity_id).bind(&item.sport).fetch_one(&mut **publication.transaction()).await?;
        ensure!(
            head == source_id,
            "Classifier acquired source changed during retry"
        );
        super::identity::validate(publication.transaction(), &item.sport, &source).await?;
        publication.commit_final().await?;
        return Ok(PluginOutcome::Committed);
    }
    // RSS rank and identity metadata can change without changing the model's exact input.
    let cached: Option<String> = sqlx::query_scalar(
        "SELECT receipt::text FROM classifier_spectra
        WHERE article_id=$1 AND sport=$2 AND target->>'entity_type'='article'
        AND model=$3 AND criteria_version=$4 AND scrub_version='scrub-v2-publisher'
        AND request_hash=$5 ORDER BY id DESC LIMIT 1",
    )
    .bind(item.entity_id)
    .bind(&item.sport)
    .bind(&model)
    .bind(super::decision::CRITERIA_VERSION)
    .bind(&request_hash)
    .fetch_optional(pool)
    .await?;
    let measured = if let Some(cached) = cached {
        let mut receipt: serde_json::Value = serde_json::from_str(&cached)?;
        ensure!(
            receipt["request"] == request,
            "cached Classifier request mismatch"
        );
        receipt["cache_hit"] = json!(true);
        super::decision::Measurement::from_receipt(receipt)
    } else {
        decision.spectrum(&scrubbed.text, &item.sport).await
    };
    let measurement = match measured {
        Ok(measurement) => measurement,
        Err(error) => {
            let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
                return Ok(PluginOutcome::Superseded);
            };
            fence_source(pool, item, &before, &mut publication).await?;
            let receipt = json!({"status":"error","production_eligible":false,"request":request,
                "error":format!("{error:#}"),"raw_response":error.downcast_ref::<crate::harness::model::ResponseFailure>()
                    .map(|failure| &failure.raw_response_body)});
            sqlx::query("INSERT INTO classifier_measurements(source_id,article_id,sport,request_hash,status,receipt)
                VALUES($1,$2,$3,$4,'error',$5::jsonb)")
                .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(&request_hash)
                .bind(receipt.to_string()).execute(&mut **publication.transaction()).await?;
            publication.commit_progress().await?;
            return Err(error);
        }
    };
    ensure!(
        measurement.model == model,
        "Classifier serving model changed during work"
    );
    let gate = super::decision::gate(&measurement.spectrum)?;
    let gate_doc = json!({"characters": &gate, "coverage": "complete", "scope":"article"});
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    fence_source(pool, item, &before, &mut publication).await?;
    let head: i64 = sqlx::query_scalar(
        "SELECT source_id FROM classifier_source_heads WHERE article_id=$1 AND sport=$2 FOR SHARE",
    )
    .bind(item.entity_id)
    .bind(&item.sport)
    .fetch_one(&mut **publication.transaction())
    .await?;
    ensure!(
        head == source_id,
        "Classifier acquired source changed during inference"
    );
    super::identity::validate(publication.transaction(), &item.sport, &source).await?;
    sqlx::query(
        "UPDATE classifier_spectrum_deliveries d SET status='superseded',
        reason='replaced_by_article_spectrum',updated_at=now() FROM classifier_spectra x
        WHERE x.id=d.spectrum_id AND x.article_id=$1 AND x.sport=$2 AND d.status='pending'",
    )
    .bind(item.entity_id)
    .bind(&item.sport)
    .execute(&mut **publication.transaction())
    .await?;
    // One authoritative article measurement; entity rows are delivery projections of that receipt.
    // Only RSS query subjects receive work. Other canonical mentions remain retained Graph evidence.
    for target in std::iter::once(&article_target).chain(before.query_entities.iter()) {
        let spectrum_id: i64 = sqlx::query_scalar("INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
            VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb,$12,$13::jsonb)
            ON CONFLICT (source_id,target,model,criteria_version,scrub_version,request_hash)
            DO UPDATE SET gate=classifier_spectra.gate RETURNING id")
            .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(target.to_string())
            .bind(&measurement.model).bind(super::decision::CRITERIA_VERSION).bind("scrub-v2-publisher")
            .bind(&scrubbed.text).bind(&clean_sha256)
            .bind(serde_json::to_string(&measurement.spectrum)?).bind(gate_doc.to_string())
            .bind(&measurement.request_hash).bind(measurement.receipt.to_string())
            .fetch_one(&mut **publication.transaction()).await?;
        if target["entity_type"] == "article" {
            continue;
        }
        for (character, fires) in &gate {
            if *fires {
                sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(spectrum_id).bind(character).execute(&mut **publication.transaction()).await?;
            }
        }
    }
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}
