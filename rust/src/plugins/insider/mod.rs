//! Insider execution: read verified source packages and publish under the queue claim.
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
use crate::tools::memories::{HistoryItem, SourceRecord};
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
    history: &[HistoryItem],
    source_records: &[SourceRecord],
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    ensure!(!reports.is_empty(), "Insider needs a verified source");
    let world = prompt::assemble(subject, reports, history, source_records);
    ensure!(
        world.len() <= prompt::CONTEXT_BUDGET_BYTES,
        "Insider article package exceeds reading budget"
    );
    let hash = crate::util::hash_components(&world);
    let extracted = studio
        .extract(
            &world,
            &prompt::options(num_ctx),
            &ReplyParser { subject, reports },
            |_| None,
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
            let material = prompt::load_material(&self.pool, item).await?;
            if material.sources.is_empty() {
                let Some(publication) = ClaimPublication::begin(&self.pool, item).await? else {
                    return Ok(PluginOutcome::Superseded);
                };
                publication.commit_final().await?;
                return Ok(PluginOutcome::Committed);
            }
            let backend = self.models.inference(manifest::ROUTE)?;
            return execute_prepared(
                &self.pool,
                backend.as_ref(),
                self.models.voice_num_ctx,
                item,
                material,
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

#[cfg(test)]
pub(crate) async fn execute_with_backend(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
) -> Result<PluginOutcome> {
    let material = prompt::load_material(pool, item).await?;
    execute_prepared(pool, backend, num_ctx, item, material).await
}

async fn execute_prepared(
    pool: &PgPool,
    backend: &dyn Inference,
    num_ctx: i32,
    item: &Item,
    material: prompt::Material,
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
    let generation = create(
        &Studio::new(backend),
        &material.subject,
        &material.reports,
        &material.history,
        &material.source_records,
        num_ctx,
    )
    .await?;
    publish::commit(pool, item, &material, &generation).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::model::{GenerateResult, Inference};
    use crate::harness::{model::GenerateOptions, Parser};
    use async_trait::async_trait;
    use std::{sync::Mutex, time::Duration};

    struct Fake(Mutex<usize>);
    #[async_trait]
    impl Inference for Fake {
        async fn generate(
            &self,
            _: &str,
            _: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            *self.0.lock().unwrap() += 1;
            Ok((GenerateResult {
                response: r#"{"body":"Jordan Sample is linked with Cleveland, according to Wire.","findings":[{"report_index":0,"counterparty":"Cleveland Browns","status":"reported","stage":"concrete_interest","evidence_quote":"Cleveland is monitoring Jordan Sample"}]}"#.into(),
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
            publisher_excerpt: "Cleveland is monitoring Jordan Sample in trade talks.".into(),
            co_mentions: vec![prompt::Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        let fake = Fake(Mutex::new(0));
        let generated = create(&Studio::new(&fake), &subject, &[report], &[], &[], 4096)
            .await
            .unwrap();
        assert_eq!(*fake.0.lock().unwrap(), 1);
        assert_eq!(generated.product.findings.len(), 1);
        assert!(generated.product.body.contains("Wire"));
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
            publisher_excerpt: "Cleveland is monitoring Jordan Sample.".into(),
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
    }
}

#[cfg(test)]
mod delivery_tests;
