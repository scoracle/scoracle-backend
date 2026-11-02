//! Shared instructions → target → new_content assembly.
//! Context modules own their data and interpretation rules; adapters own transport/schema.
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct Instructions {
    pub task: &'static str,
    pub rules: &'static str,
}
impl Instructions {
    pub fn job(&self) -> &str {
        self.task.trim()
    }
    pub fn system(&self) -> &str {
        self.rules.trim()
    }
}

/// A context provider supplies the meaning with its values.
pub struct Context<T> {
    pub guide: Guide,
    pub data: T,
}

// Dotted paths traverse every array item; false and zero are supplied values.
fn supplied(value: &Value, path: &str) -> bool {
    if path.is_empty() {
        return match value {
            Value::Null => false,
            Value::Array(items) => items.iter().any(|v| supplied(v, "")),
            Value::Object(fields) => fields.values().any(|v| supplied(v, "")),
            Value::String(text) => !text.trim().is_empty(),
            _ => true,
        };
    }
    if let Value::Array(items) = value {
        return items.iter().any(|item| supplied(item, path));
    }
    let (key, rest) = path.split_once('.').unwrap_or((path, ""));
    value.get(key).is_some_and(|child| supplied(child, rest))
}

/// Relationship guidance is always present; field meanings follow supplied values.
#[derive(Clone, Copy)]
pub struct Guide {
    pub relationship: &'static str,
    pub fields: &'static [(&'static str, &'static str)],
}

impl Guide {
    fn lines(&self, value: &Value) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.relationship.is_empty() {
            lines.push(self.relationship.to_owned());
        }
        lines.extend(self.fields.iter().filter_map(|(path, meaning)| {
            supplied(value, path).then(|| format!("{path}: {meaning}"))
        }));
        lines
    }
}

pub fn assemble(
    instructions: &Instructions,
    target: Context<impl Serialize>,
    new_content: Context<impl Serialize>,
) -> String {
    #[derive(Serialize)]
    struct ReadingInstructions<'a> {
        task: &'a str,
        target: Vec<String>,
        new_content: Vec<String>,
    }
    #[derive(Serialize)]
    struct Payload<'a, T, F> {
        instructions: ReadingInstructions<'a>,
        target: T,
        new_content: F,
    }
    let guide = ReadingInstructions {
        task: instructions.job(),
        target: target
            .guide
            .lines(&serde_json::to_value(&target.data).expect("target serializes")),
        new_content: new_content
            .guide
            .lines(&serde_json::to_value(&new_content.data).expect("content serializes")),
    };
    serde_json::to_string_pretty(&Payload {
        instructions: guide,
        target: target.data,
        new_content: new_content.data,
    })
    .expect("character payload serializes")
}

// Historical receipts retain the previous envelope; validation reads either layout.
pub(crate) fn target(packet: &Value) -> &Value {
    packet.get("target").unwrap_or(&packet["meta"])
}
pub(crate) fn new_content(packet: &Value) -> &Value {
    packet.get("new_content").unwrap_or(&packet["fresh"])
}

