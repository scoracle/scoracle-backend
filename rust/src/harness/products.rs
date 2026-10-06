//! Latest-product lookups used by application debounce policy.

use anyhow::{Context, Result};
use sqlx::PgPool;

/// EntityKey identifies the row a debounce check is scoped to. `season` is `Some` for
/// season-scoped products (sigil's `sigil_synthesis`) and `None` for entity-scoped ones
/// (vibe_scores, news_summaries).
#[derive(Clone, Debug)]
pub struct EntityKey {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    pub season: Option<i32>,
}

/// Returns true when the entity's latest row already carries this input hash.
///
/// `table` is a stage-controlled literal (never user input), so formatting it into the
/// query carries no injection surface.
pub async fn debounce_unchanged(
    pool: &PgPool,
    table: &str,
    key: &EntityKey,
    hash: &str,
) -> Result<bool> {
    // `query_scalar` over a nullable column gives Option<Option<String>>:
    //   None        → no row for this entity      → don't skip
    //   Some(None)  → latest row has NULL hash    → don't skip (marker)
    //   Some(Some)  → compare to `hash`
    let latest: Option<Option<String>> = if key.season.is_some() {
        let q = format!(
            "SELECT input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
                 ORDER BY generated_at DESC LIMIT 1"
        );
        sqlx::query_scalar(&q)
            .bind(&key.entity_type)
            .bind(key.entity_id)
            .bind(&key.sport)
            .bind(key.season)
            .fetch_optional(pool)
            .await
    } else {
        let q = format!(
            "SELECT input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
                 ORDER BY generated_at DESC LIMIT 1"
        );
        sqlx::query_scalar(&q)
            .bind(&key.entity_type)
            .bind(key.entity_id)
            .bind(&key.sport)
            .fetch_optional(pool)
            .await
    }
    .with_context(|| {
        format!(
            "debounce check {table} {}/{}",
            key.entity_type, key.entity_id
        )
    })?;

    Ok(latest.flatten().as_deref() == Some(hash))
}

/// Loads the latest score and input hash in one consistent read.
/// Missing rows and NULL columns both flatten to `None`.
pub async fn latest_with_hash(
    pool: &PgPool,
    table: &str,
    key: &EntityKey,
) -> Result<(Option<i16>, Option<String>)> {
    let row: Option<(Option<i16>, Option<String>)> = if key.season.is_some() {
        let q = format!(
            "SELECT score, input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
                 ORDER BY generated_at DESC LIMIT 1"
        );
        sqlx::query_as(&q)
            .bind(&key.entity_type)
            .bind(key.entity_id)
            .bind(&key.sport)
            .bind(key.season)
            .fetch_optional(pool)
            .await
    } else {
        let q = format!(
            "SELECT score, input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
                 ORDER BY generated_at DESC LIMIT 1"
        );
        sqlx::query_as(&q)
            .bind(&key.entity_type)
            .bind(key.entity_id)
            .bind(&key.sport)
            .fetch_optional(pool)
            .await
    }
    .with_context(|| {
        format!(
            "latest_with_hash {table} {}/{}",
            key.entity_type, key.entity_id
        )
    })?;
    Ok(row.unwrap_or((None, None)))
}
