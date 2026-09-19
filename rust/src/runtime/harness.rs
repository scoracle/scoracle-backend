//! Transitional application context: routing, Postgres access, and debounce.
//! Studio is the in-house harness; migrated creation lives in `studio/`.

use crate::runtime::providers::ollama::GenerateOptions;
use crate::runtime::route::{Role, Router};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::time::Duration;

/// Harness — the capability context handed to every stage composition. Built once at boot.
pub struct Harness {
    /// The Postgres pool (builds on `db::build_pool`). The queue host clones this for its own
    /// mechanics; the primitives read it for their corpus loads and provenance writes.
    pub pool: PgPool,
    /// Route primitive — owns the `Inference` backend(s) per role.
    pub router: Router,
    /// Per-item ceiling. Multi-call handlers use it to stop cleanly before cancellation.
    /// `Duration::ZERO` means unbounded.
    pub handler_budget: Duration,
    /// Context window shared by every voice on this host.
    pub voice_num_ctx: i32,
}

// ===========================================================================
// Extract and validate.
// ===========================================================================

pub use crate::studio::{Extracted, Generation, GenerationCall, Parser, Provenance};

impl Harness {
    /// extract is `route(role) → generate(prompt, opts) → parser.parse(response)` in one
    /// call, with the fail-closed contract enforced at the type boundary. A parse failure
    /// surfaces as the parser's `Err` (item fails + backs off); a fail-closed `Ok(None)`
    /// flows through as `Extracted.value == None` for the caller to persist as the marker.
    pub async fn extract<T, P: Parser<T>>(
        &self,
        role: Role,
        prompt: &str,
        opts: &GenerateOptions,
        parser: &P,
    ) -> Result<Extracted<T>> {
        let backend = self.router.for_role(role);
        crate::studio::Studio::new(backend.as_ref())
            .extract(prompt, opts, parser)
            .await
    }
}

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

impl Harness {
    /// Returns true when the entity's latest row already carries this input hash.
    ///
    /// `table` is a stage-controlled literal (never user input), so formatting it into the
    /// query carries no injection surface. vibe does not call this (it has no `input_hash`);
    /// callers control `table`; it is never user input.
    pub async fn debounce_unchanged(
        &self,
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
                .fetch_optional(&self.pool)
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
                .fetch_optional(&self.pool)
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
        &self,
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
                .fetch_optional(&self.pool)
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
                .fetch_optional(&self.pool)
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

    /// latest_row fetches one column from the entity's LATEST row in a product table.
    ///
    /// Load-bearing details:
    /// - the SELECT casts `{column}::text` because sqlx will not decode every product
    ///   column type (for example `sigil_synthesis.score` smallint) as `String`;
    /// - `query_scalar` returns `Option<Option<String>>`, and this deliberately flattens
    ///   no-row and NULL-in-latest-row. That is fine for the latest-value helpers this
    ///   consolidates: both cases mean no skip / no baseline / no last hash. A future
    ///   caller that needs to distinguish those states should use a bespoke query.
    ///
    /// `table` and `column` are stage-controlled literals (never user input), so formatting
    /// them into the query carries no injection surface.
    pub async fn latest_row(
        &self,
        table: &str,
        key: &EntityKey,
        column: &str,
    ) -> Result<Option<String>> {
        let latest: Option<Option<String>> = if key.season.is_some() {
            let q = format!(
                "SELECT {column}::text FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_scalar(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .bind(key.season)
                .fetch_optional(&self.pool)
                .await
        } else {
            let q = format!(
                "SELECT {column}::text FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_scalar(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .fetch_optional(&self.pool)
                .await
        }
        .with_context(|| {
            format!(
                "latest_row {table}.{column} {}/{}",
                key.entity_type, key.entity_id
            )
        })?;
        Ok(latest.flatten())
    }
}
