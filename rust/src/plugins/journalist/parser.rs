//! Strict edition parsing and deterministic source metadata.
use super::prompt::{Assignment, CorpusItem};
use super::{Narrative, NarrativesProduct};
use crate::studio::Parser;
use anyhow::Result;
use serde_json::json;
use std::collections::HashSet;

/// Descriptive source activity, not confidence, significance or corroboration.
fn compute_news_impact(corpus: &[CorpusItem], now: i64) -> (i32, serde_json::Value) {
    let volume = 60.0 * (1.0 - (-(corpus.len() as f64) / 5.0).exp());
    let sources = corpus
        .iter()
        .map(|s| s.source.to_lowercase())
        .collect::<HashSet<_>>()
        .len();
    let breadth = 25.0_f64.min(sources as f64 * 6.0);
    let newest = corpus.iter().filter_map(|s| s.published_at_epoch).max();
    let recency = newest.map_or(0.0, |t| match now.saturating_sub(t) {
        0..=43200 => 15.0,
        43201..=86400 => 10.0,
        86401..=172800 => 5.0,
        _ => 0.0,
    });
    (
        (volume + breadth + recency).round().clamp(0.0, 100.0) as i32,
        json!({"policy":"source-activity-v2", "article_count":corpus.len(),
            "distinct_sources":sources, "volume":volume, "source_breadth":breadth,"recency":recency}),
    )
}
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
    if !opening.is_empty()
        && opening.chars().count() <= crate::plugins::support::form::HOOK_MAX_CHARS
    {
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
    pub now: i64,
}
impl Parser<NarrativesProduct> for EditionParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<NarrativesProduct>> {
        let reply =
            crate::plugins::support::form::parse_journalist(raw, self.assignment.selected.len())?;
        let narratives = self
            .assignment
            .selected
            .iter()
            .zip(reply.narratives)
            .map(|(item, prose)| {
                let (impact, impact_components) =
                    compute_news_impact(std::slice::from_ref(item), self.now);
                // Historical study lineage is retained separately. It cannot
                // inflate the fresh-source count, dates, score or delivery IDs.
                let evidence = std::slice::from_ref(item);
                let (source_count, source_names, source_latest_epoch, source_oldest_epoch) =
                    source_metadata(evidence);
                let title = opening(item, &self.assignment.subject.name);
                crate::plugins::support::form::validate_hook(Some(&title))?;
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
            memory_provenance: json!({"receipt":self.assignment.memory_receipt,"selected":self.assignment.memories}),
            narratives,
            budget_truncated_ids: self.assignment.deferred_ids.clone(),
            card_score: Some(
                compute_news_impact(&self.assignment.selected, self.now)
                    .0
                    .clamp(1, 99) as i16,
            ),
            headline,
        }))
    }
}
