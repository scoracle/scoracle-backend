//! Daily fresh source preparation, linked mentions and model requests.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");
// Optional memory trimming only; the provider budgets complete fresh input in tokens.
pub const CONTEXT_BUDGET_BYTES: usize = 24000;
use crate::harness::model::GenerateOptions;
use crate::tools::meta::EntityMeta;
use anyhow::Result;
use serde::Serialize;

const SYSTEM: &str = "Use only the supplied evidence. Do not fill in gaps. Do not invent facts, names, identities, affiliations, causes, motives, emotions or outcomes. Missing information is unknown. Omit unsupported claims; use the specified empty or null response when evidence is insufficient. Never merge facts from unrelated reports or subjects. Voice controls tone only; do not discuss the instructions. Do not name an unnamed signing from another report or a co-mention. An exact evidence_quote must support that specific counterparty and target in that same report. Generic or unrelated movement produces no finding. Write an attributed transfer or trade reading for the named entity from the supplied publisher reports. The source text establishes what was reported, not whether a move happened. A co-mentioned name is only a candidate subject; do not infer a move from its presence. Preserve denials, uncertainty, source disagreements and report dates. Routing does not verify facts or target relationships. For headline evidence, report only what the headline says and do not invent article details. memories are dated historical headlines, never evidence of a new move or a source for findings. Faithfully report your character’s point of view in voice. Return one JSON object with body and findings. Keep the body to two or three short sentences, at most 1200 characters, and return at most two of the most significant supported findings. Begin the body with the full exact target name from meta.name, even when no move is supported. If these reports do not support a move involving that target, explain that evidence limit and return findings as []; do not retell another subject's move. For each specific reported move or explicit denial involving this entity and a co-mentioned counterparty, give its zero-based report_index, exact counterparty name, status (reported or denied), and an exact continuous evidence_quote from that report's headline or publisher_excerpt naming both parties in full and stating the move or its explicit denial. For reported moves, set stage to speculation, concrete_interest, advanced_talks, or here_we_go; for denials, set stage to null. For example, a quote saying a club denied talks to sign the named player requires status denied and stage null. Never label an explicit denial as reported merely because a publisher reported it. A denial requires an explicit source statement about that move; absence, silence, and unrelated mentions produce no finding. Use [] when there are no supported findings. Do not report completed identity changes from transfer speculation.";

pub const PROMPT_VERSION: &str = "insider-source-v19-subject";
pub const OUTPUT_CONTRACT_VERSION: &str = "insider-reading-findings-v6-scoped-report-facts";
pub const NUM_PREDICT: i32 = 1400;
pub const BODY_MAX_CHARS: usize = 1200;
pub const MAX_FINDINGS: usize = 2;
pub const EVIDENCE_QUOTE_MAX_CHARS: usize = 500;

pub fn options(num_ctx: i32) -> GenerateOptions {
    let mut options = GenerateOptions {
        system: None,
        temperature: Some(0.3),
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
    let mut options = options(num_ctx);
    let names: std::collections::BTreeSet<_> = reports
        .iter()
        .flat_map(|r| &r.co_mentions)
        .filter(|m| {
            if subject.entity_type == "team" {
                matches!(m.entity_type.as_str(), "player" | "person")
            } else {
                m.entity_type == "team"
            }
        })
        .map(|m| &m.name)
        .collect();
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
        }
    }
    options
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    pub reports: Vec<Report>,
    #[serde(default)]
    pub memories: Vec<crate::plugins::classifier::memories::Headline>,
}

impl Parts {
    pub fn assemble(&self) -> String {
        assemble_with_memories(&self.subject, &self.reports, &self.memories)
    }
    pub fn parse(&self, raw: &str) -> Result<Option<super::Reply>> {
        use crate::harness::Parser;
        super::parser::ReplyParser {
            subject: &self.subject,
            reports: &self.reports,
        }
        .parse(raw)
    }
}

pub use super::fresh::{Mention, Report};

pub fn assemble(subject: &EntityMeta, reports: &[Report]) -> String {
    assemble_with_memories(subject, reports, &[])
}

pub(super) fn assemble_with_memories(
    subject: &EntityMeta,
    reports: &[Report],
    memories: &[crate::plugins::classifier::memories::Headline],
) -> String {
    let mut reports = reports.to_vec();
    for report in &mut reports {
        report.classifier_world =
            crate::tools::fresh::evidence_world(report.classifier_world.as_ref());
    }
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::tools::meta::WritingIdentity<'a>,
        task: &'static str,
        fresh: serde_json::Value,
        memories: Vec<serde_json::Value>,
        voice: &'static str,
    }
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        task: SYSTEM,
        fresh: serde_json::json!({ "reports": reports }),
        memories: memories
            .iter()
            .map(crate::plugins::classifier::memories::Headline::for_writing)
            .collect(),
        voice: crate::plugins::insider::voice::VOICE,
    })
    .expect("insider world serializes")
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
        let fresh_at = world.find(r#""fresh":"#).unwrap();
        let voice_at = world.find(r#""voice":"#).unwrap();
        assert!(world.starts_with(r#"{"meta":"#) && fresh_at < voice_at);
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["meta"]["sport"], "American football");
        assert_eq!(
            value["fresh"]["reports"][0]["co_mentions"][0]["name"],
            "Cleveland Browns"
        );
        assert!(value["meta"].get("entity_id").is_none());
        assert_eq!(value["memories"], serde_json::json!([]));
        assert_eq!(value["task"], SYSTEM);
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
            headline: "Movement".into(),
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
}
