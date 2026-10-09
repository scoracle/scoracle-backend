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

/// The plugin owns which source development labels the report. Prefer the
/// complete opening sentence; fall back to canonical identity rather than ask
/// articulation to select or compress a claim.
fn opening(report: &CorpusItem, entity_name: &str) -> String {
    let excerpt = report.context.trim();
    let sentence_end = excerpt
        .char_indices()
        .find(|(_, character)| matches!(character, '.' | '?' | '!'))
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(excerpt.len());
    let opening = excerpt[..sentence_end].trim();
    if !opening.is_empty() && opening.chars().count() <= crate::tools::form::HOOK_MAX_CHARS {
        opening.to_string()
    } else {
        entity_name.trim().to_string()
    }
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
        anyhow::ensure!(
            self.activity.reports.len() == self.assignment.selected.len(),
            "Journalist activity does not match selected reports"
        );
        let reply = crate::tools::form::parse_journalist(raw, self.assignment.selected.len())?;
        let narratives = self
            .assignment
            .selected
            .iter()
            .zip(reply.narratives)
            .zip(&self.activity.reports)
            .map(|((item, prose), activity)| {
                let impact = activity.score;
                let impact_components = activity.components.clone();
                let evidence = std::slice::from_ref(item);
                let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
                    source_metadata(evidence);
                let title = opening(item, &self.assignment.subject.name);
                crate::tools::form::validate_hook(Some(&title))?;
                Ok(Narrative {
                    title,
                    body: prose.text,
                    impact,
                    impact_components,
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
            memory_provenance: json!({"receipt":null,"selected":[]}),
            narratives,
            budget_truncated_ids: self.assignment.deferred_ids.clone(),
            card_score: Some(self.activity.card_score),
            headline,
        }))
    }
}
