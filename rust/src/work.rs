//! Durable per-entity derivation work queue — the Rust client for the
//! `pipeline_work` table (migration 102). Mirrors the Go `internal/work`
//! package (`go/internal/work/work.go`) exactly so a Rust worker is a drop-in
//! queue consumer alongside — or in place of — the Go Drainer.
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
/// trigger enqueuing the per-entity derive stages (Plan §8, L6 option (i)). Only the Rust
/// `ScrubHandler` drains it — the Go Drainer has no scrub handler, so there is no double-claim.
///
/// `Momentum` is intentionally NOT a queue stage. It is its own served product: the rating and
/// vibe trajectories over time, assembled from persisted rating/vibe series rather than drained as
/// a standalone worker task. Sigil reads that trajectory as one of its pillars.
///
/// `Headlines` is a queue stage because it derives a News component. It is not a public product
/// pillar separate from News.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scrub,
    Headlines,
    Transfers,
    Narratives,
    Vibe,
    Sigil,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Scrub => "scrub",
            Stage::Headlines => "headlines",
            Stage::Transfers => "transfers",
            Stage::Narratives => "narratives",
            Stage::Vibe => "vibe",
            Stage::Sigil => "sigil",
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
    pub entity_type: String, // "player" | "team"
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

/// Drainer policy, matching the Go defaults.
pub const CLAIM_BATCH: i64 = 10;
pub const MAX_ATTEMPTS: i32 = 5;
pub const BACKOFF: Duration = Duration::from_secs(30 * 60);

/// claim atomically leases up to `limit` ready rows for a stage, marking them
/// 'running'. Ready = pending|failed with `available_at <= now` (a failed row
/// is retried once its backoff elapses). Concurrent claimers receive disjoint
/// rows via FOR UPDATE SKIP LOCKED.
///
/// The Go version wraps this in an explicit transaction; a single CTE UPDATE
/// with FOR UPDATE SKIP LOCKED is already atomic under auto-commit, so we run
/// it directly against the pool.
pub async fn claim(pool: &PgPool, stage: Stage, limit: i64) -> Result<Vec<Item>> {
    let rows: Vec<(String, i64, String, Option<String>, i32)> = sqlx::query_as(
        r#"
        WITH ready AS (
            SELECT entity_type, entity_id, sport
            FROM pipeline_work
            WHERE stage = $1
              AND status IN ('pending', 'failed')
              AND available_at <= NOW()
            ORDER BY available_at
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
        RETURNING w.entity_type, w.entity_id, w.sport, w.input_version, w.attempts
        "#,
    )
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
