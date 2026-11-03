//! Load, select and present Classifier headlines for the daily package.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "new_content contains the articles you may use as evidence. Their text identifies who acts, speaks or is discussed; the target is not automatically the actor or speaker. Keep names, actions, numbers, units, dates and qualifications with their original subjects. Preserve conditions, denials and corrections. Repeated claims are not independent confirmation. Target attributes are background, not evidence for a news story. Headlines found through a Google search for the target may include unrelated stories, repeated coverage, speculation and clickbait. Search inclusion does not establish relevance or confirm a claim. Some articles match the target but reveal no news. Keep specific news, opinions and reactions. Do not invent the missing details or turn a question or implication into a fact. Look for recurring themes supported by distinct reports. Repeated copies are not independent support. Keep unrelated stories separate; do not force a narrative from thin evidence.",
    fields: &[
        ("id", "article number within this request; cite it in report_ids."),
        ("publisher", "outlet supplying this article. Name it when citing the story; it is not necessarily the speaker or original source named in the text."),
        ("headline", "article title, not the full article. Use only what it states; do not guess omitted details or identify unnamed people."),
        ("publisher_excerpt", "supplied passage."),
        ("evidence_scope", "available text only; headline supplies no unseen article contents."),
    ],
};

use super::request::{assemble, prompt, Assignment, Disposition, NARRATIVES_PROMPT_VERSION};
use crate::tools::meta::EntityMeta;
use crate::tools::source::SourceContext;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashSet;

const FRESH_VERSION: &str = "journalist-fresh-v11-token-admission";

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
    id: usize,
    publisher: &'a str,
    evidence_scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    headline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher_excerpt: Option<String>,
}

impl Report<'_> {
    pub(super) fn evidence(&self) -> crate::tools::fresh::ReportEvidence<'_> {
        crate::tools::fresh::ReportEvidence {
            publisher: self.publisher,
            text: format!(
                "{}: {}",
                self.publisher,
                self.headline
                    .as_deref()
                    .or(self.publisher_excerpt.as_deref())
                    .unwrap_or_default()
            ),
            published_at: None,
        }
    }
}

/// Preserve selected order and the declared source scope. Headline-only reports
/// expose their headline; historical article reports preserve their complete text.
///
pub(super) fn reports(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            id: index + 1,
            headline: report
                .classifier_world
                .as_ref()
                .is_some_and(|w| w["evidence_scope"] == "headline")
                .then(|| crate::tools::fetch::decode_entities(&report.title)),
            publisher: &report.source,
            publisher_excerpt: report
                .classifier_world
                .as_ref()
                .is_none_or(|w| w["evidence_scope"] != "headline")
                .then(|| crate::tools::fetch::decode_entities(&report.context)),
            evidence_scope: if report
                .classifier_world
                .as_ref()
                .is_some_and(|world| world["evidence_scope"] == "headline")
            {
                "headline"
            } else {
                "publisher_excerpt"
            },
        })
        .collect()
}

pub(super) fn select(reports: &[CorpusItem]) -> Vec<CorpusItem> {
    crate::tools::source::top_articles(reports, "subject", |r| r.classifier_world.as_ref())
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
    let mut seen = published_reports
        .iter()
        .map(|r| crate::tools::fresh::evidence_key(&r.source, &r.context))
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for item in corpus {
        ensure!(
            item.id > 0 && ids.insert(item.id),
            "duplicate or invalid article identity"
        );
        let key = crate::tools::fresh::evidence_key(&item.source, &item.context);
        let reason = if item
            .classifier_world
            .as_ref()
            .is_some_and(|world| world["target_identity_resolved"] == false)
        {
            Some("unresolved_target_identity")
        } else if item.context.trim().is_empty() || item.source.trim().is_empty() {
            Some("missing_source_material")
        } else if crate::tools::source::is_headline_spam(&item.context) {
            Some("livestream_spam")
        } else if crate::tools::source::contains_instruction_override(&item.context) {
            Some("source_instruction_override")
        } else if item.published_at_epoch.is_none() {
            Some("unknown_publication_time")
        } else if item.published_at_epoch.unwrap() > now {
            Some("future_publication_time")
        } else if seen.contains(&key) {
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
        seen.insert(key);
        selected.push(item);
    }
    let selected = select(&selected);
    // The provider admits the complete request using exact tokens plus reserved output.
    let rendered = assemble(&subject, &selected, None);
    let input_hash = crate::util::hash_components(
        &json!({
            "subject": subject, "reports": selected, "version": NARRATIVES_PROMPT_VERSION,
            "fresh_contract": FRESH_VERSION, "world": crate::util::hash_components(&rendered),
            "instruction": super::character_instructions::INSTRUCTIONS,
        })
        .to_string(),
    );
    Ok(Assignment {
        database_record: None,
        subject,
        selected,
        dispositions,
        input_hash,
    })
}

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
    let mut assignment = prepare(subject, fresh, &[], now)?;
    if !assignment.selected.is_empty() {
        assignment.database_record = crate::tools::meta::load_character_identity(
            pool,
            &assignment.subject.entity_type,
            assignment.subject.entity_id,
            &assignment.subject.sport,
        )
        .await?;
    }
    assignment.input_hash = crate::util::hash_components(&serde_json::to_string(&(
        &assignment.input_hash,
        prompt(&assignment),
    ))?);
    Ok(NarrativesMaterial {
        assignment,
        sources,
    })
}

pub(super) fn context(reports: &[CorpusItem]) -> crate::tools::prompt::Context<Vec<Report<'_>>> {
    crate::tools::prompt::Context {
        guide: GUIDE,
        data: self::reports(reports),
    }
}
