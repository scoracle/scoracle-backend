//! Influencer's request and presentation policy for the shared memory tool.
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::memories::{ReportingHistory, Study};
use crate::plugins::meta::EntityMeta;
use anyhow::Result;
use sqlx::PgPool;

pub use crate::plugins::memories::HistoryItem;
pub const LOOKBACK_SECONDS: i64 = 7 * 86400;
pub const BUDGET_BYTES: usize = 1200;
const HISTORY: ReportingHistory = ReportingHistory {
    lookback_seconds: LOOKBACK_SECONDS,
    max_reports: 2,
    budget_bytes: BUDGET_BYTES,
    // The Influencer presents history as a short list of dated headlines and
    // supplies no grouping, so the shared item omits `group` entirely rather
    // than presenting the study's one-article-per-observation default as if it
    // meant something.
    grouped: false,
};

pub fn select(
    study: &Study,
    subject: &EntityMeta,
    source: &SourceContext,
) -> Result<Vec<HistoryItem>> {
    HISTORY.select(
        study,
        subject,
        source.published_at_epoch,
        &[source.article_id],
        |report| {
            super::prompt::source_disposition(
                &report.headline,
                report.reported_at,
                report.reported_at,
            )
            .is_none()
        },
    )
}

pub async fn load(
    pool: &PgPool,
    subject: &EntityMeta,
    source: &SourceContext,
) -> Result<Option<Study>> {
    HISTORY
        .load(
            pool,
            subject,
            source.published_at_epoch,
            &[source.article_id],
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::memories::{Finding, Observation, Receipt};
    #[test]
    fn history_excludes_current_future_wrong_subject_and_over_budget_reporting() {
        let before = 1790553600;
        let subject = EntityMeta {
            name: "Club".into(),
            sport: "FOOTBALL".into(),
            entity_type: "team".into(),
            entity_id: 7,
        };
        let source = SourceContext {
            classification_id: 1,
            article_id: 10,
            headline: "Fresh".into(),
            context: "Fresh".into(),
            source: "Wire".into(),
            published_at_epoch: Some(before),
        };
        let observation = |id, time, headline: &str| Observation {
            article_id: id,
            canonical_id: id,
            topic: "article".into(),
            publisher: "Old Wire".into(),
            reported_at: time,
            headline: headline.into(),
        };
        let mut study = Study {
            receipt: Receipt {
                version: "test".into(),
                subject: subject.clone(),
                from: before - LOOKBACK_SECONDS,
                before,
                input_hash: "test".into(),
                captured_at: before,
                mvcc_snapshot: "test".into(),
                observed_articles: 5,
                included_articles: 0,
                // no include list,
            },
            findings: vec![Finding {
                from: before - LOOKBACK_SECONDS,
                before,
                topic: "article".into(),
                article_count: 5,
                publisher_count: 1,
                publishers: vec![],
                source_ids: vec![],
                reports: vec![
                    observation(10, before - 1, "Current source cannot become history"),
                    observation(11, before, "Future cannot become history"),
                    observation(12, before - LOOKBACK_SECONDS - 1, "Too old"),
                    observation(13, before - 1, &"x".repeat(BUDGET_BYTES + 1)),
                    observation(14, before - 1, "Club announced training"),
                ],
            }],
        };
        let selected = select(&study, &subject, &source).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].reported_headline, "Club announced training");
        study.receipt.subject.entity_id = 8;
        assert!(select(&study, &subject, &source).is_err());
    }
}
