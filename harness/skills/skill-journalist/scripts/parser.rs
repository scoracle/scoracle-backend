//! Decode the news edition and attach its cited reports.
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

/// Link the model’s paragraphs to the reports in this assignment.
pub struct EditionParser<'a> {
    pub assignment: &'a Assignment,
    pub activity: &'a EditionActivity,
}
impl Parser<NarrativesProduct> for EditionParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<NarrativesProduct>> {
        let selected = super::fresh::select(&self.assignment.selected);
        let reply = crate::tools::form::decode_journalist(raw, selected.len())?;
        Ok(Some(self.product(reply, &selected)))
    }
}

impl EditionParser<'_> {
    fn product(
        &self,
        reply: crate::tools::form::JournalistReply,
        selected: &[CorpusItem],
    ) -> NarrativesProduct {
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
        NarrativesProduct {
            memory_provenance: json!({"receipt":null,"selected":[]}),
            narratives,
            card_score: headline.as_ref().map(|_| self.activity.card_score),
            headline,
        }
    }
}
