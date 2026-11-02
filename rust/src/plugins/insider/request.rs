//! Daily fresh source preparation, linked mentions and model requests.
use super::character_instructions::INSTRUCTIONS;
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");
use crate::harness::model::GenerateOptions;
use crate::tools::meta::EntityMeta;
use anyhow::Result;

pub const PROMPT_VERSION: &str = "insider-source-v36-paragraphs";
pub const OUTPUT_CONTRACT_VERSION: &str = "insider-reading-findings-v7-paragraphs";
pub const NUM_PREDICT: i32 = 3500;
pub const BODY_MAX_CHARS: usize = 12000;
pub const MAX_FINDINGS: usize = 2;
pub const EVIDENCE_QUOTE_MAX_CHARS: usize = 500;

pub fn options(num_ctx: i32) -> GenerateOptions {
    let mut options = GenerateOptions {
        system: Some(INSTRUCTIONS.system().to_owned()),
        temperature: Some(0.0),
        num_predict: NUM_PREDICT,
        num_ctx: if num_ctx > 0 {
            num_ctx
        } else {
            crate::harness::model::VOICE_NUM_CTX_PACKET
        },
        json_mode: false,
        format_schema: Some(serde_json::json!({
            "type": "object", "additionalProperties": false,
            "required": ["body", "findings"],
            "properties": {
                "body": {"type": "string", "maxLength": BODY_MAX_CHARS},
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
                            "evidence_quote": {"type": "string", "minLength":1, "maxLength":EVIDENCE_QUOTE_MAX_CHARS}
                        }
                    }
                }
            }
        })),
        format_schema_raw: None,
    };
    let items = &mut options.format_schema.as_mut().unwrap()["properties"]["findings"]["items"];
    let mut reported = items.clone();
    reported["properties"]["status"] = serde_json::json!({"type":"string","enum":["reported"]});
    reported["properties"]["stage"] = serde_json::json!({"type":"string","enum":["speculation","concrete_interest","advanced_talks","here_we_go"]});
    let mut denied = items.take();
    denied["properties"]["status"] = serde_json::json!({"type":"string","enum":["denied"]});
    denied["properties"]["stage"] = serde_json::json!({"type":"null"});
    *items = serde_json::json!({"oneOf":[denied,reported]});
    options
}

pub fn options_for_reports(
    num_ctx: i32,
    subject: &EntityMeta,
    reports: &[Report],
) -> GenerateOptions {
    let reports = super::fresh::select(reports);
    let mut options = options(num_ctx);
    // A co-mention is not enough: the quote contract requires both full names in one source span.
    let eligible: Vec<_> = reports
        .iter()
        .enumerate()
        .flat_map(|(index, report)| {
            report.co_mentions.iter().filter_map(move |mention| {
                let valid_type = if subject.entity_type == "team" {
                    matches!(mention.entity_type.as_str(), "player" | "person")
                } else {
                    mention.entity_type == "team"
                };
                let names_in_one_text =
                    [&report.headline, &report.publisher_excerpt]
                        .iter()
                        .any(|text| {
                            crate::tools::fresh::names_entity(text, &subject.name)
                                && crate::tools::fresh::names_entity(text, &mention.name)
                        });
                (valid_type && names_in_one_text).then_some((index, &mention.name))
            })
        })
        .collect();
    let names: std::collections::BTreeSet<_> = eligible.iter().map(|(_, name)| name).collect();
    let indices: std::collections::BTreeSet<_> = eligible.iter().map(|(index, _)| index).collect();
    let findings = &mut options.format_schema.as_mut().unwrap()["properties"]["findings"];
    findings["maxItems"] = serde_json::json!(if names.is_empty() {
        0
    } else {
        MAX_FINDINGS.min(reports.len() * 8)
    });
    for item in findings["items"]["oneOf"].as_array_mut().unwrap() {
        item["properties"]["report_index"]["maximum"] =
            serde_json::json!(reports.len().saturating_sub(1));
        if !names.is_empty() {
            item["properties"]["counterparty"]["enum"] = serde_json::json!(names);
            item["properties"]["report_index"]["enum"] = serde_json::json!(indices);
        }
    }
    options
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    #[serde(default)]
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub reports: Vec<Report>,
    #[serde(default)]
    pub memories: Vec<crate::plugins::classifier::memories::Headline>,
}

impl Parts {
    pub fn assemble(&self) -> String {
        assemble_with_record(&self.subject, &self.reports, self.database_record.as_ref())
    }
    pub fn parse(&self, raw: &str) -> Result<Option<super::Reply>> {
        use crate::harness::Parser;
        super::parser::ReplyParser {
            subject: &self.subject,
            reports: &super::fresh::select(&self.reports),
        }
        .parse(raw)
    }
}

