//! SQL-owned descriptive source activity; never confidence or corroboration.
use super::prompt::CorpusItem;
use anyhow::Result;
use serde_json::json;
use sqlx::PgPool;
#[cfg(test)]
use std::collections::HashSet;

pub(super) struct Activity {
    pub score: i32,
    pub components: serde_json::Value,
}
pub(super) struct EditionActivity {
    pub summary: Activity,
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
    let row: (i64, i32, i64, i64, f64, f64, f64, i16) =
        sqlx::query_as(include_str!("activity.sql"))
            .bind(publishers)
            .bind(epochs)
            .bind(now)
            .fetch_one(pool)
            .await?;
    Ok(EditionActivity {
        summary: Activity {
            score: row.1,
            components: json!({"policy":"source-activity-v2","article_count":row.2,
                "distinct_sources":row.3,"volume":row.4,"source_breadth":row.5,"recency":row.6}),
        },
        card_score: row.7,
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
        summary: {
            let (score, components) = reference_score(corpus, now);
            Activity { score, components }
        },
        card_score: reference_score(corpus, now).0.clamp(1, 99) as i16,
    }
}
