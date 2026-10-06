//! SQL-owned descriptive source activity; never confidence or corroboration.
use super::prompt::CorpusItem;
use anyhow::{ensure, Result};
use serde_json::json;
use sqlx::PgPool;
#[cfg(test)]
use std::collections::HashSet;

pub(super) struct Activity {
    pub score: i32,
    pub components: serde_json::Value,
}
pub(super) struct EditionActivity {
    pub reports: Vec<Activity>,
    pub card_score: i16,
}

pub(super) async fn load(
    pool: &PgPool,
    corpus: &[CorpusItem],
    now: i64,
) -> Result<EditionActivity> {
    // Preserve Rust's existing Unicode casing and exact whitespace identity;
    // database collation/lower() must not change what counts as one publisher.
    let publishers = corpus
        .iter()
        .map(|s| s.source.to_lowercase())
        .collect::<Vec<_>>();
    let epochs = corpus
        .iter()
        .map(|s| s.published_at_epoch)
        .collect::<Vec<_>>();
    let rows: Vec<(i64, i32, i64, i64, f64, f64, f64, i16)> =
        sqlx::query_as(include_str!("activity.sql"))
            .bind(publishers)
            .bind(epochs)
            .bind(now)
            .fetch_all(pool)
            .await?;
    ensure!(
        rows.len() == corpus.len() + 1,
        "Journalist activity slots are incomplete"
    );
    let card_score = rows[0].7;
    let mut reports = Vec::with_capacity(corpus.len());
    for (
        index,
        (slot, score, article_count, distinct_sources, volume, source_breadth, recency, _),
    ) in rows.into_iter().enumerate()
    {
        ensure!(
            slot == index as i64,
            "Journalist activity slots changed order"
        );
        if index > 0 {
            reports.push(Activity {score, components: json!({"policy":"source-activity-v2", "article_count":article_count,
                "distinct_sources":distinct_sources,"volume":volume,"source_breadth":source_breadth,"recency":recency})});
        }
    }
    Ok(EditionActivity {
        reports,
        card_score,
    })
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;

#[cfg(test)]
/// Descriptive source activity, not confidence, significance or corroboration.
fn reference_score(corpus: &[CorpusItem], now: i64) -> (i32, serde_json::Value) {
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

#[cfg(test)]
pub(super) fn reference(corpus: &[CorpusItem], now: i64) -> EditionActivity {
    EditionActivity {
        reports: corpus
            .iter()
            .map(|item| {
                let (score, components) = reference_score(std::slice::from_ref(item), now);
                Activity { score, components }
            })
            .collect(),
        card_score: reference_score(corpus, now).0.clamp(1, 99) as i16,
    }
}
