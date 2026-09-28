//! Select dated, frequency-ranked historical reporting for the fresh assignment.
use super::cognition::CorpusItem;
use crate::plugins::meta::EntityMeta;
use crate::util::utc_timestamp;
use anyhow::Result;
use serde::Serialize;
use sqlx::{PgPool, Row};

// Plugin policy: change the requested horizon here, not in a precompute job.
pub const LOOKBACK_SECONDS: i64 = 30 * 86400;
const MAX_GROUPS: usize = 3;
pub(super) const BUDGET_BYTES: usize = 2200;

#[derive(Clone, Debug, Default)]
pub struct Continuity {
    /// Publication history is used only for exact fresh-source deduplication.
    pub reports: Vec<CorpusItem>,
    pub previous_score: Option<i16>,
    pub study: Option<crate::evidence::memory_studies::Study>,
}

pub use crate::evidence::memory_studies::{Finding, Receipt};

#[derive(Serialize)]
pub(super) struct MemoryReport<'a> {
    reported_headline: &'a str,
    sources: Vec<MemorySource<'a>>,
}

#[derive(Serialize)]
struct MemorySource<'a> {
    publisher: &'a str,
    published_at: String,
}

#[derive(Serialize)]
pub(super) struct MemoryGroup<'a> {
    article_population: &'static str,
    from: String,
    before: String,
    distinct_recorded_articles: usize,
    publisher_article_counts: &'a [crate::evidence::memory_studies::PublisherCount],
    reports: Vec<MemoryReport<'a>>,
}

/// Counts describe the selected stored reporting population, not confirmations
/// or an inferred event. Retain source headlines instead of generated titles.
pub(super) fn context(findings: &[Finding]) -> Vec<MemoryGroup<'_>> {
    findings
        .iter()
        .map(|f| MemoryGroup {
            article_population: if f.topic == "requested_pair" {
                "articles mentioning both requested entities"
            } else if f.topic.starts_with("storyline/") {
                "articles indexed to a story group"
            } else {
                "one canonical article"
            },
            from: utc_timestamp(f.from),
            before: utc_timestamp(f.before),
            distinct_recorded_articles: f.article_count,
            publisher_article_counts: &f.publishers,
            reports: reporting_context(f),
        })
        .collect()
}

/// Identical headlines share text, but every source/date remains attached. This
/// is lossless presentation deduplication, not a new corroboration calculation.
fn reporting_context(finding: &Finding) -> Vec<MemoryReport<'_>> {
    let mut reports: Vec<MemoryReport<'_>> = Vec::new();
    for report in &finding.reports {
        let source = MemorySource {
            publisher: &report.publisher,
            published_at: utc_timestamp(report.reported_at),
        };
        if let Some(existing) = reports
            .iter_mut()
            .find(|r| r.reported_headline == report.headline)
        {
            existing.sources.push(source);
        } else {
            reports.push(MemoryReport {
                reported_headline: &report.headline,
                sources: vec![source],
            });
        }
    }
    reports
}

pub(super) fn select(
    memory: &Continuity,
    fresh: &[CorpusItem],
    _now: i64,
    fits: impl Fn(&[Finding]) -> bool,
) -> Vec<Finding> {
    let Some(oldest) = fresh.iter().filter_map(|r| r.published_at_epoch).min() else {
        return Vec::new();
    };
    let Some(study) = &memory.study else {
        return Vec::new();
    };
    if study.receipt.before > oldest {
        return Vec::new();
    }
    let mut selected = Vec::new();
    for finding in &study.findings {
        if finding.reports.iter().any(|r| {
            r.reported_at >= oldest
                || fresh
                    .iter()
                    .any(|f| f.id == r.article_id || f.id == r.canonical_id)
        }) {
            continue;
        }
        let mut candidate = selected.clone();
        candidate.push(finding.clone());
        if serde_json::to_vec(&context(&candidate))
            .expect("memory serializes")
            .len()
            <= BUDGET_BYTES
            && fits(&candidate)
        {
            selected = candidate;
            if selected.len() == MAX_GROUPS {
                break;
            }
        }
    }
    selected
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
    let prepared = super::cognition::prepare(subject.clone(), fresh.to_vec(), &continuity, now)?;
    if let Some(before) = prepared
        .selected
        .iter()
        .filter_map(|r| r.published_at_epoch)
        .filter(|t| *t <= now)
        .min()
    {
        let ids = fresh.iter().map(|r| r.id).collect::<Vec<_>>();
        continuity.study = Some(
            crate::evidence::memory_studies::reporting(
                pool,
                subject,
                before - LOOKBACK_SECONDS,
                before,
                &ids,
                MAX_GROUPS,
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
        .bind(super::manifest::MANIFEST.id.as_str()).bind(now - super::cognition::LOOKBACK_SECONDS).bind(now)
        .fetch_all(pool).await?;
    let previous_score: Option<i16> = sqlx::query_scalar(
        "SELECT card_score FROM news_summaries WHERE entity_type=$1 AND entity_id=$2 \
         AND sport=$3 AND body IS NOT NULL AND generated_at >= to_timestamp($4::double precision) \
         ORDER BY generated_at DESC,id DESC LIMIT 1",
    )
    .bind(&subject.entity_type)
    .bind(subject.entity_id)
    .bind(&subject.sport)
    .bind(now - super::cognition::LOOKBACK_SECONDS)
    .fetch_optional(pool)
    .await?
    .flatten();
    let mut reports = rows
        .into_iter()
        .map(|r| CorpusItem {
            id: r.get("article_id"),
            title: r.get("headline"),
            context: r.get("context_text"),
            source: r.get("source"),
            published_at_epoch: r.get("published_at_epoch"),
        })
        .collect::<Vec<_>>();
    reports.sort_by(|a, b| {
        b.published_at_epoch
            .cmp(&a.published_at_epoch)
            .then(b.id.cmp(&a.id))
    });
    reports.truncate(256);
    Ok(Continuity {
        reports,
        previous_score,
        study: None,
    })
}
