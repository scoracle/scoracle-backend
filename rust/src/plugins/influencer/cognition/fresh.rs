//! Fresh source presentation. Durable IDs and classification stay in provenance.
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::support::source::Reporting;

pub(super) fn prepare(source: &SourceContext) -> Reporting<'_> {
    Reporting::new(&source.source, source.published_at_epoch, &source.context)
}

/// Source-owned title; articulation cannot invent a headline claim.
pub(super) fn title(source: &SourceContext, entity_name: &str) -> String {
    let headline = source.headline.trim();
    if !headline.is_empty()
        && headline.chars().count() <= crate::plugins::support::form::HOOK_MAX_CHARS
    {
        headline.to_string()
    } else {
        entity_name.to_string()
    }
}
