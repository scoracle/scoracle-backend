//! Task, field guide, then target identity and evidence in one replayable request.
use super::meta::{DatabaseRecord, WritingIdentity};
use serde::Serialize;

pub fn assemble(
    instructions: &str,
    fresh_guide: &str,
    identity: WritingIdentity<'_>,
    database_record: Option<&DatabaseRecord>,
    fresh: impl Serialize,
    tone: &str,
) -> String {
    #[derive(Serialize)]
    struct Meta<'a> {
        #[serde(flatten)]
        identity: WritingIdentity<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attributes: Option<&'a std::collections::BTreeMap<String, serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        database_record: Option<&'a str>,
    }
    let (attributes, legacy) = match database_record {
        Some(DatabaseRecord::Attributes(fields)) => (Some(fields), None),
        Some(DatabaseRecord::Legacy(text)) => (None, Some(text.as_str())),
        None => (None, None),
    };
    let meta = Meta {
        identity,
        attributes,
        database_record: legacy,
    };
    // The opening job comes from the authoritative instructions, never a second task definition.
    let job = instructions.trim().split("\n\n").next().unwrap_or_default();
    #[derive(Serialize)]
    struct Guide<'a> {
        meta: &'a str,
        fresh: &'a str,
        tone: &'a str,
    }
    let guide = Guide {
        meta: include_str!("context.txt").trim(),
        fresh: fresh_guide.trim(),
        tone: "tone controls the expression of your answer. It supplies no facts, feelings or certainty about the target.",
    };
    format!(
        "{{\"job\":{},\n\"context_guide\":{},\n\"meta\":{},\n\"fresh\":{},\n\"tone\":{}}}",
        serde_json::to_string(job).expect("job serializes"),
        serde_json::to_string(&guide).expect("context guide serializes"),
        serde_json::to_string(&meta).expect("target serializes"),
        serde_json::to_string(&fresh).expect("fresh serializes"),
        serde_json::to_string(tone).expect("tone serializes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_target_and_fresh_are_ordered_without_inventing_missing_attributes() {
        let render = || {
            assemble(
                "Summarize the supplied reporting.\n\nReturn JSON.",
                "fresh.reports contains the supplied reporting.",
                WritingIdentity::new("Cedar", "team", "football"),
                None,
                serde_json::json!({"reports": []}),
                "Steady.",
            )
        };
        let prompt = render();
        assert!(prompt.starts_with("{\"job\":"));
        assert!(prompt.find("\n\"meta\":").unwrap() < prompt.find("\n\"fresh\":").unwrap());
        let packet: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(packet["job"], "Summarize the supplied reporting.");
        assert!(prompt.find("\"context_guide\":").unwrap() < prompt.find("\"meta\":{").unwrap());
        assert_eq!(packet["meta"]["name"], "Cedar");
        assert!(packet["meta"].get("target").is_none());
        assert!(packet["meta"].get("attributes").is_none());
        assert!(packet.get("history").is_none());
        assert_eq!(prompt, render());
    }

    #[test]
    fn the_same_guide_preserves_different_target_types_and_relationships() {
        for (kind, attributes) in [
            (
                "team",
                serde_json::json!({"coach_on_record":{"name":"Alex Reed","observed_at":"2025-10-01"}}),
            ),
            (
                "player",
                serde_json::json!({"position":"G","affiliation_on_record":{"team":"Harbor"}}),
            ),
            (
                "person",
                serde_json::json!({"role":"owner","affiliation_on_record":{"team":"Harbor"}}),
            ),
        ] {
            let record = serde_json::from_value(attributes.clone()).unwrap();
            let evidence =
                serde_json::json!({"reports":[{"headline":"Another person speaks about Target"}]});
            let request: serde_json::Value = serde_json::from_str(&assemble(
                "Summarize the reporting.",
                "fresh.reports contains reporting.",
                WritingIdentity::new("Target", kind, "basketball"),
                Some(&record),
                &evidence,
                "Steady.",
            ))
            .unwrap();
            assert_eq!(request["meta"]["entity_type"], kind);
            assert_eq!(request["meta"]["attributes"], attributes);
            assert_eq!(request["fresh"], evidence);
            assert_eq!(
                request["context_guide"]["meta"],
                include_str!("context.txt").trim()
            );
            assert!(request["meta"].get("target").is_none());
        }
    }
}
