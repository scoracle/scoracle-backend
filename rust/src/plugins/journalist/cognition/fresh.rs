//! Newly fetched reporting selected by the plugin. This module shapes source data;
//! it does not own identity, memory, voice, form, or context assembly.
use super::CorpusItem;
use crate::util::utc_timestamp as timestamp;
use serde::Serialize;

pub const VERSION: &str = "journalist-fresh-v7";

#[derive(Serialize)]
pub(super) struct Report<'a> {
    /// A request-local writing slot, not a durable source identity.
    report_key: String,
    publisher: &'a str,
    /// Publication time is not the time of the event described in the excerpt.
    published_at: Option<String>,
    /// One intact evidence unit: a later denial stays with the preceding report.
    publisher_excerpt: &'a str,
}

/// Preserve selected order and complete source text. Durable IDs, hashes,
/// classifications, activity scores and publisher headlines stay in provenance.
pub(super) fn prepare(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            publisher: &report.source,
            published_at: report.published_at_epoch.map(timestamp),
            publisher_excerpt: &report.context,
        })
        .collect()
}

/// The plugin owns which source development labels the report. Prefer the
/// complete opening sentence; fall back to canonical identity rather than ask
/// articulation to select or compress a claim.
pub(super) fn opening(report: &CorpusItem, entity_name: &str) -> String {
    let excerpt = report.context.trim();
    let sentence_end = excerpt
        .char_indices()
        .find(|(_, character)| matches!(character, '.' | '?' | '!'))
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(excerpt.len());
    let opening = excerpt[..sentence_end].trim();
    if !opening.is_empty()
        && opening.chars().count() <= crate::plugins::support::form::HOOK_MAX_CHARS
    {
        opening.to_string()
    } else {
        entity_name.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_times_preserve_leap_days_time_of_day_and_utc() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(-1), "1969-12-31T23:59:59Z");
        assert_eq!(
            timestamp(1709164800 + 13 * 3600 + 61),
            "2024-02-29T13:01:01Z"
        );
        assert_eq!(timestamp(1709251200), "2024-03-01T00:00:00Z");
    }
}