/// The decision adapter uses the same context ownership with its native questions protocol.
pub fn decision(
    subject: &str,
    context: Context<Value>,
    mut questions: Value,
) -> anyhow::Result<Value> {
    use anyhow::Context as _;
    let guide = context.guide.lines(&context.data).join("\n");
    for question in questions.as_object_mut().context("questions")?.values_mut() {
        question["instructions"] = serde_json::json!(format!(
            "{} {}",
            guide,
            question["instructions"]
                .as_str()
                .context("instructions")?
                .replace("{subject}", subject)
        ));
    }
    Ok(serde_json::json!({"state":context.data,"questions":questions}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::meta::{self, WritingIdentity};
    use serde_json::json;

    const INSTRUCTIONS: Instructions = Instructions {
        task: "Summarize supplied reporting. Be measured.",
        rules: "Return JSON.",
    };

    #[test]
    fn explicit_fields_preserve_presence_and_prose_without_parsing_punctuation() {
        let value = json!({"reports":[{"score":0,"confirmed":false,"missing":null,
            "empty":[],"blank":"  ","object":{}}, {"nested":{"date":"2026-11-02"}}]});
        let guide = Guide {
            relationship: "Reporting: about the target.\n- Preserve actor ownership.",
            fields: &[
                ("reports.score", "score: zero is a value."),
                ("reports.confirmed", "confirmation."),
                ("reports.missing", "absent."),
                ("reports.empty", "empty."),
                ("reports.blank", "blank."),
                ("reports.object", "object."),
                ("reports.unknown", "unknown."),
                ("reports.nested.date", "publication date."),
            ],
        };
        assert_eq!(guide.lines(&value).join("\n"), "Reporting: about the target.\n- Preserve actor ownership.\nreports.score: score: zero is a value.\nreports.confirmed: confirmation.\nreports.nested.date: publication date.");
        assert!(!supplied(&json!([null, {}, [], " "]), ""));
        assert_eq!(
            Guide {
                relationship: "",
                fields: &[("score", "score.")]
            }
            .lines(&json!({"score":0}))
            .join("\n"),
            "score: score."
        );
    }

    #[test]
    fn assembly_accepts_prepared_context_shapes_and_orders_task_guide_target_evidence() {
        let target = json!({"name":"Cedar","entity_type":"organization","custom_attribute":false});
        let evidence = json!({"reports":[],"count":0});
        let prompt = assemble(
            &INSTRUCTIONS,
            Context {
                guide: Guide {
                    relationship: "The supplied organization is the target.",
                    fields: &[("custom_attribute", "Plugin-specific flag.")],
                },
                data: &target,
            },
            Context {
                guide: Guide {
                    relationship: "Reports about the target.",
                    fields: &[("reports", "supplied reports."), ("count", "report count.")],
                },
                data: &evidence,
            },
        );
        let packet: Value = serde_json::from_str(&prompt).unwrap();
        assert!(prompt.starts_with("{\n  \"instructions\":"));
        assert!(
            prompt.find("\n  \"instructions\":").unwrap() < prompt.find("\n  \"target\":").unwrap()
        );
        assert!(
            prompt.find("\n  \"target\":").unwrap() < prompt.find("\n  \"new_content\":").unwrap()
        );
        assert_eq!(
            packet["instructions"]["task"],
            "Summarize supplied reporting. Be measured."
        );
        assert_eq!(prompt.matches("Be measured.").count(), 1);
        assert!(!INSTRUCTIONS.system().contains(INSTRUCTIONS.task));
        assert_eq!(packet.as_object().unwrap().len(), 3);
        assert_eq!(packet["target"], target);
        assert_eq!(packet["new_content"], evidence);
        assert_eq!(
            packet["instructions"]["target"],
            json!([
                "The supplied organization is the target.",
                "custom_attribute: Plugin-specific flag."
            ])
        );
        assert_eq!(
            packet["instructions"]["new_content"],
            json!(["Reports about the target.", "count: report count."])
        );
    }

    #[test]
    fn metadata_keeps_target_and_stored_relationships_separate() {
        for (kind, attributes) in [
            (
                "team",
                json!({"coach_on_record":{"name":"Alex Reed","observed_at":"2025-10-01"}}),
            ),
            (
                "player",
                json!({"position":"G","affiliation_on_record":{"team":"Harbor"}}),
            ),
            (
                "person",
                json!({"role":"owner","affiliation_on_record":{"team":"Harbor"}}),
            ),
            ("organization", Value::Null),
        ] {
            let record = (!attributes.is_null())
                .then(|| serde_json::from_value(attributes.clone()).unwrap());
            let evidence = json!({"reports":[{"headline":"Another person speaks about Target"}]});
            let packet: Value = serde_json::from_str(&assemble(
                &INSTRUCTIONS,
                meta::context(
                    WritingIdentity::new("Target", kind, "basketball"),
                    record.as_ref(),
                ),
                Context {
                    guide: Guide {
                        relationship:
                            "Reporting about the target; source text identifies its speaker.",
                        fields: &[],
                    },
                    data: &evidence,
                },
            ))
            .unwrap();
            assert_eq!(packet["target"]["entity_type"], kind);
            if record.is_some() {
                assert_eq!(packet["target"]["attributes"], attributes);
            } else {
                assert!(packet["target"].get("attributes").is_none());
            }
            assert_eq!(packet["new_content"], evidence);
            assert!(packet["target"].get("target").is_none());
        }
    }
}
