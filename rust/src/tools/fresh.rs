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
    classifier_world: Option<&'a serde_json::Value>,
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
            classifier_world: s.classifier_world.as_ref(),
        })
        .collect()
}
