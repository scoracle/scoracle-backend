//! Complete current reporting and native publication deduplication.
use crate::tools::{
    meta::EntityMeta,
    source::{Reporting, SourceContext},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

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
    report_key: String,
    #[serde(flatten)]
    reporting: Reporting<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classifier_world: Option<&'a serde_json::Value>,
}

/// Preserve selected order and complete source text. Durable IDs, hashes,
/// classifications, activity scores and publisher headlines stay in provenance.
///
pub(super) fn reports(reports: &[CorpusItem]) -> Vec<Report<'_>> {
    reports
        .iter()
        .enumerate()
        .map(|(index, report)| Report {
            report_key: format!("report_{}", index + 1),
            reporting: Reporting::new(&report.source, report.published_at_epoch, &report.context),
            classifier_world: report.classifier_world.as_ref(),
        })
        .collect()
}

pub async fn published_reports(
    pool: &PgPool,
    subject: &EntityMeta,
    now: i64,
) -> Result<Vec<CorpusItem>> {
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
            classifier_world: None,
            id: r.get("article_id"),
            title: r.get("headline"),
            context: r.get("context_text"),
            source: r.get("source"),
            published_at_epoch: r.get("published_at_epoch"),
        })
        .collect::<Vec<_>>();
    published_reports.extend(
        crate::plugins::classifier::delivery::load_used(
            pool,
            super::manifest::MANIFEST.id.as_str(),
            &subject.entity_type,
            subject.entity_id,
            &subject.sport,
            now - super::prompt::LOOKBACK_SECONDS,
            now + 1,
        )
        .await?
        .iter()
        .map(CorpusItem::from),
    );
    published_reports.sort_by(|a, b| {
        b.published_at_epoch
            .cmp(&a.published_at_epoch)
            .then(b.id.cmp(&a.id))
    });
    published_reports.truncate(256);
    Ok(published_reports)
}
