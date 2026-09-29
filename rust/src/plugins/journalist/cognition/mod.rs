//! Source-bound Journalist preparation and articulation. The plugin selects complete
//! attributed reports; SmolLM3 articulates them, never selecting facts or scores.
use super::memories;
pub use super::memories::Continuity;
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::meta::EntityMeta;
use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

pub mod fresh;
mod journalist;
mod prompt;
pub use journalist::CHARACTER;
/// n95 attaches each report's history under the report itself and returns a flat
/// keyed prose map. Both change the prepared world and the response surface, so
/// this is a new contract and not a revision of n94.
pub const NARRATIVES_PROMPT_VERSION: &str = "n95";
pub const NUM_PREDICT: i32 = 900;
pub const NARRATIVES_SYSTEM_PROMPT: &str = prompt::FRESH_TASK;
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v11-nested-history";
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;
pub const MAX_REPORTS: usize = 3;
pub const SOURCE_BUDGET_BYTES: usize = 6000;
pub const CONTEXT_BUDGET_BYTES: usize = SOURCE_BUDGET_BYTES + memories::BUDGET_BYTES;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorpusItem {
    pub id: i64,
    pub title: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
}
impl From<&SourceContext> for CorpusItem {
    fn from(s: &SourceContext) -> Self {
        Self {
            id: s.article_id,
            title: s.headline.clone(),
            context: s.context.clone(),
            source: s.source.clone(),
            published_at_epoch: s.published_at_epoch,
        }
    }
}
/// One grounded storyline with deterministic impact and evidence provenance.
#[derive(Clone, Debug)]
pub struct Narrative {
    pub title: String,
    pub body: String,
    pub impact: i32,
    pub impact_components: serde_json::Value,
    pub input_news_ids: Vec<i64>,
    pub source_count: i32,
    pub source_names: Vec<String>,
    pub source_latest_epoch: Option<i64>,
    pub source_oldest_epoch: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct Disposition {
    pub article_id: i64,
    pub reason: &'static str,
}
#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: EntityMeta,
    pub selected: Vec<CorpusItem>,
    /// Index-aligned with `selected`: the history this plugin attached to each
    /// report, or `None` where it could not determine which history belongs.
    pub memories: Vec<Option<memories::Selected>>,
    pub memory_receipt: Option<memories::Receipt>,
    pub dispositions: Vec<Disposition>,
    pub deferred_ids: Vec<i64>,
    pub input_hash: String,
}

/// Exact-text equality is duplication, never semantic corroboration. Preserve
/// complete openings: a missing late qualification is worse than abstention.
pub fn prepare(
    subject: EntityMeta,
    mut corpus: Vec<CorpusItem>,
    memory: &Continuity,
    now: i64,
) -> Result<Assignment> {
    ensure!(
        memory
            .study
            .as_ref()
            .is_none_or(|s| s.receipt.subject == subject),
        "memory subject does not match assignment"
    );
    ensure!(
        subject.entity_id > 0
            && !subject.name.trim().is_empty()
            && !subject.sport.trim().is_empty()
            && !subject.entity_type.trim().is_empty(),
        "Journalist subject metadata is incomplete"
    );
    corpus.sort_by(|a, b| {
        b.published_at_epoch
            .cmp(&a.published_at_epoch)
            .then(b.id.cmp(&a.id))
    });
    let mut selected = Vec::new();
    let mut dispositions = Vec::new();
    let mut deferred_ids = Vec::new();
    let mut seen = memory
        .published_reports
        .iter()
        .map(|r| r.context.clone())
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for item in corpus {
        ensure!(
            item.id > 0 && ids.insert(item.id),
            "duplicate or invalid article identity"
        );
        let reason = if item.context.trim().is_empty() || item.source.trim().is_empty() {
            Some("missing_source_material")
        } else if crate::plugins::support::source::contains_instruction_override(&item.context) {
            Some("source_instruction_override")
        } else if item.published_at_epoch.is_none() {
            Some("unknown_publication_time")
        } else if item.published_at_epoch.unwrap() > now {
            Some("future_publication_time")
        } else if now.saturating_sub(item.published_at_epoch.unwrap()) > LOOKBACK_SECONDS {
            Some("outdated_report")
        } else if seen.contains(&item.context) {
            Some("already_reported_exact_text")
        } else {
            None
        };
        if let Some(reason) = reason {
            dispositions.push(Disposition {
                article_id: item.id,
                reason,
            });
            continue;
        }
        if render_context(&subject, std::slice::from_ref(&item), &[]).len() > SOURCE_BUDGET_BYTES {
            dispositions.push(Disposition {
                article_id: item.id,
                reason: "complete_report_exceeds_context_budget",
            });
            continue;
        }
        let mut candidate = selected.clone();
        candidate.push(item.clone());
        if selected.len() == MAX_REPORTS
            || render_context(&subject, &candidate, &[]).len() > SOURCE_BUDGET_BYTES
        {
            deferred_ids.push(item.id);
            continue;
        }
        seen.insert(item.context.clone());
        selected.push(item);
    }
    let memories = memories::select(memory, &selected, now, |history| {
        render_context(&subject, &selected, history).len() <= CONTEXT_BUDGET_BYTES
    });
    // The world is assembled once and both rendered and hashed from, so the
    // fingerprint always describes the package the model will actually read.
    let world = assemble(&subject, &selected, &memories);
    let input_hash = crate::util::hash_components(
        &json!({
            "subject": subject, "reports": selected, "version": NARRATIVES_PROMPT_VERSION,
            "memory_source_hash": memory.study.as_ref().map(|s| &s.receipt.input_hash),
            "fresh_contract": fresh::VERSION, "world": world.hash(), "memories": memories,
        })
        .to_string(),
    );
    Ok(Assignment {
        subject,
        selected,
        memories,
        memory_receipt: memory.study.as_ref().map(|s| s.receipt.clone()),
        dispositions,
        deferred_ids,
        input_hash,
    })
}

