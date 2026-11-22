//! Insider execution: read verified source packages and publish under the queue claim.
pub const TASK: &str = crate::harness::skill::instructions(include_str!("../SKILL.md"));
pub mod fresh;
pub mod manifest;
mod parser;
mod publish;
pub mod request;

use crate::harness::model::Inference;
use crate::harness::models::ExecutionCapabilities;
use crate::harness::queue::publication::ClaimPublication;
use crate::harness::queue::work::Item;
use crate::harness::skill::{Skill, SkillManifest, SkillOutcome};
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
    #[serde(skip_deserializing)]
    pub card_score: i16,
    pub body: String,
    pub headline: String,
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
    reports: &[request::Report],
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    create_with_record(studio, subject, reports, None, num_ctx).await
}

async fn create_with_record(
    studio: &Studio<'_>,
    subject: &EntityMeta,
    reports: &[request::Report],
    database_record: Option<&crate::tools::meta::DatabaseRecord>,
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    let selected = fresh::select(reports);
    let reports = selected.as_slice();
    ensure!(!reports.is_empty(), "Insider needs a verified source");
    let world = request::assemble_with_record(subject, reports, database_record);
    let hash = crate::util::hash_components(&serde_json::to_string(&(
        &world,
        crate::tools::source::ACTIVITY_SCORING_VERSION,
        TASK,
        request::PROMPT_VERSION,
    ))?);
    let extracted = studio
        .extract(
            &world,
            &request::options_for_reports(num_ctx, subject, reports),
            &ReplyParser { subject, reports },
            crate::harness::session::no_rewrite,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let value = extracted
        .value
        .ok_or_else(|| anyhow::anyhow!("Insider returned no reply"))?;
    Ok(Generation::called(
        value,
        extracted.model,
        request::PROMPT_VERSION,
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
impl Skill for TransferHandler {
    fn manifest(&self) -> &'static SkillManifest {
        &crate::harness::skills::insider::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<SkillOutcome> {
        if item.input_version.as_deref().is_some_and(|version| {
            version.starts_with(crate::harness::skills::classifier::CONTRACT)
        }) {
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
            return Ok(SkillOutcome::Superseded);
        };
        publication.commit_final().await?;
        Ok(SkillOutcome::Committed)
    }
}

pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
) -> Result<SkillOutcome> {
    let material = fresh::load_material(pool, item).await?;
    execute_prepared(pool, backend, num_ctx, item, material).await
}

async fn execute_prepared(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
    material: fresh::Material,
) -> Result<SkillOutcome> {
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
        return Ok(SkillOutcome::Superseded);
    }
    if !material.held.is_empty() || material.sources.is_empty() {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(SkillOutcome::Superseded);
        };
        let held: Vec<_> = material
            .held
            .iter()
            .map(|(source, _)| source.clone())
            .collect();
        crate::harness::skills::classifier::delivery::validate_for_publication(
            publication.transaction(),
            manifest::MANIFEST.id.as_str(),
            &item.entity_type,
            item.entity_id_i32()?,
            &material.subject.sport,
            &held,
        )
        .await?;
        for (source, reason) in &material.held {
            crate::harness::skills::classifier::delivery::settle(
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
            return Ok(SkillOutcome::Committed);
        }
        publication.commit_progress().await?;
    }
    let result = create_with_record(
        &Studio::new(backend),
        &material.subject,
        &material.reports,
        material.database_record.as_ref(),
        num_ctx,
    )
    .await;
    let generation = match result {
        Ok(generation) => generation,
        Err(error) => {
            let (reason, receipt) = if let Some(rejected) =
                error.downcast_ref::<crate::harness::model::RejectedReply>()
            {
                (
                    "model_reply_rejected",
                    serde_json::json!({"model":backend.model(),"prompt_version":request::PROMPT_VERSION,
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
                return Ok(SkillOutcome::Superseded);
            };
            crate::harness::skills::classifier::delivery::validate_for_publication(
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
            crate::harness::skills::classifier::delivery::settle(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &ids,
                "held",
                Some(reason),
                serde_json::json!({"rejected_attempt":receipt}),
            )
            .await?;
            let remaining = crate::harness::skills::classifier::delivery::undelivered_count(
                publication.transaction(),
                manifest::MANIFEST.id.as_str(),
                &item.entity_type,
                item.entity_id_i32()?,
                &material.subject.sport,
            )
            .await?;
            if remaining > 0 {
                publication.commit_progress().await?;
                return Ok(SkillOutcome::deferred(
                    "rejected Insider article held; more reports remain",
                    std::time::Duration::from_secs(1),
                ));
            }
            publication.commit_final().await?;
            return Ok(SkillOutcome::Committed);
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

    struct Fake(Mutex<usize>, Option<&'static str>);
    #[async_trait]
    impl Inference for Fake {
        async fn generate(
            &self,
            _: &str,
            _: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let mut calls = self.0.lock().unwrap();
            *calls += 1;
            let first_body = self.1.filter(|_| *calls == 1);
            drop(calls);
            Ok((
                GenerateResult {
                    response: if let Some(body) = first_body {
                        serde_json::json!({"body":body,"findings":[],"headline":"The personnel picture"}).to_string()
                    } else {
                        r#"{"body":"Jordan Sample is linked with Cleveland, according to Wire.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"reported","stage":"concrete_interest","evidence_quote":"Cleveland Browns is monitoring Jordan Sample in trade talks"}],"headline":"Cleveland keeps tabs on Sample"}"#.into()
                    },
                    thinking: String::new(),
                    model: "fake".into(),
                    total_duration: Duration::from_millis(1),
                    prompt_eval_count: 10,
                    eval_count: 10,
                    completion_reason: Some("stop".into()),
                    raw_response_body: "{}".into(),
                },
                serde_json::json!({}),
            ))
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({})
        }
    }
    include!("../../../tests/internal/insider/mod.rs");

    #[tokio::test]
    async fn one_call_returns_reading_and_exact_source_finding() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let report = request::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt:
                "Cleveland Browns is monitoring Jordan Sample in trade talks in trade talks.".into(),
            co_mentions: vec![request::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        for (first_body, calls) in [
            (None, 1),
            (Some("Jordan Sample"), 1),
            (
                Some("Jordan Sample is linked with Cleveland in Week 7, according to Wire."),
                1,
            ),
        ] {
            let fake = Fake(Mutex::new(0), first_body);
            let generated = create(
                &Studio::new(&fake),
                &subject,
                std::slice::from_ref(&report),
                4096,
            )
            .await;
            assert_eq!(*fake.0.lock().unwrap(), calls);
            if let Some(body) = first_body {
                assert_eq!(generated.unwrap().product.body, body);
                continue;
            }
            let generated = generated.unwrap();
            assert_eq!(generated.product.findings.len(), 1);
            assert_eq!(generated.product.card_score, 10);
            assert_eq!(generated.product.headline, "Cleveland keeps tabs on Sample");
            assert!(generated.product.body.contains("Wire"));
            let final_world: serde_json::Value =
                serde_json::from_str(&generated.call.unwrap().built_prompt).unwrap();
            let original: serde_json::Value =
                serde_json::from_str(&request::assemble(&subject, std::slice::from_ref(&report)))
                    .unwrap();
            assert_eq!(final_world["evidence"], original["evidence"]);
        }
    }

    #[test]
    fn unlinked_quote_does_not_hold_the_briefing() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let reports = [request::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt: "Cleveland Browns is monitoring Jordan Sample in trade talks."
                .into(),
            co_mentions: vec![request::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        assert!(parser.parse(r#"{"body":"Jordan Sample is linked with Cleveland.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"reported","stage":"concrete_interest","evidence_quote":"Cleveland has signed Jordan Sample"}],"headline":"The recorded performance"}"#).unwrap().unwrap().findings.is_empty());
    }

    #[test]
    fn explicit_denial_requires_null_stage_and_exact_source_text() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let reports = [request::Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns deny Jordan Sample talks".into(),
            publisher_excerpt: "Cleveland Browns denied talks for Jordan Sample.".into(),
            co_mentions: vec![request::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        }];
        let parser = ReplyParser {
            subject: &subject,
            reports: &reports,
        };
        let denied = r#"{"body":"Jordan Sample is not in talks with Cleveland, according to Wire.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"denied","stage":null,"evidence_quote":"Cleveland Browns denied talks for Jordan Sample"}],"headline":"The recorded performance"}"#;
        assert_eq!(
            parser.parse(denied).unwrap().unwrap().findings[0].status,
            Status::Denied
        );
        assert!(parser
            .parse(&denied.replace(r#""stage":null"#, r#""stage":"speculation""#))
            .unwrap()
            .unwrap()
            .findings
            .is_empty());
        assert!(
            parser
                .parse(
                    &denied
                        .replace(r#""status":"denied""#, r#""status":"reported""#)
                        .replace(r#""stage":null"#, r#""stage":"concrete_interest""#)
                )
                .unwrap()
                .unwrap()
                .findings
                .len()
                == 1
        );
    }
}
