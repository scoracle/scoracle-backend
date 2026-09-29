//! Newly fetched reporting selected by the plugin. This module shapes source data;
//! it does not own identity, memory, voice, form, or context assembly.
use super::CorpusItem;
use crate::plugins::support::source::Reporting;
use serde::Serialize;

pub const VERSION: &str = "journalist-fresh-v7";

/// The studied history the plugin attached to one report.
///
/// This nests under the report rather than sitting beside it. Presenting history
/// as a parallel array would hand the model a pairing decision this plugin has
/// already made.
#[derive(Clone, Serialize)]
pub(super) struct History<'a> {
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub history: &'a [crate::plugins::memories::HistoryItem],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub history_groups: &'a [crate::plugins::memories::GroupSummary],
}

impl Default for History<'_> {
    fn default() -> Self {
        Self {
            history: &[],
            history_groups: &[],
        }
    }
}

#[derive(Serialize)]
pub(super) struct Report<'a> {
    /// A request-local writing slot, not a durable source identity.
    report_key: String,
    #[serde(flatten)]
    reporting: Reporting<'a>,
    #[serde(flatten)]
    history: History<'a>,
}

/// Preserve selected order and complete source text. Durable IDs, hashes,
/// classifications, activity scores and publisher headlines stay in provenance.
///
/// `attached` is index-aligned with `reports`; a report with no usable history
/// carries no history keys at all rather than an empty array, so "no history was
/// attached" and "history was attached and was empty" stay distinguishable.
pub(super) fn prepare<'a>(reports: &'a [CorpusItem], attached: &[History<'a>]) -> Vec<Report<'a>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            reporting: Reporting::new(&report.source, report.published_at_epoch, &report.context),
            history: attached.get(index).cloned().unwrap_or_default(),
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
