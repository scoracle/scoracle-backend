//! Select dated, frequency-ranked historical reporting for the fresh assignment.
use super::prompt::CorpusItem;
use crate::tools::memories::{GroupSummary, HistoryItem, Observation};
use crate::tools::meta::EntityMeta;
use crate::util::utc_timestamp;
use anyhow::Result;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

// Plugin policy: change the requested horizon here, not in a precompute job.
pub const LOOKBACK_SECONDS: i64 = 30 * 86400;
const MAX_GROUPS: usize = 3;
pub(super) const BUDGET_BYTES: usize = 2200;

#[derive(Clone, Debug, Default)]
pub struct Continuity {
    /// What this plugin has already published about the subject. It is
    /// self-memory, not studied history: its only consumer is exact fresh-source
    /// deduplication, and it is never presented to a model.
    pub published_reports: Vec<CorpusItem>,
    pub study: Option<crate::tools::memories::Study>,
    /// Storyline membership for the selected fresh reports, resolved by this
    /// plugin. It is the exact link that attaches studied history to a report;
    /// see [`select`].
    pub storylines: HashMap<i64, i64>,
}

pub use crate::tools::memories::Receipt;

/// The history this plugin selected: the shared items, plus a description of
/// each group they came from. Both types are the shared ones, so the `history`
/// key a model reads has the same shape here as in every other character.
#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct Selected {
    pub items: Vec<HistoryItem>,
    pub groups: Vec<GroupSummary>,
}

impl Selected {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Articles indexed to the same story group, for the requested subject and
/// window. This is the grouping the shared study used to perform itself, moved
/// here: the storyline tables are a Journalist dependency, and the study should
/// not carry a plugin's idea of what makes two articles the same story.
async fn storyline_groups(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
) -> Result<HashMap<i64, i64>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT sa.article_id, min(sa.storyline_id) AS storyline_id
         FROM storyline_articles sa
         JOIN storyline_entities se ON se.storyline_id=sa.storyline_id
         JOIN news_articles a ON a.id=sa.article_id
         WHERE se.sport=$1 AND se.entity_type=$2 AND se.entity_id=$3
           AND a.published_at>=to_timestamp($4::double precision)
           AND a.published_at<to_timestamp($5::double precision)
         GROUP BY sa.article_id",
    )
    .bind(&subject.sport)
    .bind(&subject.entity_type)
    .bind(subject.entity_id)
    .bind(from as f64)
    .bind(before as f64)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Exposed for the isolated database test that covers the grouping SQL; the
/// production path reaches it only through `load_for_assignment`.
#[cfg(test)]
pub(crate) async fn storyline_groups_for_test(
    pool: &PgPool,
    subject: &EntityMeta,
    from: i64,
    before: i64,
) -> Result<HashMap<i64, i64>> {
    storyline_groups(pool, subject, from, before).await
}

/// The Journalist's own words for what a group of observations represents.
/// Counts describe the stored article population, not confirmations or an
/// inferred event.
fn population(topic: &str) -> String {
    if topic.starts_with("storyline/") {
        "articles indexed to a story group".into()
    } else {
        "one canonical article".into()
    }
}

/// Attach studied history to each fresh report, index-aligned with `fresh`.
///
/// This is the plugin's assembly work and it happens entirely before inference.
/// The join is deterministic and never asks the model to resolve a pairing:
///
/// Only the report's own resolved storyline can attach history. A shared subject
/// or an earlier reporting window does not establish that two reports belong
/// together; an absent or unavailable storyline match leaves history unknown.
///
/// Returning one entry per report is what lets the package nest history under
/// the report it belongs to instead of presenting two parallel arrays.
pub(super) fn select(
    memory: &Continuity,
    fresh: &[CorpusItem],
    _now: i64,
    fits: impl Fn(&[Option<Selected>]) -> bool,
) -> Vec<Option<Selected>> {
    let mut attached: Vec<Option<Selected>> = vec![None; fresh.len()];
    let Some(study) = memory.study.as_ref() else {
        return attached;
    };
    let Some(oldest) = fresh.iter().filter_map(|r| r.published_at_epoch).min() else {
        return attached;
    };
    if study.receipt.before > oldest {
        return attached;
    }
    let groups: Vec<GroupSummary> = study
        .findings
        .iter()
        .filter(|finding| {
            !finding
                .reports
                .iter()
                .any(|report| crate::tools::source::contains_instruction_override(&report.headline))
        })
        .filter(|finding| {
            // History cannot restate fresh reporting, and cannot include it.
            !finding.reports.iter().any(|r| {
                r.reported_at >= oldest
                    || fresh
                        .iter()
                        .any(|f| f.id == r.article_id || f.id == r.canonical_id)
            })
        })
        .map(|finding| GroupSummary::of(finding, population(&finding.topic)))
        .take(MAX_GROUPS)
        .collect();
    if groups.is_empty() {
        return attached;
    }
    for (index, report) in fresh.iter().enumerate() {
        let candidate = attach(memory, &groups, report);
        let Some(candidate) = candidate else { continue };
        if serde_json::to_vec(&candidate)
            .expect("memory serializes")
            .len()
            > BUDGET_BYTES
        {
            continue;
        }
        // Measure the package this actually produces, attachment included, so the
        // budget is spent on what the model will read.
        let mut probe = attached.clone();
        probe[index] = Some(candidate.clone());
        if !fits(&probe) {
            continue;
        }
        attached[index] = Some(candidate);
    }
    attached
}

