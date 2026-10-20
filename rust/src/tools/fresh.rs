//! Present fresh headlines and historical publisher evidence in character packages.
use super::source::SourceContext;
use serde::Serialize;

/// URLs and routing scores do not make a second copy of publisher wording new evidence.
pub fn evidence_key(publisher: &str, text: &str) -> (String, String) {
    (
        publisher.trim().to_lowercase(),
        super::fetch::decode_entities(text).trim().to_owned(),
    )
}

#[derive(Serialize)]
pub struct Report<'a> {
    headline: String,
    publisher: &'a str,
    published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classifier_world: Option<serde_json::Value>,
}

pub fn reports(sources: &[SourceContext]) -> Vec<Report<'_>> {
    sources
        .iter()
        .map(|s| Report {
            headline: super::fetch::decode_entities(&s.headline),
            publisher: &s.source,
            published_at: s.published_at_epoch.map(crate::util::utc_timestamp),
            publisher_text: (s
                .classifier_world
                .as_ref()
                .is_none_or(|w| w["evidence_scope"] != "headline"))
            .then(|| super::fetch::decode_entities(&s.context)),
            classifier_world: evidence_world(s.classifier_world.as_ref()),
        })
        .collect()
}

/// Keep evidence scope and qualifications in prose inputs; routing metadata stays in receipts.
pub fn evidence_world(world: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    world.cloned().map(|mut world| {
        if let Some(fields) = world.as_object_mut() {
            for key in ["url", "gate", "spectrum"] {
                fields.remove(key);
            }
        }
        world
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn writing_metadata_keeps_qualifications_without_routing_payload() {
        let world = json!({"url":"https://example.invalid", "gate":{"admitted":true}, "spectrum":{"emotion":99},
            "evidence_scope":"headline", "target_identity_resolved":false,
            "qualified_claims":[{"negation":"not confirmed"}]});
        let result = evidence_world(Some(&world)).unwrap();
        assert_eq!(
            result,
            json!({"evidence_scope":"headline", "target_identity_resolved":false,
            "qualified_claims":[{"negation":"not confirmed"}]})
        );
        assert!(world.get("url").is_some());
        assert!(evidence_world(None).is_none());
    }
}

/// Compare complete words, rather than treating a shared surname or substring as identity.
pub fn names_entity(text: &str, name: &str) -> bool {
    let words = |s: &str| {
        crate::tools::guards::fold_for_match(s)
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .map(|word| word.trim_matches('\''))
            .map(|word| word.strip_suffix("'s").unwrap_or(word))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let name = words(name);
    !name.is_empty() && format!(" {} ", words(text)).contains(&format!(" {name} "))
}

/// A numerical fact cannot acquire a different named person by pooling report citations.
pub fn validate_named_facts(text: &str, sources: &[&str]) -> anyhow::Result<()> {
    let numbers = crate::tools::guards::numeric_literals(text);
    let mut names = Vec::new();
    let mut run = Vec::new();
    for word in text.split_whitespace().chain(std::iter::once("")) {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
        if cleaned.chars().next().is_some_and(char::is_uppercase)
            && !matches!(
                cleaned,
                "The"
                    | "A"
                    | "An"
                    | "For"
                    | "In"
                    | "On"
                    | "At"
                    | "After"
                    | "Before"
                    | "According"
                    | "Meanwhile"
                    | "However"
                    | "Reports"
                    | "Reportedly"
                    | "Additionally"
                    | "Multiple"
                    | "During"
                    | "Despite"
                    | "With"
                    | "Without"
            )
        {
            run.push(cleaned);
        } else {
            if run.len() >= 2 {
                names.push(run.join(" "));
            }
            run.clear();
        }
        if word.ends_with([',', '.', ';', ':', '!', '?']) {
            if run.len() >= 2 {
                names.push(run.join(" "));
            }
            run.clear();
        }
    }
    for name in names {
        let evidence: Vec<_> = sources
            .iter()
            .filter(|source| names_entity(source, &name))
            .collect();
        anyhow::ensure!(
            !evidence.is_empty(),
            "named claim is absent from cited reports: {name}"
        );
        for (_, number) in &numbers {
            anyhow::ensure!(
                evidence
                    .iter()
                    .any(|source| crate::tools::guards::numeric_literals(source)
                        .iter()
                        .any(|(_, known)| (known - number).abs() < 0.000_001)),
                "cited reports do not connect {name} with numerical fact {number}"
            );
        }
    }
    anyhow::ensure!(
        numbers
            .iter()
            .all(|(_, number)| sources.iter().any(|source| {
                crate::tools::guards::numeric_literals(source)
                    .iter()
                    .any(|(_, known)| (known - number).abs() < 0.000_001)
            })),
        "numerical claim is absent from cited reports"
    );
    Ok(())
}
