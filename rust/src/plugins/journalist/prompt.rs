//! Assemble Journalist identity, voice and fresh reporting into one JSON payload.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("narrative-logic", "NARRATIVE_LOGIC");
// Reserve room for complete reporting and output.
pub const ARTICLE_NUM_CTX: i32 = 32768;
pub use super::fresh::CorpusItem;
use crate::harness::model::GenerateOptions;
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashSet;

pub const TASK: &str = "Write concise news reports about meta, using voice as tone and fresh as the only evidence; preserve attribution, event dates and qualifications, and return only JSON mapping each report_key to its report text.";

pub const NARRATIVES_PROMPT_VERSION: &str = "n102-fresh-only-json";
pub const NUM_PREDICT: i32 = 900;
pub const NARRATIVES_OUTPUT_CONTRACT_VERSION: &str = "narratives-v12-fresh-only";
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;
pub const MAX_REPORTS: usize = 3;
pub const SOURCE_BUDGET_BYTES: usize = 24000;

#[derive(Clone, Debug)]
pub struct Disposition {
    pub article_id: i64,
    pub reason: &'static str,
}
#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: EntityMeta,
    pub selected: Vec<CorpusItem>,
    pub dispositions: Vec<Disposition>,
    pub deferred_ids: Vec<i64>,
    pub input_hash: String,
}

/// Exact-text equality is duplication, never semantic corroboration. Preserve
/// complete openings: a missing late qualification is worse than abstention.
pub fn prepare(
    subject: EntityMeta,
    mut corpus: Vec<CorpusItem>,
    published_reports: &[CorpusItem],
    now: i64,
) -> Result<Assignment> {
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
    let mut seen = published_reports
        .iter()
        .map(|r| {
            (
                r.context.clone(),
                r.classifier_world
                    .as_ref()
                    .map(serde_json::Value::to_string),
            )
        })
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for item in corpus {
        ensure!(
            item.id > 0 && ids.insert(item.id),
            "duplicate or invalid article identity"
        );
        let reason = if item.context.trim().is_empty() || item.source.trim().is_empty() {
            Some("missing_source_material")
        } else if crate::tools::source::contains_instruction_override(&item.context) {
            Some("source_instruction_override")
        } else if item.published_at_epoch.is_none() {
            Some("unknown_publication_time")
        } else if item.published_at_epoch.unwrap() > now {
            Some("future_publication_time")
        } else if now.saturating_sub(item.published_at_epoch.unwrap()) > LOOKBACK_SECONDS {
            Some("outdated_report")
        } else if seen.contains(&(
            item.context.clone(),
            item.classifier_world
                .as_ref()
                .map(serde_json::Value::to_string),
        )) {
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
        if assemble(&subject, std::slice::from_ref(&item)).len() > SOURCE_BUDGET_BYTES {
            dispositions.push(Disposition {
                article_id: item.id,
                reason: "complete_report_exceeds_context_budget",
            });
            continue;
        }
        let mut candidate = selected.clone();
        candidate.push(item.clone());
        if selected.len() == MAX_REPORTS
            || assemble(&subject, &candidate).len() > SOURCE_BUDGET_BYTES
        {
            deferred_ids.push(item.id);
            continue;
        }
        seen.insert((
            item.context.clone(),
            item.classifier_world
                .as_ref()
                .map(serde_json::Value::to_string),
        ));
        selected.push(item);
    }
    let rendered = assemble(&subject, &selected);
    let input_hash = crate::util::hash_components(
        &json!({
            "subject": subject, "reports": selected, "version": NARRATIVES_PROMPT_VERSION,
            "fresh_contract": FRESH_VERSION, "world": crate::util::hash_components(&rendered),
            "instruction": TASK,
        })
        .to_string(),
    );
    Ok(Assignment {
        subject,
        selected,
        dispositions,
        deferred_ids,
        input_hash,
    })
}

/// Stored parts rebuild through the same assembler as production.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    pub reports: Vec<CorpusItem>,
}

impl Parts {
    pub fn assemble(&self) -> String {
        assemble(&self.subject, &self.reports)
    }
}

/// Budget checks, hashing, fixtures and production all use this payload.
pub fn assemble(subject: &EntityMeta, reports: &[CorpusItem]) -> String {
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::tools::meta::WritingIdentity<'a>,
        voice: &'static str,
        fresh: Vec<super::fresh::Report<'a>>,
    }
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        voice: super::voice::VOICE,
        fresh: super::fresh::reports(reports),
    })
    .expect("journalist payload serializes")
}

pub fn prompt(assignment: &Assignment) -> String {
    assemble(&assignment.subject, &assignment.selected)
}

pub fn generation_options(assignment: &Assignment, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(TASK.to_string()),
        temperature: Some(0.0),
        num_predict: NUM_PREDICT,
        num_ctx: num_ctx.max(ARTICLE_NUM_CTX),
        json_mode: false,
        // Native shape validation stays in place without form instructions in the payload.
        format_schema: Some(crate::tools::form::journalist_schema(
            assignment.selected.len(),
        )),
        format_schema_raw: None,
    }
}

const FRESH_VERSION: &str = "journalist-fresh-v10-json";

pub struct NarrativesMaterial {
    pub assignment: Assignment,
    pub sources: Vec<crate::tools::source::SourceContext>,
}

/// One source-only path for every trigger, including old queue revisions. A
/// trigger cannot select an Editor packet fallback or upgrade a source receipt.
pub async fn load_narratives_material(
    pool: &PgPool,
    subject: EntityMeta,
    now: i64,
) -> Result<NarrativesMaterial> {
    let sources = crate::plugins::classifier::delivery::load_for_character(
        pool,
        crate::plugins::journalist::manifest::MANIFEST.id.as_str(),
        &subject.entity_type,
        subject.entity_id,
        &subject.sport,
    )
    .await?;
    let fresh = sources.iter().map(CorpusItem::from).collect::<Vec<_>>();
    let published = super::fresh::published_reports(pool, &subject, now).await?;
    let assignment = prepare(subject, fresh, &published, now)?;
    let sources = sources
        .into_iter()
        .filter(|s| !assignment.deferred_ids.contains(&s.article_id))
        .collect();
    Ok(NarrativesMaterial {
        assignment,
        sources,
    })
}
