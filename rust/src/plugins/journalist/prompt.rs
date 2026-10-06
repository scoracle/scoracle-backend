//! Complete source admission, reporting continuity and model-input preparation.
use super::memories;
use super::memories::Continuity;
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::meta::EntityMeta;
use crate::plugins::support::source::Reporting;
use crate::studio::model::GenerateOptions;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashSet;

pub(super) const FRESH_TASK: &str =
    "Articulate each fresh item in its matching report_key, with the supplied voice and form.";

const HISTORY_TASK: &str = "The input is an articulation package.
meta identifies the entity.
fresh is the new source-backed reporting; each item has its output report_key.
memories contains earlier source-backed reporting explicitly attached to a fresh item by report_key.
voice describes how to articulate it.
form describes the output structure.
Articulate each fresh item in its matching report_key, using its attached memories where present. A report with no attached memories is articulated from its fresh item alone. Add no history and no claim that is not supplied.";

/// The current prompt keeps the flat keyed prose map and attaches prior reports
/// through matching report keys in `memories`.
pub const NARRATIVES_PROMPT_VERSION: &str = "n96-six-part";
pub const NUM_PREDICT: i32 = 900;
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
        if assemble(&subject, std::slice::from_ref(&item), &[]).len() > SOURCE_BUDGET_BYTES {
            dispositions.push(Disposition {
                article_id: item.id,
                reason: "complete_report_exceeds_context_budget",
            });
            continue;
        }
        let mut candidate = selected.clone();
        candidate.push(item.clone());
        if selected.len() == MAX_REPORTS
            || assemble(&subject, &candidate, &[]).len() > SOURCE_BUDGET_BYTES
        {
            deferred_ids.push(item.id);
            continue;
        }
        seen.insert(item.context.clone());
        selected.push(item);
    }
    let memories = memories::select(memory, &selected, now, |history| {
        assemble(&subject, &selected, history).len() <= CONTEXT_BUDGET_BYTES
    });
    // The world is assembled once and both rendered and hashed from, so the
    // fingerprint always describes the package the model will actually read.
    let rendered = assemble(&subject, &selected, &memories);
    let input_hash = crate::util::hash_components(
        &json!({
            "subject": subject, "reports": selected, "version": NARRATIVES_PROMPT_VERSION,
            "memory_source_hash": memory.study.as_ref().map(|s| &s.receipt.input_hash),
            "fresh_contract": FRESH_VERSION, "world": crate::util::hash_components(&rendered),
            "memories": memories,
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

/// This plugin's parts, in a form a quality fixture can store.
///
/// A fixture that stores only a rendered prompt cannot detect a changed
/// assembler: the stored string keeps passing while production sends something
/// else. Storing the parts and rebuilding through [`assemble`] makes that a test
/// failure. The type lives here because this plugin owns what its parts are;
/// the harness only chooses the JSON.
///
/// `memory` is index-aligned with `reports` — the same attachment production
/// resolved, so a stored fixture exercises the nesting rather than describing
/// it. `GroupSummary::before_epoch` is not serialized and does not reach the
/// model, so it does not survive the round trip; nothing rendered here reads it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    pub reports: Vec<CorpusItem>,
    pub memory: Vec<Option<memories::Selected>>,
}

impl Parts {
    pub fn assemble(&self) -> String {
        assemble(&self.subject, &self.reports, &self.memory)
    }
}

/// Serialize the same reporting package used for budget checks, hashing and
/// production requests. Stored parts replay through this function too.
pub fn assemble(
    subject: &EntityMeta,
    reports: &[CorpusItem],
    history: &[Option<memories::Selected>],
) -> String {
    #[derive(Serialize)]
    struct Attached<'a> {
        report_key: String,
        history: &'a [crate::plugins::memories::HistoryItem],
        history_groups: &'a [crate::plugins::memories::GroupSummary],
    }
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::plugins::meta::WritingIdentity<'a>,
        fresh: Vec<Report<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        memories: Vec<Attached<'a>>,
        voice: &'static str,
        form: serde_json::Value,
    }
    let memories = history
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            let selected = slot.as_ref().filter(|h| !h.is_empty())?;
            Some(Attached {
                report_key: format!("report_{}", index + 1),
                history: &selected.items,
                history_groups: &selected.groups,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: fresh_reports(reports),
        memories,
        voice: crate::plugins::journalist::voice::VOICE,
        form: crate::plugins::support::form::journalist_form(reports.len()),
    })
    .expect("journalist world serializes")
}

/// Production and replay use the exact assembled context measured by preparation.
pub fn prompt(assignment: &Assignment) -> String {
    assemble(
        &assignment.subject,
        &assignment.selected,
        &assignment.memories,
    )
}
pub fn system_prompt(assignment: &Assignment) -> &'static str {
    // A package where some reports carry history and others do not is a real
    // shape rather than an error. The manual describes history per report, so
    // the history task is selected when any report has one.
    if assignment
        .memories
        .iter()
        .any(|slot| slot.as_ref().is_some_and(|h| !h.is_empty()))
    {
        HISTORY_TASK
    } else {
        FRESH_TASK
    }
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

const FRESH_VERSION: &str = "journalist-fresh-v8";

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
pub(super) fn fresh_reports(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            reporting: Reporting::new(&report.source, report.published_at_epoch, &report.context),
        })
        .collect()
}

pub struct NarrativesMaterial {
    pub assignment: Assignment,
    pub sources: Vec<crate::plugins::harvester::delivery::SourceContext>,
}

/// One source-only path for every trigger, including old queue revisions. A
/// trigger cannot select an Editor packet fallback or upgrade a source receipt.
pub async fn load_narratives_material(
    pool: &PgPool,
    subject: EntityMeta,
    now: i64,
) -> Result<NarrativesMaterial> {
    let sources = crate::plugins::harvester::delivery::load_for_character(
        pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    let fresh = sources.iter().map(CorpusItem::from).collect::<Vec<_>>();
    let memory = super::memories::load_for_assignment(pool, &subject, &fresh, now).await?;
    let assignment = prepare(subject, fresh, &memory, now)?;
    let sources = sources
        .into_iter()
        .filter(|s| !assignment.deferred_ids.contains(&s.article_id))
        .collect();
    Ok(NarrativesMaterial {
        assignment,
        sources,
    })
}
