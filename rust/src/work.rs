//! Durable per-entity derivation work queue — the Rust client for the
//! `pipeline_work` table (migration 102). The Go derive worker is retired; this
//! module owns claim/complete/fail/requeue while Go keeps enqueue and operator
//! helpers.
//!
//! Row lifecycle:
//!   enqueue  → 'pending'                (idempotent; reopens on a changed input)
//!   claim    → 'running'                (FOR UPDATE SKIP LOCKED; leased)
//!   complete → row deleted              (only while still 'running')
//!   fail     → 'failed' + backoff       (retryable until MAX_ATTEMPTS, then dead-letter)
//!   requeue_stale: 'running' → 'pending' (recover a crashed worker's lease)

use crate::util::truncate;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::time::Duration;

/// Stage names the derivation step a work item belongs to, held in `pipeline_work`. Most stages are
/// per-entity (player/team); `Scrub` is the exception — it is ARTICLE-keyed (entity_type='article',
/// entity_id=`news_articles.id`) and is the news ID-gate that, on writing `vetted`, fires the mig-103
/// trigger enqueuing the per-entity derive stages (Plan §8, L6 option (i)). `Momentum` is the
/// generated trajectory card over PEAK/Vibe plus deterministic momentum scores. The Rust handlers
/// drain these stages; Go only enqueues/operates queue rows.
///
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scrub,
    ArticleRead,
    FixtureBoxscore,
    Graph,
    Peak,
    Momentum,
    Transfers,
    Narratives,
    Vibe,
    Sigil,
    // `Oracle` retired 2026-07-16 (Session B): the voice is an in-process step of the
    // Sigil stage now. Queue rows with stage='oracle' were swept at the cutover deploy.
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Scrub => "scrub",
            Stage::ArticleRead => "article_read",
            Stage::FixtureBoxscore => "fixture_boxscore",
            Stage::Graph => "graph",
            Stage::Peak => "peak",
            Stage::Momentum => "momentum",
            Stage::Transfers => "transfers",
            Stage::Narratives => "narratives",
            Stage::Vibe => "vibe",
            Stage::Sigil => "sigil",
        }
    }

    /// The ORDER BY used when claiming this stage's work. A `&'static str` spliced into the query —
    /// never user input, so there is nothing to escape.
    ///
    /// FIFO by `available_at` is right for stages whose items are interchangeable. Article reads
    /// are not: the reading budget is finite, so when a backlog exists the order decides which
    /// articles get a model call and which age out. Google already ranked them
    /// (`news_articles.feed_rank`, mig 194), so drain best-first. NULLS LAST keeps pre-migration
    /// backlog rows from displacing a fresh top hit.
    fn claim_order(self) -> &'static str {
        match self {
            Stage::ArticleRead => {
                "(SELECT a.feed_rank FROM public.news_articles a WHERE a.id = pipeline_work.entity_id) \
                 ASC NULLS LAST, available_at"
            }
            _ => "available_at",
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Item identifies one unit of derivation work for an entity. Mirrors the Go
/// `work.Item`. `input_version` is `None` when unused (stored as SQL NULL).
#[derive(Clone, Debug)]
pub struct Item {
    pub stage: Stage,
    pub entity_type: String, // "player" | "team" | "article" | "fixture"
    pub entity_id: i64,
    pub sport: String,
    pub input_version: Option<String>,
    pub attempts: i32, // failures so far (populated by claim)
}

impl Item {
    pub fn entity_id_i32(&self) -> Result<i32> {
        i32::try_from(self.entity_id).with_context(|| {
            format!(
                "{} {}/{} entity_id outside i32 range",
                self.stage, self.entity_type, self.entity_id
            )
        })
    }
}

/// Drainer policy. CLAIM_BATCH and MAX_ATTEMPTS match the Go defaults; the retry
/// ramp below replaces the old flat 30-minute backoff.
pub const CLAIM_BATCH: i64 = 10;
pub const MAX_ATTEMPTS: i32 = 5;

/// retry_backoff ramps a failed item's delay by how many times it has already
/// failed (`Item.attempts` as of the claim, i.e. BEFORE this failure is counted):
/// 30s → 2m → 10m → 30m. Most first failures are transient (a model hiccup, a
/// timed-out await), so the first retry comes fast instead of parking good work
/// for 30 minutes; a persistently failing item still backs off to the old flat
/// ceiling. With MAX_ATTEMPTS = 5 the four live retries walk the whole ramp,
/// then the fifth failure dead-letters.
pub fn retry_backoff(prior_failures: i32) -> Duration {
    match prior_failures {
        i32::MIN..=0 => Duration::from_secs(30),
        1 => Duration::from_secs(2 * 60),
        2 => Duration::from_secs(10 * 60),
        _ => Duration::from_secs(30 * 60),
    }
}

/// claim atomically leases up to `limit` ready rows for a stage, marking them
/// 'running'. Ready = pending|failed with `available_at <= now` (a failed row
/// is retried once its backoff elapses). Concurrent claimers receive disjoint
/// rows via FOR UPDATE SKIP LOCKED.
///
/// The Go version wraps this in an explicit transaction; a single CTE UPDATE
/// with FOR UPDATE SKIP LOCKED is already atomic under auto-commit, so we run
/// it directly against the pool.
pub async fn claim(pool: &PgPool, stage: Stage, limit: i64) -> Result<Vec<Item>> {
    let rows: Vec<(String, i64, String, Option<String>, i32)> = sqlx::query_as(&format!(
        r#"
        WITH ready AS (
            SELECT entity_type, entity_id, sport
            FROM pipeline_work
            WHERE stage = $1
              AND status IN ('pending', 'failed')
              AND available_at <= NOW()
            ORDER BY {}
            FOR UPDATE SKIP LOCKED
            LIMIT $2
        )
        UPDATE pipeline_work w
           SET status = 'running', updated_at = NOW()
          FROM ready r
         WHERE w.stage = $1
           AND w.entity_type = r.entity_type
           AND w.entity_id = r.entity_id
           AND w.sport = r.sport
        RETURNING w.entity_type, w.entity_id::bigint, w.sport, w.input_version, w.attempts
        "#,
        stage.claim_order(),
    ))
    .bind(stage.as_str())
    .bind(limit)
    .fetch_all(pool)
    .await
    .with_context(|| format!("claim {stage}"))?;

    Ok(rows
        .into_iter()
        .map(
            |(entity_type, entity_id, sport, input_version, attempts)| Item {
                stage,
                entity_type,
                entity_id,
                sport,
                input_version,
                attempts,
            },
        )
        .collect())
}

/// complete removes a finished work item — only while still 'running' (the
/// caller holds the lease). If a newer input reopened the row to 'pending'
/// mid-flight, this is a no-op and the reopened work survives for reprocessing.
pub async fn complete(pool: &PgPool, it: &Item) -> Result<()> {
    sqlx::query(
        r#"
        DELETE FROM pipeline_work
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .execute(pool)
    .await
    .with_context(|| format!("complete {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(())
}

/// fail marks a leased item 'failed', records the cause, bumps attempts, and
/// schedules a backoff before it is claimable again. At `max_attempts` the row
/// is parked far in the future — a visible dead-letter, not an infinite retry.
/// Acts only on a row still 'running'.
pub async fn fail(
    pool: &PgPool,
    it: &Item,
    cause: &str,
    backoff: Duration,
    max_attempts: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'failed',
               attempts = attempts + 1,
               last_error = $5,
               updated_at = NOW(),
               available_at = CASE
                   WHEN attempts + 1 >= $6 THEN NOW() + INTERVAL '100 years'
                   ELSE NOW() + make_interval(secs => $7)
               END
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(truncate(cause, 2000))
    .bind(max_attempts)
    .bind(backoff.as_secs_f64()) // make_interval(secs => float8) — no overload ambiguity
    .execute(pool)
    .await
    .with_context(|| format!("fail {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(())
}

/// release returns a leased item to 'pending' with no attempt penalty — the
/// shutdown path hands unprocessed claims straight back so the next boot picks
/// them up immediately instead of waiting out stale-lease recovery. Acts only
/// on a row still 'running'.
pub async fn release(pool: &PgPool, it: &Item) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'pending', updated_at = NOW(), available_at = NOW()
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .execute(pool)
    .await
    .with_context(|| format!("release {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(())
}

/// requeue_stale flips 'running' rows whose lease has expired (updated_at older
/// than `lease`) back to 'pending', recovering work abandoned by a crashed
/// worker. Returns the number of rows recovered.
pub async fn requeue_stale(pool: &PgPool, lease: Duration) -> Result<u64> {
    let res = sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'pending', updated_at = NOW(), available_at = NOW()
         WHERE status = 'running'
           AND updated_at < NOW() - make_interval(secs => $1)
        "#,
    )
    .bind(lease.as_secs_f64())
    .execute(pool)
    .await
    .context("requeue stale")?;
    Ok(res.rows_affected())
}

/// enqueue records that (stage, entity, sport) needs work. Idempotent and safe
/// for downstream hand-offs (e.g. vibe → sigil). Conflict policy mirrors Go: a
/// row is REOPENED to 'pending' only when its input_version changed or it was
/// 'failed'; an unchanged pending/running row is left untouched.
pub async fn enqueue(pool: &PgPool, it: &Item) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        VALUES ($1, $2, $3, $4, 'pending', $5, NOW(), NOW())
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status        = 'pending',
            attempts      = 0,
            available_at  = NOW(),
            updated_at    = NOW(),
            last_error    = NULL,
            input_version = EXCLUDED.input_version
        WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
           OR pipeline_work.status = 'failed'
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(it.input_version.as_deref())
    .execute(pool)
    .await
    .with_context(|| format!("enqueue {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_ramps_then_caps() {
        assert_eq!(retry_backoff(0), Duration::from_secs(30));
        assert_eq!(retry_backoff(1), Duration::from_secs(2 * 60));
        assert_eq!(retry_backoff(2), Duration::from_secs(10 * 60));
        assert_eq!(retry_backoff(3), Duration::from_secs(30 * 60));
        assert_eq!(retry_backoff(4), Duration::from_secs(30 * 60)); // capped
        assert_eq!(retry_backoff(-1), Duration::from_secs(30)); // defensive: never negative-index
    }
}
