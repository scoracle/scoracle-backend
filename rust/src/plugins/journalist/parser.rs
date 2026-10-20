//! Strict edition parsing and deterministic source metadata.
use super::activity::EditionActivity;
use super::prompt::{Assignment, CorpusItem};
use super::{Narrative, NarrativesProduct};
use crate::harness::Parser;
use anyhow::Result;
use serde_json::json;
use std::collections::HashSet;

fn source_metadata(corpus: &[CorpusItem]) -> (i32, Vec<String>, Option<i64>, Option<i64>) {
    let mut source_names = Vec::new();
    let mut seen_sources = HashSet::new();
    let mut latest = None;
    let mut oldest = None;
    for item in corpus {
        let source = item.source.trim();
        if !source.is_empty() && seen_sources.insert(source.to_lowercase()) {
            source_names.push(source.to_string());
        }
        if let Some(epoch) = item.published_at_epoch {
            latest = Some(latest.map_or(epoch, |current: i64| current.max(epoch)));
            oldest = Some(oldest.map_or(epoch, |current: i64| current.min(epoch)));
        }
    }
    (corpus.len() as i32, source_names, latest, oldest)
}

/// Production and replay share the same strict form parser and source mapping.
/// This checks shape and surface, not semantic entailment; fidelity is evaluated
/// against the prepared reports, not inferred from parser success.
pub struct EditionParser<'a> {
    pub assignment: &'a Assignment,
    pub activity: &'a EditionActivity,
}
impl Parser<NarrativesProduct> for EditionParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<NarrativesProduct>> {
        let reply = parse_reports(raw, &self.assignment.selected)?;
        let narratives = reply
            .narratives
            .into_iter()
            .map(|prose| {
                let evidence: Vec<_> = prose
                    .report_keys
                    .iter()
                    .map(|key| {
                        let index = key
                            .strip_prefix("report_")
                            .unwrap()
                            .parse::<usize>()
                            .unwrap()
                            - 1;
                        self.assignment.selected[index].clone()
                    })
                    .collect();
                let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
                    source_metadata(&evidence);
                Ok(Narrative {
                    title: prose.headline,
                    body: prose.body,
                    // Cited-source activity is measured after articulation, before publication.
                    impact: 0,
                    impact_components: json!({}),
                    input_news_ids: evidence.iter().map(|r| r.id).collect(),
                    source_count,
                    source_names,
                    source_latest_epoch,
                    source_oldest_epoch,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let headline = narratives.first().map(|narrative| narrative.title.clone());
        Ok(Some(NarrativesProduct {
            memory_provenance: json!({"version":"headline-archive-v1","selected":self.assignment.memories}),
            narratives,
            card_score: Some(self.activity.card_score),
            headline,
        }))
    }
}

pub(super) fn parse_reports(
    raw: &str,
    reports: &[CorpusItem],
) -> Result<crate::tools::form::JournalistReply> {
    let reply = crate::tools::form::parse_journalist(raw, reports.len())?;
    for prose in &reply.narratives {
        let sources: Vec<_> = prose
            .report_keys
            .iter()
            .map(|key| {
                let index = key
                    .strip_prefix("report_")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
                    - 1;
                let report = &reports[index];
                format!("{}: {} {}", report.source, report.title, report.context)
            })
            .collect();
        crate::tools::fresh::validate_named_facts(
            &format!("{}. {}", prose.headline, prose.body),
            &sources.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .map_err(|error| crate::tools::form::SurfaceError(error.to_string()))?;
    }
    Ok(reply)
}
