//! Newly fetched reporting selected by the plugin. This module shapes source data;
//! it does not own identity, memory, voice, form, or context assembly.
use super::CorpusItem;
use crate::util::utc_timestamp as timestamp;
use serde::Serialize;

pub const VERSION: &str = "journalist-fresh-v4";

#[derive(Serialize)]
pub(super) struct Report<'a> {
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
        .map(|report| Report {
            publisher: &report.source,
            published_at: report.published_at_epoch.map(timestamp),
            publisher_excerpt: &report.context,
        })
        .collect()
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