/// Compose the model's world from independently prepared components.
///
/// History is nested under the report it belongs to. The plugin resolved that
/// attachment before this function is called, so the model reads one prepared
/// set per report and is never handed two parallel arrays to pair up itself.
///
/// The parts and their order are this plugin's choice; `assembly::World` only
/// renders them, deterministically and in the order given here.
fn render_context(
    subject: &EntityMeta,
    reports: &[CorpusItem],
    history: &[Option<memories::Selected>],
) -> String {
    assemble(subject, reports, history).render()
}

/// The prepared world, before it is rendered.
///
/// Returned rather than rendered inline so preparation can measure and hash the
/// same world the model reads, instead of assembling it again and hoping the two
/// agree.
fn assemble(
    subject: &EntityMeta,
    reports: &[CorpusItem],
    history: &[Option<memories::Selected>],
) -> crate::plugins::assembly::World {
    let attached = history
        .iter()
        .map(|slot| {
            let (items, groups) = slot
                .as_ref()
                .map(|h| (h.items.as_slice(), h.groups.as_slice()))
                .unwrap_or_default();
            fresh::History {
                history: items,
                history_groups: groups,
            }
        })
        .collect::<Vec<_>>();
    crate::plugins::assembly::World::new()
        .part("identity", subject.for_writing())
        .part("fresh", fresh::prepare(reports, &attached))
        .part("voice", journalist::CHARACTER)
        .part(
            "form",
            crate::plugins::support::form::journalist_form(reports.len()),
        )
}

/// Production and replay use the exact assembled context measured by preparation.
pub fn prompt(assignment: &Assignment) -> String {
    render_context(
        &assignment.subject,
        &assignment.selected,
        &assignment.memories,
    )
}
pub fn system_prompt(assignment: &Assignment) -> &'static str {
    // A package where some reports carry history and others do not is a real
    // shape rather than an error. The manual describes history per report, so
    // the history task is selected when any report has one.
    prompt::task(
        assignment
            .memories
            .iter()
            .any(|slot| slot.as_ref().is_some_and(|h| !h.is_empty())),
    )
}
pub fn generation_options(assignment: &Assignment, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(system_prompt(assignment).to_string()),
        temperature: Some(0.0),
        num_predict: NUM_PREDICT,
        num_ctx,
        json_mode: false,
        // The package supplies the form to the model; the matching grammar and parser keep
        // publication atomic without adding content direction.
        format_schema: Some(crate::plugins::support::form::journalist_schema(
            assignment.selected.len(),
        )),
        format_schema_raw: None,
    }
}

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

#[derive(Clone, Debug)]
pub struct NarrativesProduct {
    pub memory_provenance: serde_json::Value,
    pub narratives: Vec<Narrative>,
    pub budget_truncated_ids: Vec<i64>,
    pub card_score: Option<i16>,
    pub headline: Option<String>,
}
pub type NarrativesOutput = Generation<NarrativesProduct>;

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
                let title = fresh::opening(item, &self.assignment.subject.name);
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
pub async fn create(
    studio: &Studio<'_>,
    assignment: &Assignment,
    now: i64,
    num_ctx: i32,
) -> Result<NarrativesOutput> {
    if assignment.selected.is_empty() {
        return Ok(Generation::uncalled(
            NarrativesProduct {
                memory_provenance: json!({"receipt":assignment.memory_receipt,"selected":assignment.memories}),
                narratives: Vec::new(),
                budget_truncated_ids: assignment.deferred_ids.clone(),
                card_score: None,
                headline: None,
            },
            studio.model_name().to_string(),
            NARRATIVES_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash.clone()),
        ));
    }
    let output = studio
        .extract(
            &prompt(assignment),
            &generation_options(assignment, num_ctx),
            &EditionParser { assignment, now },
            |_| None,
        )
        .await?;
    let call = GenerationCall::from(&output);
    Ok(Generation::called(
        output
            .value
            .ok_or_else(|| anyhow::anyhow!("missing edition"))?,
        output.model,
        NARRATIVES_PROMPT_VERSION,
        assignment.selected.iter().map(|s| s.id).collect(),
        Some(assignment.input_hash.clone()),
        call,
    ))
}
#[cfg(test)]
mod tests;
