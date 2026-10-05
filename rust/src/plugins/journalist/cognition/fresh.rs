//! Newly fetched reporting selected by the plugin. This module shapes source data;
//! it does not own meta, memories, voice, form, or context assembly.
use super::CorpusItem;
use crate::plugins::support::source::Reporting;
use serde::Serialize;

pub const VERSION: &str = "journalist-fresh-v8";

#[derive(Serialize)]
pub(super) struct Report<'a> {
    /// A request-local writing slot, not a durable source identity.
    report_key: String,
    #[serde(flatten)]
    reporting: Reporting<'a>,
}

/// Preserve selected order and complete source text. Durable IDs, hashes,
/// classifications, activity scores and publisher headlines stay in provenance.
///
pub(super) fn prepare(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            reporting: Reporting::new(&report.source, report.published_at_epoch, &report.context),
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
