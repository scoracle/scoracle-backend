//! Insider execution: read verified source packages and publish under the queue claim.
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
use crate::harness::{Generation, GenerationCall, Studio};
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use parser::ReplyParser;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Reported,
    Denied,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub report_index: usize,
    pub counterparty: String,
    pub status: Status,
    pub stage: Option<String>,
    pub evidence_quote: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub body: String,
    pub findings: Vec<Finding>,
}

pub(crate) const TRANSFER_PUBLISHED: &str = "transfer_published";

pub(crate) async fn record_transfer_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &Item,
    kind: &'static str,
    entity_type: &str,
    entity_id: i32,
    input_version: Option<&str>,
) -> Result<()> {
    crate::harness::queue::outbox::record(
        tx,
        item,
        crate::harness::queue::outbox::NewEvent {
            kind,
            entity_type,
            entity_id,
            source_input_version: input_version,
        },
    )
    .await
    .context("record transfer publication")
}

pub async fn create(
    studio: &Studio<'_>,
    subject: &EntityMeta,
    reports: &[prompt::Report],
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    create_with_memories(studio, subject, reports, &[], num_ctx).await
}

async fn create_with_memories(
    studio: &Studio<'_>,
    subject: &EntityMeta,
    reports: &[prompt::Report],
    memories: &[crate::plugins::classifier::memories::Headline],
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    ensure!(!reports.is_empty(), "Insider needs a verified source");
    let world = prompt::assemble_with_memories(subject, reports, memories);
    ensure!(
        world.len() <= prompt::CONTEXT_BUDGET_BYTES,
        "Insider article package exceeds reading budget"
    );
    let hash = crate::util::hash_components(&world);
    let extracted = studio
        .extract(
            &world,
            &prompt::options_for_reports(num_ctx, subject, reports),
            &ReplyParser { subject, reports },
            crate::harness::session::publishing_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let value = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("Insider returned no reply"))?;
    Ok(Generation::called(
        value,
        extracted.model,
        prompt::PROMPT_VERSION,
        Vec::new(),
        Some(hash),
        call,
    ))
}

pub struct TransferHandler {
    pool: PgPool,
    models: ExecutionCapabilities,
}

impl TransferHandler {
    pub fn new(pool: PgPool, models: ExecutionCapabilities) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for TransferHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::insider::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        if item
            .input_version
            .as_deref()
            .is_some_and(|version| version.starts_with(crate::plugins::classifier::CONTRACT))
        {
            let backend = self.models.inference(manifest::ROUTE)?;
            return execute_with_backend(
                &self.pool,
                backend.as_ref(),
                self.models.voice_num_ctx,
                item,
            )
            .await;
        }
        // Retire work created by the old periodic pipeline.
        let Some(publication) = ClaimPublication::begin(&self.pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        publication.commit_final().await?;
        Ok(PluginOutcome::Committed)
    }
}

pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let material = fresh::load_material(pool, item).await?;
    execute_prepared(pool, backend, num_ctx, item, material).await
}

