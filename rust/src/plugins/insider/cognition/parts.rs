//! The one model-facing Insider world, shared by production and replay.

use crate::plugins::memories::{HistoryItem, SourceRecord};
use crate::plugins::meta::EntityMeta;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Mention {
    pub name: String,
    pub entity_type: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub publisher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub headline: String,
    pub publisher_excerpt: String,
    pub co_mentions: Vec<Mention>,
}

pub fn assemble(
    subject: &EntityMeta,
    reports: &[Report],
    history: &[HistoryItem],
    source_records: &[SourceRecord],
) -> String {
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::plugins::meta::WritingIdentity<'a>,
        fresh: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        memories: Option<serde_json::Value>,
        voice: &'static str,
        form: serde_json::Value,
    }
    let memories = (!history.is_empty() || !source_records.is_empty()).then(|| {
        serde_json::json!({
            "prior_reports": history,
            "source_records": source_records,
        })
    });
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: serde_json::json!({ "reports": reports }),
        memories,
        voice: crate::plugins::insider::voice::VOICE,
        form: serde_json::json!({
            "keys": ["body", "findings"],
            "max_chars": crate::plugins::support::form::BODY_MAX_CHARS,
            "paragraph_max_chars": null,
            "findings": {
                "type": "array",
                "item_keys": ["report_index", "counterparty", "status", "stage", "evidence_quote"]
            }
        }),
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
            publisher: "Wire".into(),
            published_at: Some("2026-10-02".into()),
            headline: "Browns consider Jordan Sample".into(),
            publisher_excerpt: "Cleveland is monitoring Jordan Sample.".into(),
            co_mentions: vec![Mention {
                name: "Cleveland Browns".into(),
                entity_type: "team".into(),
            }],
        };
        let world = assemble(&subject, &[report], &[], &[]);
        let fresh_at = world.find(r#""fresh":"#).unwrap();
        let voice_at = world.find(r#""voice":"#).unwrap();
        let form_at = world.find(r#""form":"#).unwrap();
        assert!(world.starts_with(r#"{"meta":"#) && fresh_at < voice_at && voice_at < form_at);
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["meta"]["sport"], "American football");
        assert_eq!(
            value["fresh"]["reports"][0]["co_mentions"][0]["name"],
            "Cleveland Browns"
        );
        assert!(value["meta"].get("entity_id").is_none());
        assert!(value.get("memories").is_none());
        assert_eq!(
            value["form"]["keys"],
            serde_json::json!(["body", "findings"])
        );
        assert_eq!(
            value["form"]["findings"]["item_keys"],
            serde_json::json!([
                "report_index",
                "counterparty",
                "status",
                "stage",
                "evidence_quote"
            ])
        );
    }

    #[test]
    fn measured_source_record_keeps_its_sample_size() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let world = assemble(
            &subject,
            &[],
            &[],
            &[SourceRecord {
                publisher: "Wire".into(),
                confirmed: 2,
                tracked: 3,
                reliability: 25,
            }],
        );
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert_eq!(value["memories"]["source_records"][0]["tracked"], 3);
        assert_eq!(value["memories"]["source_records"][0]["reliability"], 25);
    }
}