pub use super::fresh::{Mention, Report};

pub fn assemble(subject: &EntityMeta, reports: &[Report]) -> String {
    assemble_with_record(subject, reports, None)
}

pub(super) fn assemble_with_record(
    subject: &EntityMeta,
    reports: &[Report],
    database_record: Option<&crate::tools::meta::DatabaseRecord>,
) -> String {
    crate::tools::prompt::assemble(
        &INSTRUCTIONS,
        crate::tools::meta::context(subject.for_writing(), database_record),
        super::fresh::context(reports),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_source_world_contains_only_the_approved_parts() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let report = Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: Some("2026-10-02".into()),
            headline: "Browns consider Jordan Sample".into(),
            publisher_excerpt: "Cleveland is monitoring Jordan Sample.".into(),
            co_mentions: vec![Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        let world = assemble(&subject, &[report]);
        let fresh_at = world.find(r#""new_content":"#).unwrap();
        assert!(world.starts_with("{\n  \"instructions\":"));
        assert!(fresh_at > world.find(r#""target":"#).unwrap());
        assert!(!world.contains(r#""tone":"#));
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["target"]["sport"], "American football");
        assert_eq!(value["new_content"]["reports"][0]["report_index"], 0);
        assert_eq!(
            value["new_content"]["reports"][0]["co_mentions"][0]["name"],
            "Cleveland Browns"
        );
        assert!(value["target"].get("entity_id").is_none());
        assert!(value.get("memories").is_none());
        assert!(value.get("task").is_none());
        assert_eq!(options(7900).system.as_deref(), Some(INSTRUCTIONS.system()));
        assert!(value.get("form").is_none());
    }

    #[test]
    fn native_findings_are_bounded_to_the_supplied_counterparties_and_reports() {
        let subject = EntityMeta {
            name: "Cedar".into(),
            entity_type: "team".into(),
            entity_id: 1,
            sport: "NFL".into(),
        };
        let mut report = Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Cedar express interest in signing Alex Reed".into(),
            publisher_excerpt: "Cedar reported talks.".into(),
            co_mentions: vec![
                Mention {
                    name: "Alex Reed".into(),
                    entity_type: "player".into(),
                },
                Mention {
                    name: "Other Club".into(),
                    entity_type: "team".into(),
                },
            ],
        };
        let options = options_for_reports(7900, &subject, std::slice::from_ref(&report));
        assert_eq!(options.num_ctx, 7900);
        assert_eq!(options.temperature, Some(0.0));
        let findings = &options.format_schema.as_ref().unwrap()["properties"]["findings"];
        for item in findings["items"]["oneOf"].as_array().unwrap() {
            assert_eq!(
                item["properties"]["counterparty"]["enum"],
                serde_json::json!(["Alex Reed"])
            );
            assert_eq!(item["properties"]["report_index"]["maximum"], 0);
            if item["properties"]["status"]["enum"][0] == "denied" {
                assert_eq!(
                    item["properties"]["stage"],
                    serde_json::json!({"type":"null"})
                );
            } else {
                assert_eq!(item["properties"]["stage"]["type"], "string");
            }
        }
        report.co_mentions.clear();
        assert_eq!(
            options_for_reports(32768, &subject, &[report])
                .format_schema
                .unwrap()["properties"]["findings"]["maxItems"],
            0
        );
    }

    #[test]
    fn quote_eligibility_cannot_borrow_names_between_reports_or_source_fields() {
        let subject = EntityMeta {
            name: "Rowan Vale".into(),
            entity_type: "player".into(),
            entity_id: 1,
            sport: "NFL".into(),
        };
        let mut report = Report {
            classifier_world: None,
            publisher: "Wire".into(),
            published_at: None,
            headline: "Rowan Vale considers a move".into(),
            publisher_excerpt: "Harbor FC discuss signing a player".into(),
            co_mentions: vec![Mention {
                name: "Harbor FC".into(),
                entity_type: "team".into(),
            }],
        };
        let unsupported = report.clone();
        assert_eq!(
            options_for_reports(7900, &subject, &[unsupported.clone()])
                .format_schema
                .unwrap()["properties"]["findings"]["maxItems"],
            0
        );
        report.publisher_excerpt = "Harbor FC deny talks to sign Rowan Vale".into();
        let options = options_for_reports(7900, &subject, &[unsupported, report]);
        let findings = &options.format_schema.unwrap()["properties"]["findings"];
        assert_eq!(findings["maxItems"], 2);
        for branch in findings["items"]["oneOf"].as_array().unwrap() {
            assert_eq!(
                branch["properties"]["report_index"]["enum"],
                serde_json::json!([1])
            );
            assert_eq!(
                branch["properties"]["counterparty"]["enum"],
                serde_json::json!(["Harbor FC"])
            );
        }
    }
}