async fn execute_prepared(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
    mut material: fresh::Material,
) -> Result<PluginOutcome> {
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM public.pipeline_work \
         WHERE stage=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4 \
           AND status='running' AND claim_token=$5::uuid \
           AND running_input_version IS NOT DISTINCT FROM $6)",
    )
    .bind(item.stage.as_str())
    .bind(&item.entity_type)
    .bind(item.entity_id)
    .bind(&item.sport)
    .bind(item.require_claim_token()?)
    .bind(item.input_version.as_deref())
    .fetch_one(pool)
    .await?;
    if !current {
        return Ok(PluginOutcome::Superseded);
    }
    if !material.held.is_empty() || material.sources.is_empty() {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        let held: Vec<_> = material
            .held
            .iter()
            .map(|(source, _)| source.clone())
            .collect();
        crate::plugins::classifier::delivery::validate_for_publication(
            publication.transaction(),
            manifest::MANIFEST.id.as_str(),
            &item.entity_type,
            item.entity_id_i32()?,
            &material.subject.sport,
            &held,
        )
        .await?;
        for (source, reason) in &material.held {
            crate::plugins::classifier::delivery::settle(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &[source.classification_id],
                if *reason == "known_copy" {
                    "redundant"
                } else if *reason == "livestream_spam" {
                    "abstained"
                } else {
                    "held"
                },
                Some(reason),
                serde_json::json!({"model_called":false}),
            )
            .await?;
        }
        if material.sources.is_empty() {
            publication.commit_final().await?;
            return Ok(PluginOutcome::Committed);
        }
        publication.commit_progress().await?;
    }
    let result = loop {
        let result = create_with_memories(
            &Studio::new(backend),
            &material.subject,
            &material.reports,
            &material.memories,
            num_ctx,
        )
        .await;
        if result
            .as_ref()
            .err()
            .is_some_and(|error| error.is::<crate::harness::model::ContextExceeded>())
            && !material.memories.is_empty()
        {
            material.memories.clear();
            continue;
        }
        break result;
    };
    let generation = match result {
        Ok(generation) => generation,
        Err(error) => {
            let (reason, receipt) = if let Some(rejected) =
                error.downcast_ref::<crate::harness::model::RejectedReply>()
            {
                (
                    "model_reply_rejected",
                    serde_json::json!({"model":backend.model(),"prompt_version":prompt::PROMPT_VERSION,
                    "error":format!("{error:#}"),"raw_response_body":rejected.raw_response_body,
                    "request":rejected.request_body,"prompt":rejected.built_prompt}),
                )
            } else if error.is::<crate::harness::model::ContextExceeded>() {
                (
                    "daily_package_exceeds_token_budget",
                    serde_json::json!({"model_called":false,"error":error.to_string()}),
                )
            } else {
                return Err(error);
            };
            let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
                return Ok(PluginOutcome::Superseded);
            };
            crate::plugins::classifier::delivery::validate_for_publication(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &item.entity_type,
                item.entity_id_i32()?,
                &material.subject.sport,
                &material.sources,
            )
            .await?;
            let ids: Vec<_> = material
                .sources
                .iter()
                .map(|s| s.classification_id)
                .collect();
            crate::plugins::classifier::delivery::settle(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &ids,
                "held",
                Some(reason),
                serde_json::json!({"rejected_attempt":receipt}),
            )
            .await?;
            let remaining = crate::plugins::classifier::delivery::undelivered_count(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &item.entity_type,
                item.entity_id_i32()?,
                &material.subject.sport,
            )
            .await?;
            if remaining > 0 {
                publication.commit_progress().await?;
                return Ok(PluginOutcome::deferred(
                    "rejected Insider article held; more reports remain",
                    std::time::Duration::from_secs(1),
                ));
            }
            publication.commit_final().await?;
            return Ok(PluginOutcome::Committed);
        }
    };
    publish::commit(pool, item, &material, &generation).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::model::{GenerateResult, Inference};
    use crate::harness::{model::GenerateOptions, Parser};
    use async_trait::async_trait;
    use std::{sync::Mutex, time::Duration};

    struct Fake(Mutex<usize>, bool);
    #[async_trait]
    impl Inference for Fake {
        async fn generate(
            &self,
            _: &str,
            _: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let mut calls = self.0.lock().unwrap();
            *calls += 1;
            let name_only = self.1 && *calls == 1;
            drop(calls);
            Ok((GenerateResult {
                response: if name_only {r#"{"body":"Jordan Sample","findings":[]}"#} else {r#"{"body":"Jordan Sample is linked with Cleveland, according to Wire.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"reported","stage":"concrete_interest","evidence_quote":"Cleveland Browns is monitoring Jordan Sample in trade talks"}]}"#}.into(),
                thinking: String::new(), model: "fake".into(), total_duration: Duration::from_millis(1),
                prompt_eval_count: 10, eval_count: 10, completion_reason: Some("stop".into()), raw_response_body: "{}".into(),
            }, serde_json::json!({})))
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({})
        }
    }
    #[tokio::test]
    #[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
    async fn known_owner_headline_publishes_a_person_card_and_fenced_source_outcome() -> Result<()>
    {
        let pool = crate::harness::dbtest::pool("disposable classifier_test database").await;
        crate::plugins::classifier::plumbing_tests::setup_disposable(&pool).await?;
        let sport = "ZZ_PERSON_CARD";
        sqlx::query(
            "UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL",
        )
        .execute(&pool)
        .await?;
        sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM news_articles WHERE id=940101")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM insider_scores WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM transfer_rumors WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM application_outbox WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Person card',2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await?;
        sqlx::query("INSERT INTO teams(id,sport,name) VALUES(940101,$1,'Cleveland Browns') ON CONFLICT(id,sport) DO NOTHING").bind(sport).execute(&pool).await?;
        sqlx::query("INSERT INTO persons(id,sport,full_name,kind) VALUES(940101,$1,'Jordan Sample','owner') ON CONFLICT(id) DO UPDATE SET sport=EXCLUDED.sport,full_name=EXCLUDED.full_name,kind=EXCLUDED.kind").bind(sport).execute(&pool).await?;
        sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',940101,$1,nrm('Cleveland Browns'),'name'),('person',940101,$1,nrm('Jordan Sample'),'name')").bind(sport).execute(&pool).await?;
        sqlx::query("INSERT INTO news_articles(id,url,url_hash,title,source,published_at) VALUES(940101,'https://example.invalid/person-card',md5('person-card'),'Cleveland Browns is monitoring Jordan Sample in trade talks; Cleveland Browns consider talks.','Wire',now())").execute(&pool).await?;
        sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES(940101,'team',940101,$1)").bind(sport).execute(&pool).await?;
        sqlx::query("SELECT classifier_enqueue_admission(940101,$1)")
            .bind(sport)
            .execute(&pool)
            .await?;
        let scorer = crate::plugins::classifier::tests::Model::new()?;
        let handler = crate::plugins::classifier::adapter::ClassifierHandler::new(
            pool.clone(),
            scorer.decision()?,
        );
        let claim = crate::harness::queue::work::claim(
            &pool,
            crate::plugins::classifier::manifest::TASK,
            1,
        )
        .await?
        .remove(0);
        handler.execute(&claim).await?;
        let mut claimed = None;
        for item in crate::harness::queue::work::claim(&pool, manifest::TASK, 100).await? {
            if item.entity_type == "person" && item.entity_id == 940101 && item.sport == sport {
                claimed = Some(item);
            } else {
                crate::harness::queue::work::release(&pool, &item).await?;
            }
        }
        let item = claimed.context("person transfer claim")?;
        let material = fresh::load_material(&pool, &item).await?;
        assert_eq!(material.sources.len(), 1);
        let backend = Fake(Mutex::new(0), false);
        assert!(matches!(
            execute_prepared(&pool, &backend, 32768, &item, material).await?,
            PluginOutcome::Committed
        ));
        let body:String=sqlx::query_scalar("SELECT read FROM insider_scores WHERE sport=$1 AND entity_type='person' AND entity_id=940101").bind(sport).fetch_one(&pool).await?;
        assert!(body.contains("Jordan Sample"));
        let settled:i64=sqlx::query_scalar("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport=$1 AND x.target->>'entity_type'='person' AND d.character='insider' AND d.status='used' AND d.product_ref->>'score_id' IS NOT NULL").bind(sport).fetch_one(&pool).await?;
        assert_eq!(settled, 1);
        let reactions = crate::harness::registration::build_reactions(pool.clone())?;
        assert_eq!(
            crate::harness::queue::outbox::drain(&pool, &reactions, 100).await?,
            1
        );
        let statistics:i64=sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1 AND entity_type='person' AND stage IN ('rating','momentum','sigil')").bind(sport).fetch_one(&pool).await?;
        assert_eq!(statistics, 0);
        sqlx::query("DELETE FROM pipeline_work WHERE sport=$1")
            .bind(sport)
            .execute(&pool)
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn one_call_returns_reading_and_exact_source_finding() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let report = prompt::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt:
                "Cleveland Browns is monitoring Jordan Sample in trade talks in trade talks.".into(),
            co_mentions: vec![prompt::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        for (name_only, calls) in [(false, 1), (true, 2)] {
            let fake = Fake(Mutex::new(0), name_only);
            let generated = create(
                &Studio::new(&fake),
                &subject,
                std::slice::from_ref(&report),
                4096,
            )
            .await
            .unwrap();
            assert_eq!(*fake.0.lock().unwrap(), calls);
            assert_eq!(generated.product.findings.len(), 1);
            assert!(generated.product.body.contains("Wire"));
            let final_world: serde_json::Value =
                serde_json::from_str(&generated.call.unwrap().built_prompt).unwrap();
            let original: serde_json::Value =
                serde_json::from_str(&prompt::assemble(&subject, std::slice::from_ref(&report)))
                    .unwrap();
            assert_eq!(final_world["fresh"], original["fresh"]);
        }
    }

    #[test]
    fn oversized_prose_and_findings_are_rejected_even_without_native_schema_enforcement() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 1,
            sport: "NFL".into(),
        };
        let reports = [prompt::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Movement".into(),
            publisher_excerpt: "Jordan Sample is considering a move.".into(),
            co_mentions: vec![],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        let too_long = serde_json::json!({"body":format!("Jordan Sample {}","x".repeat(prompt::BODY_MAX_CHARS)),"findings":[]});
        assert!(parser
            .parse(&too_long.to_string())
            .unwrap_err()
            .to_string()
            .contains("body exceeds"));
        let too_many = serde_json::json!({"body":"Jordan Sample is considering a move.","findings":(0..3).map(|i|
            serde_json::json!({"report_index":0,"counterparty":format!("Club {i}"),"status":"reported","stage":"speculation","evidence_quote":"considering a move"})).collect::<Vec<_>>()});
        assert!(parser
            .parse(&too_many.to_string())
            .unwrap_err()
            .to_string()
            .contains("too many findings"));
    }

    #[test]
    fn rejects_a_finding_without_an_exact_publisher_quote() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let reports = [prompt::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt: "Cleveland Browns is monitoring Jordan Sample in trade talks."
                .into(),
            co_mentions: vec![prompt::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        assert!(parser.parse(r#"{"body":"Jordan Sample is linked with Cleveland.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"reported","stage":"concrete_interest","evidence_quote":"Cleveland has signed Jordan Sample"}]}"#).is_err());
    }

    #[test]
    fn explicit_denial_requires_null_stage_and_exact_source_text() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let reports = [prompt::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns deny Jordan Sample talks".into(),
            publisher_excerpt: "Cleveland Browns denied talks for Jordan Sample.".into(),
            co_mentions: vec![prompt::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        let denied = r#"{"body":"Jordan Sample is not in talks with Cleveland, according to Wire.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"denied","stage":null,"evidence_quote":"Cleveland Browns denied talks for Jordan Sample"}]}"#;
        assert_eq!(
            parser.parse(denied).unwrap().unwrap().findings[0].status,
            Status::Denied
        );
        assert!(parser
            .parse(&denied.replace(r#""stage":null"#, r#""stage":"speculation""#))
            .is_err());
        assert!(parser
            .parse(
                &denied
                    .replace(r#""status":"denied""#, r#""status":"reported""#)
                    .replace(r#""stage":null"#, r#""stage":"concrete_interest""#)
            )
            .is_err());
    }
}
