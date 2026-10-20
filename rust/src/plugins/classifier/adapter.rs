//! Score and archive RSS headlines, then dispatch daily character obligations.
use super::{hash, Source};
use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::{publication::ClaimPublication, work::Item};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::{sync::Arc, time::Duration};

pub struct ClassifierHandler {
    pool: PgPool,
    decision: super::decision::Decision,
}
impl ClassifierHandler {
    pub fn new(pool: PgPool, decision: super::decision::Decision) -> Self {
        Self { pool, decision }
    }
}

// RSS query provenance supplies candidates, not relevance decisions.
async fn discovery(pool: &PgPool, item: &Item) -> Result<Source> {
    ensure!(
        item.entity_type == "article",
        "Classifier requires an article claim"
    );
    let article = sqlx::query("SELECT url,title,COALESCE(source,'') AS source,published_at::text AS published_at,duplicate_of FROM public.news_articles WHERE id=$1")
        .bind(item.entity_id).fetch_one(pool).await.context("load Classifier source")?;
    let queries: serde_json::Value =
        sqlx::query_scalar("SELECT public.classifier_headline_subjects($1,$2)")
            .bind(item.entity_id)
            .bind(&item.sport)
            .fetch_one(pool)
            .await?;
    let queries: Vec<serde_json::Value> = serde_json::from_value(queries)?;
    ensure!(
        !queries.is_empty() && queries.len() <= 128,
        "Classifier requires 1..128 canonical headline subjects"
    );
    Ok(Source {
        article_id: item.entity_id,
        body: String::new(),
        source: article.get("source"),
        published_at: article.get("published_at"),
        query_entities: queries,
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

pub(super) fn input_hash(source: &Source) -> Result<String> {
    Ok(hash(&serde_json::to_string(source)?))
}

pub(super) async fn fence_source(
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
    sqlx::query("SELECT article_id FROM harvester_query_provenance WHERE article_id=$1 AND sport=$2 FOR SHARE NOWAIT")
        .bind(item.entity_id).bind(&item.sport).fetch_all(&mut **publication.transaction()).await?;
    sqlx::query("SELECT article_id FROM news_article_entities WHERE article_id=$1 AND sport=$2 FOR SHARE NOWAIT")
        .bind(item.entity_id).bind(&item.sport).fetch_all(&mut **publication.transaction()).await?;
    for (kind, table) in [
        ("team", "teams"),
        ("player", "players"),
        ("person", "persons"),
    ] {
        let ids: Vec<i32> = expected
            .query_entities
            .iter()
            .filter(|c| c["entity_type"] == kind)
            .filter_map(|c| {
                c["entity_id"]
                    .as_i64()
                    .and_then(|id| i32::try_from(id).ok())
            })
            .collect();
        sqlx::query(&format!(
            "SELECT id FROM {table} WHERE sport=$1 AND id=ANY($2) ORDER BY id FOR SHARE NOWAIT"
        ))
        .bind(&item.sport)
        .bind(ids)
        .fetch_all(&mut **publication.transaction())
        .await?;
    }
    ensure!(
        input_hash(&discovery(pool, item).await?)? == input_hash(expected)?,
        "Classifier source or candidate provenance changed during work"
    );
    Ok(())
}

#[async_trait]
impl StudioPlugin for ClassifierHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &super::manifest::MANIFEST
    }
    fn scheduled_operations(&self) -> Vec<Arc<dyn crate::harness::plugin::ScheduledOperation>> {
        headline_replay(
            &self.pool,
            std::env::var("CLASSIFIER_REPLAY_BACKLOG").ok().as_deref(),
        )
    }
    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        ensure!(item.stage == super::manifest::TASK, "wrong admission task");
        let before = discovery(&self.pool, item).await?;
        let Some(measurements) =
            super::headline::measure(&self.pool, &self.decision, item, &before).await?
        else {
            return Ok(PluginOutcome::Superseded);
        };
        let discovery_hash = input_hash(&before)?;
        let key = hash(&format!(
            "{discovery_hash}\n{}",
            serde_json::to_string(&measurements)?
        ));
        let mut source = before.clone();
        source
            .provenance
            .insert("discovery_input_hash".into(), json!(discovery_hash));
        source.provenance.insert(
            "admission_criteria_version".into(),
            json!(super::decision::CRITERIA_VERSION),
        );
        source
            .provenance
            .insert("admission_measurements".into(), json!(measurements));
        let admitted = validate_admission(&self.pool, item, &before, &source)
            .await?
            .context("admission changed during measurement")?;
        source.provenance.insert("admitted".into(), json!(admitted));
        source
            .provenance
            .insert("evidence_scope".into(), json!("headline"));
        let identities =
            super::identity::candidates(&mut *self.pool.acquire().await?, &item.sport, &source)
                .await?;
        source
            .provenance
            .insert("identity_candidates".into(), identities);
        let Some(mut publication) = ClaimPublication::begin(&self.pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        fence_source(&self.pool, item, &before, &mut publication).await?;
        let source_id:i64=sqlx::query_scalar("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source,discovery_version,identity_hash) VALUES($1,$2,$3,$4,$5,public.classifier_discovery_version($1,$2),$6) ON CONFLICT(article_id,sport,input_hash,body_sha256,identity_hash) DO UPDATE SET discovery_version=EXCLUDED.discovery_version RETURNING id")
            .bind(item.entity_id).bind(&item.sport).bind(&key).bind(hash("")).bind(json!(source)).bind(hash(&source.provenance["identity_candidates"].to_string())).fetch_one(&mut **publication.transaction()).await?;
        super::identity::validate(publication.transaction(), &item.sport, &source).await?;
        sqlx::query("UPDATE classifier_spectrum_deliveries d SET status='superseded',reason='headline_revised',updated_at=now() FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.article_id=$1 AND x.sport=$2 AND x.source_id<>$3 AND d.status='pending'")
            .bind(item.entity_id).bind(&item.sport).bind(source_id).execute(&mut **publication.transaction()).await?;
        sqlx::query("INSERT INTO classifier_source_heads(article_id,sport,source_id) VALUES($1,$2,$3) ON CONFLICT(article_id,sport) DO UPDATE SET source_id=EXCLUDED.source_id")
            .bind(item.entity_id).bind(&item.sport).bind(source_id).execute(&mut **publication.transaction()).await?;
        for entry in source.provenance["admission_measurements"]
            .as_array()
            .context("admission batch")?
        {
            let measured =
                super::decision::Measurement::from_receipt(entry["measurement"].clone())?;
            let accepted = super::decision::admitted(&measured.spectrum)?;
            let characters = super::decision::characters(&measured.spectrum)?;
            let gate = json!({"coverage":"complete","scope":"headline","admitted":accepted,
                "characters":characters});
            let spectrum_id: i64 = sqlx::query_scalar("INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt) VALUES($1,$2,$3,$4,$5,$6,'rss-headline-spectrum-v1',$7,$8,$9,$10,$11,$12) ON CONFLICT(source_id,target,model,criteria_version,scrub_version,request_hash) DO UPDATE SET request_hash=EXCLUDED.request_hash RETURNING id")
                .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(&entry["target"]).bind(&measured.model).bind(super::decision::CRITERIA_VERSION)
                .bind(before.provenance["title"].as_str().context("headline")?).bind(hash(before.provenance["title"].as_str().context("headline")?))
                .bind(json!(measured.spectrum)).bind(gate).bind(&measured.request_hash).bind(&measured.receipt).fetch_one(&mut **publication.transaction()).await?;
            if accepted
                && !item
                    .input_version
                    .as_deref()
                    .is_some_and(|v| v.starts_with("headline-history:"))
            {
                sqlx::query("INSERT INTO classifier_spectrum_deliveries(spectrum_id,character)
                    SELECT $1,key FROM jsonb_each_text($2) WHERE value='true' AND NOT EXISTS (
                        SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
                        JOIN classifier_sources s ON s.id=x.source_id WHERE d.character=key AND d.status IN ('used','abstained','redundant')
                        AND x.article_id=$3 AND x.sport=$4 AND x.target=$5 AND x.request_hash=$6 AND x.model=$7 AND x.criteria_version=$8
                        AND s.source->>'published_at' IS NOT DISTINCT FROM $9::text)
                    ON CONFLICT(spectrum_id,character) DO UPDATE SET status='pending',reason=NULL,product_ref='{}'::jsonb,updated_at=now()
                    WHERE classifier_spectrum_deliveries.status='superseded' AND classifier_spectrum_deliveries.reason='headline_revised' ")
                    .bind(spectrum_id).bind(&characters).bind(item.entity_id).bind(&item.sport).bind(&entry["target"]).bind(&measured.request_hash).bind(&measured.model).bind(super::decision::CRITERIA_VERSION).bind(&before.published_at).execute(&mut **publication.transaction()).await?;
            }
        }
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}

async fn validate_admission(
    pool: &PgPool,
    item: &Item,
    before: &Source,
    source: &Source,
) -> Result<Option<bool>> {
    if source.provenance["admission_criteria_version"] != super::decision::CRITERIA_VERSION
        || source.provenance["discovery_input_hash"].as_str() != Some(input_hash(before)?.as_str())
    {
        return Ok(None);
    }
    let retained = source.provenance["admission_measurements"]
        .as_array()
        .context("retained admission batch")?;
    ensure!(
        !retained.is_empty() && retained.len() == before.query_entities.len(),
        "incomplete admission batch"
    );
    let mut accepted = false;
    let mut model = None;
    for (candidate, entry) in before.query_entities.iter().zip(retained) {
        let meta = super::headline::subject_meta(pool, candidate).await?;
        let target = json!({"entity_type":meta["identity"]["entity_type"],"entity_id":meta["identity"]["entity_id"],"sport":item.sport});
        ensure!(entry["target"] == target, "admission target changed");
        let measured = super::decision::Measurement::from_receipt(entry["measurement"].clone())?;
        if measured.receipt["request"]
            != super::decision::request(
                before.provenance["title"].as_str().context("headline")?,
                &before.source,
                &item.sport,
                &meta,
            )?
        {
            return Ok(None);
        }
        if let Some(expected) = &model {
            ensure!(
                &measured.model == expected,
                "mixed admission model fingerprints"
            );
        } else {
            model = Some(measured.model.clone());
        }
        accepted |= super::decision::admitted(&measured.spectrum)?;
    }
    Ok(Some(accepted))
}

// Backscoring is explicit; historical jobs archive spectra without creating fresh deliveries.
pub(super) fn headline_replay(
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
        "classifier.admission-replay"
    }
    async fn run(&self, cause: &'static str) -> Duration {
        match sqlx::query_scalar::<_, i64>("SELECT public.classifier_replay_admission(1000)")
            .fetch_one(&self.0)
            .await
        {
            Ok(queued) => {
                tracing::info!(cause, queued, "Classifier admission backlog reconciled");
                if queued == 1000 {
                    return Duration::from_secs(5);
                }
            }
            Err(error) => tracing::error!(cause, %error, "Classifier admission replay failed"),
        }
        Duration::from_secs(300)
    }
}