/// Attach history only through a resolved storyline match.
fn attach(memory: &Continuity, groups: &[GroupSummary], report: &CorpusItem) -> Option<Selected> {
    let storyline = memory.storylines.get(&report.id)?;
    let topic = format!("storyline/{storyline}");
    let group = groups.iter().find(|g| g.group == topic)?;
    let items = items_for(memory, &topic);
    (!items.is_empty()).then(|| Selected {
        items,
        groups: vec![group.clone()],
    })
}

/// The observed history items belonging to one studied group.
///
/// Identical headlines share a group key, but every source and date stays
/// attached. This is lossless presentation deduplication, not a corroboration
/// calculation.
fn items_for(memory: &Continuity, topic: &str) -> Vec<HistoryItem> {
    let Some(study) = memory.study.as_ref() else {
        return Vec::new();
    };
    study
        .findings
        .iter()
        .filter(|finding| finding.topic == topic)
        .flat_map(|finding| {
            finding.reports.iter().map(move |report| HistoryItem {
                group: Some(topic.to_string()),
                publisher: report.publisher.clone(),
                published_at: utc_timestamp(report.reported_at),
                reported_headline: report.headline.clone(),
            })
        })
        .collect()
}

pub async fn load_for_assignment(
    pool: &PgPool,
    subject: &EntityMeta,
    fresh: &[CorpusItem],
    now: i64,
) -> Result<Continuity> {
    let mut continuity = load(pool, subject, now).await?;
    // Determine actual fresh eligibility before spending a study call or fixing
    // its historical cutoff. Deferred/outdated/duplicate deliveries are not the
    // edition's reporting clock.
    let prepared = super::prompt::prepare(subject.clone(), fresh.to_vec(), &continuity, now)?;
    if let Some(before) = prepared
        .selected
        .iter()
        .filter_map(|r| r.published_at_epoch)
        .filter(|t| *t <= now)
        .min()
    {
        let ids = fresh.iter().map(|r| r.id).collect::<Vec<_>>();
        let from = before - LOOKBACK_SECONDS;
        let groups = storyline_groups(pool, subject, from, before).await?;
        let topic = move |observation: &Observation| {
            groups
                .get(&observation.article_id)
                .map(|id| format!("storyline/{id}"))
        };
        // The same lookup, widened to the fresh reports, is the exact link that
        // attaches a studied group to the report that continues it. Resolving it
        // here is what keeps the pairing out of the model.
        continuity.storylines = storyline_groups(
            pool,
            subject,
            from,
            prepared
                .selected
                .iter()
                .filter_map(|r| r.published_at_epoch)
                .max()
                .unwrap_or(before)
                + 1,
        )
        .await?;
        continuity.study = Some(
            crate::tools::memories::reporting_scope(
                pool,
                subject,
                from,
                before,
                &ids,
                MAX_GROUPS,
                2,
                &[],
                Some(&topic),
            )
            .await?,
        );
    }
    Ok(continuity)
}

pub async fn load(pool: &PgPool, subject: &EntityMeta, now: i64) -> Result<Continuity> {
    // Receipt text and v6/v7 source identity preserve the original reporting.
    // Legacy receipts have no identity snapshot; retain their article attribution.
    // A prompt revision does not erase prior coverage.
    let rows = sqlx::query(
        "WITH published_reports AS ( \
         SELECT DISTINCT ON (c.article_id) c.article_id, c.headline, c.context_text, \
          COALESCE(CASE WHEN c.contract_version IN ('harvest-context-v6','harvest-context-v7') \
            THEN c.model_provenance->'source_identity'->>'source' ELSE a.source END,'') AS source, \
          EXTRACT(EPOCH FROM CASE WHEN c.contract_version IN ('harvest-context-v6','harvest-context-v7') \
            THEN (c.model_provenance->'source_identity'->>'published_at')::timestamptz \
            ELSE a.published_at END)::bigint AS published_at_epoch \
          FROM harvester_classifications c \
          JOIN harvester_assignments d ON d.classification_id=c.id \
          JOIN news_articles a ON a.id=c.article_id \
          WHERE c.entity_type=$1 AND c.entity_id=$2 AND c.sport=$3 \
            AND d.plugin_id=$4 AND d.status='used' \
            AND c.created_at >= to_timestamp($5::double precision) \
            AND c.created_at < to_timestamp($6::double precision + 1) \
            AND EXISTS (SELECT 1 FROM news_summaries n WHERE n.entity_type=c.entity_type \
              AND n.entity_id=c.entity_id AND n.sport=c.sport AND c.article_id=ANY(n.input_news_ids) \
              AND n.body IS NOT NULL) \
          ORDER BY c.article_id, c.created_at DESC, c.id DESC) \
         SELECT * FROM published_reports ORDER BY published_at_epoch DESC NULLS LAST, article_id DESC LIMIT 256")
        .bind(&subject.entity_type).bind(subject.entity_id).bind(&subject.sport)
        .bind(super::manifest::MANIFEST.id.as_str()).bind(now - super::prompt::LOOKBACK_SECONDS).bind(now)
        .fetch_all(pool).await?;
    let mut published_reports = rows
        .into_iter()
        .map(|r| CorpusItem {
            id: r.get("article_id"),
            title: r.get("headline"),
            context: r.get("context_text"),
            source: r.get("source"),
            published_at_epoch: r.get("published_at_epoch"),
        })
        .collect::<Vec<_>>();
    published_reports.sort_by(|a, b| {
        b.published_at_epoch
            .cmp(&a.published_at_epoch)
            .then(b.id.cmp(&a.id))
    });
    published_reports.truncate(256);
    Ok(Continuity {
        published_reports,
        study: None,
        storylines: HashMap::new(),
    })
}
