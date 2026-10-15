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
    gliner: Arc<super::gliner::Gliner>,
}
impl AcquireHandler {
    pub fn new(pool: PgPool, web: Arc<WebBroker>) -> Self {
        Self { pool, web }
    }
}
impl ClassifierHandler {
    pub fn new(pool: PgPool, gliner: Arc<super::gliner::Gliner>) -> Self {
        Self { pool, gliner }
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
                source.body = fetched.body.replace('\0', "");
                source
                    .provenance
                    .insert("publisher_url".into(), json!(fetched.final_url));
                source
                    .provenance
                    .insert("publisher_domain".into(), json!(fetched.domain));
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
        execute_spectrum(&self.pool, item, &self.gliner).await
    }
}

/// Native spectrum path: classify with the candle GLiNER model, scrub, gate and store one
/// immutable `classifier_spectra` row per target. No generative call.
pub async fn execute_spectrum(
    pool: &PgPool,
    item: &Item,
    gliner: &Arc<super::gliner::Gliner>,
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

    let window_texts: Vec<String> = crate::tools::source::windows(&source.body, 256)?
        .into_iter()
        .map(|window| window.text)
        .collect();
    let scrubbed = crate::tools::source::scrub(&source.body);
    let clean_sha256 = hash(&scrubbed.text);
    for target in &source.query_entities {
        let target_name = target["name"].as_str().context("target name")?.to_string();
        let gliner = gliner.clone();
        let windows = window_texts.clone();
        let spectrum = tokio::task::spawn_blocking(move || {
            let mut aggregated: std::collections::BTreeMap<String, f32> = Default::default();
            for text in &windows {
                for (criterion, value) in gliner.spectrum(text, &target_name)? {
                    let slot = aggregated.entry(criterion).or_insert(0.0);
                    if value > *slot {
                        *slot = value;
                    }
                }
            }
            Ok::<_, anyhow::Error>(aggregated)
        })
        .await??;
        let gate = super::gliner::gate(&spectrum)?;
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        fence_source(pool, item, &before, &mut publication).await?;
        super::identity::validate(publication.transaction(), &item.sport, &source).await?;
        let spectrum_id: i64 = sqlx::query_scalar("INSERT INTO public.classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate) VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb) ON CONFLICT (article_id,sport,target,model,criteria_version) DO UPDATE SET gate=EXCLUDED.gate RETURNING id")
            .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(target.to_string())
            .bind(super::gliner::MODEL_ID).bind(super::gliner::CRITERIA_VERSION).bind("scrub-v1")
            .bind(&scrubbed.text).bind(&clean_sha256)
            .bind(serde_json::to_string(&spectrum)?).bind(serde_json::to_string(&gate)?)
            .fetch_one(&mut **publication.transaction()).await?;
        // One obligation per character whose bucket fired; a complete measurement with no eligible
        // signal creates none (the plan's no-action disposition).
        for (character, fires) in &gate {
            if *fires {
                sqlx::query("INSERT INTO public.classifier_spectrum_deliveries(spectrum_id,character) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(spectrum_id).bind(character)
                    .execute(&mut **publication.transaction()).await?;
            }
        }
        publication.commit_progress().await?;
    }
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    fence_source(pool, item, &before, &mut publication).await?;
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}


