//! Complete publisher evidence shared by character prompt assemblers.
use super::source::SourceContext;
use serde::Serialize;

#[derive(Serialize)]
pub struct Report<'a> {
    headline: String,
    publisher: &'a str,
    published_at: Option<String>,
    publisher_text: String,
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
            publisher_text: super::fetch::decode_entities(&s.context),
            classifier_world: s.classifier_world.as_ref(),
        })
        .collect()
}
