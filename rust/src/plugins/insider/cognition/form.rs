//! One attributed reading and its source-linked move findings per entity.
use super::{parts, prompt};
use crate::plugins::memories::{HistoryItem, SourceRecord};
use crate::plugins::meta::EntityMeta;
use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const PROMPT_VERSION: &str = "insider-source-v3";
pub const OUTPUT_CONTRACT_VERSION: &str = "insider-reading-findings-v2";
pub const NUM_PREDICT: i32 = 1400;

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

pub fn options(num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(prompt::SYSTEM.into()),
        temperature: Some(0.3),
        num_predict: NUM_PREDICT,
        num_ctx,
        json_mode: false,
        format_schema: Some(serde_json::json!({
            "type": "object", "additionalProperties": false,
            "required": ["body", "findings"],
            "properties": {
                "body": {"type": "string"},
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object", "additionalProperties": false,
                        "required": ["report_index", "counterparty", "status", "stage", "evidence_quote"],
                        "properties": {
                            "report_index": {"type": "integer", "minimum": 0},
                            "counterparty": {"type": "string"},
                            "status": {"type": "string", "enum": ["reported", "denied"]},
                            "stage": {"type": ["string", "null"], "enum": ["speculation", "concrete_interest", "advanced_talks", "here_we_go", null]},
                            "evidence_quote": {"type": "string"}
                        }
                    }
                }
            }
        })),
        format_schema_raw: None,
    }
}

struct ReplyParser<'a> {
    subject: &'a EntityMeta,
    reports: &'a [parts::Report],
}

impl Parser<Reply> for ReplyParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<Reply>> {
        let mut reply: Reply = serde_json::from_str(raw)?;
        reply.body = crate::plugins::support::guards::clean_served_prose(
            &crate::plugins::support::form::normalize_body(&reply.body),
        );
        crate::plugins::support::form::validate_body(&reply.body)?;
        ensure!(
            crate::plugins::support::guards::title_names_entity(&reply.body, &self.subject.name),
            "Insider reading does not name the subject"
        );
        ensure!(
            !crate::plugins::support::guards::has_bookkeeping_citation(&reply.body)
                && crate::plugins::support::guards::first_product_name(&reply.body).is_none()
                && !crate::plugins::support::guards::has_foreign_script(&reply.body),
            "Insider reading violates served prose guard"
        );
        ensure!(
            reply.findings.len() <= self.reports.len() * 8,
            "too many findings"
        );
        let mut seen = std::collections::HashSet::new();
        for finding in &reply.findings {
            let report = self
                .reports
                .get(finding.report_index)
                .ok_or_else(|| anyhow::anyhow!("finding cites a missing report"))?;
            ensure!(
                report
                    .co_mentions
                    .iter()
                    .any(|mention| mention.name == finding.counterparty),
                "finding counterparty was not co-mentioned in its report"
            );
            ensure!(
                match finding.status {
                    Status::Reported => matches!(
                        finding.stage.as_deref(),
                        Some("speculation" | "concrete_interest" | "advanced_talks" | "here_we_go")
                    ),
                    Status::Denied => finding.stage.is_none(),
                },
                "finding stage does not match its status"
            );
            ensure!(
                !finding.evidence_quote.trim().is_empty()
                    && finding.evidence_quote.chars().count() <= 500
                    && (report.headline.contains(&finding.evidence_quote)
                        || report.publisher_excerpt.contains(&finding.evidence_quote)),
                "finding quote is not an exact publisher span"
            );
            ensure!(
                seen.insert((finding.report_index, finding.counterparty.as_str())),
                "duplicate source/counterparty finding"
            );
        }
        Ok(Some(reply))
    }
}

pub async fn create(
    studio: &Studio<'_>,
    subject: &EntityMeta,
    reports: &[parts::Report],
    history: &[HistoryItem],
    source_records: &[SourceRecord],
    num_ctx: i32,
) -> Result<Generation<Reply>> {
    ensure!(!reports.is_empty(), "Insider needs a verified source");
    let world = parts::assemble(subject, reports, history, source_records);
    let hash = crate::util::hash_components(&world);
    let extracted = studio
        .extract(
            &world,
            &options(num_ctx),
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
        PROMPT_VERSION,
        Vec::new(),
        Some(hash),
        call,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::model::{GenerateResult, Inference};
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
        let report = parts::Report {
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt: "Cleveland is monitoring Jordan Sample in trade talks.".into(),
            co_mentions: vec![parts::Mention {
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
        let reports = [parts::Report {
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns pursue Jordan Sample".into(),
            publisher_excerpt: "Cleveland is monitoring Jordan Sample.".into(),
            co_mentions: vec![parts::Mention {
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
        let reports = [parts::Report {
            publisher: "Wire".into(),
            published_at: None,
            headline: "Browns deny Jordan Sample talks".into(),
            publisher_excerpt: "Cleveland Browns denied talks for Jordan Sample.".into(),
            co_mentions: vec![parts::Mention {
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
