//! Complete fresh reporting for the daily package.
use crate::tools::source::{Reporting, SourceContext};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorpusItem {
    pub id: i64,
    pub title: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
}
impl From<&SourceContext> for CorpusItem {
    fn from(s: &SourceContext) -> Self {
        Self {
            classifier_world: s.classifier_world.clone(),
            id: s.article_id,
            title: s.headline.clone(),
            context: s.context.clone(),
            source: s.source.clone(),
            published_at_epoch: s.published_at_epoch,
        }
    }
}
#[derive(Serialize)]
pub(super) struct Report<'a> {
    /// A request-local writing slot, not a durable source identity.
    report_key: String,
    #[serde(flatten)]
    reporting: Reporting<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classifier_world: Option<&'a serde_json::Value>,
}

/// Preserve selected order and complete source text. Durable IDs, hashes,
/// classifications, activity scores and publisher headlines stay in provenance.
///
pub(super) fn reports(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            reporting: Reporting::new(&report.source, report.published_at_epoch, &report.context),
            classifier_world: report.classifier_world.as_ref(),
        })
        .collect()
}
