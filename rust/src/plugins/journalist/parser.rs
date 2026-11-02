//! Strict edition parsing and deterministic source metadata.
use super::activity::EditionActivity;
use super::request::{Assignment, CorpusItem};
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
        let selected = super::fresh::select(&self.assignment.selected);
        let reply = parse_reports(raw, &selected)?;
        let headline = reply.headline;
        let mut narratives = Vec::new();
        if let Some(title) = &headline {
            let mut seen = HashSet::new();
            let evidence: Vec<_> = reply
                .paragraphs
                .iter()
                .flat_map(|p| &p.report_ids)
                .filter(|id| seen.insert(**id))
                .map(|id| selected[*id - 1].clone())
                .collect();
            let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
                source_metadata(&evidence);
            narratives.push(Narrative {
                title: title.clone(),
                body: reply
                    .paragraphs
                    .iter()
                    .map(|p| p.body.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                impact: 0,
                impact_components: json!({}),
                input_news_ids: evidence.iter().map(|r| r.id).collect(),
                source_count,
                source_names,
                source_latest_epoch,
                source_oldest_epoch,
            });
        }
        Ok(Some(NarrativesProduct {
            memory_provenance: json!({"receipt":null,"selected":[]}),
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
    let evidence = super::fresh::reports(reports);
    let mut cited = HashSet::new();
    for paragraph in &reply.paragraphs {
        let sources: Vec<_> = paragraph
            .report_ids
            .iter()
            .map(|id| {
                cited.insert(*id);
                evidence[*id - 1].evidence()
            })
            .collect();
        crate::tools::fresh::validate_report_facts(&paragraph.body, &sources)
            .map_err(|error| crate::tools::form::SurfaceError(error.to_string()))?;
    }
    if let Some(headline) = &reply.headline {
        let sources: Vec<_> = evidence
            .iter()
            .enumerate()
            .filter(|(i, _)| cited.contains(&(i + 1)))
            .map(|(_, report)| report.evidence())
            .collect();
        crate::tools::fresh::validate_report_facts(headline, &sources)
            .map_err(|error| crate::tools::form::SurfaceError(error.to_string()))?;
    }
    Ok(reply)
}
