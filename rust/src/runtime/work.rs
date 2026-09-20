//! Durable per-entity work queue over `pipeline_work`.
//!
//! Row lifecycle:
//!   enqueue  → 'pending'                (idempotent; reopens on a changed input)
//!   claim    → 'running'                (FOR UPDATE SKIP LOCKED; uniquely fenced lease)
//!   complete → row deleted              (only by its current claim)
//!   fail     → 'failed' + backoff       (retryable until MAX_ATTEMPTS, then dead-letter)
//!   requeue_stale: 'running' → 'pending' (recover a crashed worker's lease)

use crate::util::truncate;
use anyhow::{anyhow, Context, Result};
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;

/// Derivation stage stored on a `pipeline_work` item.
///
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Article reader and downstream fan-out.
    Editor,
    /// Candidate-keyed entity discovery.
    InvestigateEntity,
    FixtureBoxscore,
    Graph,
    /// The Scout's stats rail.
    Rating,
    Momentum,
    Transfers,
    Narratives,
    Vibe,
    Sigil,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Editor => "editor",
            Stage::InvestigateEntity => "investigate_entity",
            Stage::FixtureBoxscore => "fixture_boxscore",
            Stage::Graph => "graph",
            Stage::Rating => "rating",
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
    /// FIFO except for ranked articles and team-first product cards.
    fn claim_order(self) -> &'static str {
        match self {
            // The Editor drains best-first: when a backlog exists, order decides which
            // articles get a model call, and Google already ranked them.
            Stage::Editor => {
                "(SELECT a.feed_rank FROM public.news_articles a WHERE a.id = pipeline_work.entity_id) \
                 ASC NULLS LAST, available_at"
            }
            // Product cards prioritize the bounded team set before the larger player tail.
            Stage::Narratives
            | Stage::Vibe
            | Stage::Sigil
            | Stage::Rating
            | Stage::Momentum
            | Stage::Transfers => {
                "CASE entity_type WHEN 'team' THEN 0 ELSE 1 END, available_at"
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
/// `work.Item`. `input_version` is the desired revision on enqueue and the exact
/// running revision captured on claim. `claim_token` is present only on a claimed item.
#[derive(Clone, Debug)]
pub struct Item {
    pub stage: Stage,
    pub entity_type: String, // "player" | "team" | "article" | "fixture"
    pub entity_id: i64,
    pub sport: String,
    pub input_version: Option<String>,
    pub attempts: i32, // failures so far (populated by claim)
    pub claim_token: Option<String>,
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

    pub(crate) fn require_claim_token(&self) -> Result<&str> {
        self.claim_token.as_deref().ok_or_else(|| {
            anyhow!(
                "{} {}/{} is not a claimed work item",
                self.stage,
                self.entity_type,
                self.entity_id
            )
        })
    }
}

/// Lock and validate the exact lease before a claim-aware publisher writes anything. Inference
/// happens before this short transaction; the lock is held only across publication bookkeeping.
pub(crate) async fn lock_claim(tx: &mut Transaction<'_, Postgres>, it: &Item) -> Result<bool> {
    let claim_token = it.require_claim_token()?;
    let owned: Option<bool> = sqlx::query_scalar(
        r#"
        SELECT TRUE
          FROM pipeline_work
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
           AND claim_token = $5::uuid
           AND running_input_version IS NOT DISTINCT FROM $6
         FOR UPDATE
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(claim_token)
    .bind(it.input_version.as_deref())
    .fetch_optional(&mut **tx)
    .await
    .with_context(|| {
        format!(
            "lock claim {} {}/{}",
            it.stage, it.entity_type, it.entity_id
        )
    })?;
    Ok(owned.is_some())
}

/// Delete an exact claim inside a product publication transaction. Call after [`lock_claim`].
pub(crate) async fn complete_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    it: &Item,
) -> Result<bool> {
    let claim_token = it.require_claim_token()?;
    let result = sqlx::query(
        r#"
        DELETE FROM pipeline_work
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
           AND claim_token = $5::uuid
           AND running_input_version IS NOT DISTINCT FROM $6
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(claim_token)
    .bind(it.input_version.as_deref())
    .execute(&mut **tx)
    .await
    .with_context(|| {
        format!(
            "complete transaction {} {}/{}",
            it.stage, it.entity_type, it.entity_id
        )
    })?;
    Ok(result.rows_affected() == 1)
}

/// Maximum failures before work stops retrying; matches the Go queue policy.
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
    let rows: Vec<(String, i64, String, Option<String>, i32, String)> = sqlx::query_as(&format!(
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
           SET status = 'running',
               running_input_version = w.input_version,
               claim_token = gen_random_uuid(),
               updated_at = NOW()
          FROM ready r
         WHERE w.stage = $1
           AND w.entity_type = r.entity_type
           AND w.entity_id = r.entity_id
           AND w.sport = r.sport
        RETURNING w.entity_type, w.entity_id::bigint, w.sport,
                  w.running_input_version, w.attempts, w.claim_token::text
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
            |(entity_type, entity_id, sport, input_version, attempts, claim_token)| Item {
                stage,
                entity_type,
                entity_id,
                sport,
                input_version,
                attempts,
                claim_token: Some(claim_token),
            },
        )
        .collect())
}

/// Claim priority and dependency order of the six voices:
///
///   1. `Narratives` — The Journalist reads the corpus and depends on no other voice
///   2. `Vibe`       — The Influencer reads those stories for their emotional charge
///   3. `Rating`     — The Scout reads the stat rail, independent of the news rail
///   4. `Transfers`  — The Insider reads the vetted wire
///   5. `Momentum`   — The Analyst CONSUMES the Scout's card and the Influencer's
///   6. `Sigil`      — The Oracle CONSUMES all five pillars, so it is terminal
///
/// Per-stage caps prevent this priority from becoming a starvation ladder.
pub const VOICE_ORDER: [Stage; 6] = [
    Stage::Narratives,
    Stage::Vibe,
    Stage::Rating,
    Stage::Transfers,
    Stage::Momentum,
    Stage::Sigil,
];

/// The five pillar stages the Oracle reads before it can crown an entity — one per character:
/// `narratives` (The Journalist), `rating` (The Scout), `vibe` (The Influencer), `momentum`
/// (The Analyst), `transfers` (The Insider).
pub const PILLAR_STAGES: [Stage; 5] = [
    Stage::Narratives,
    Stage::Rating,
    Stage::Vibe,
    Stage::Momentum,
    Stage::Transfers,
];

/// True when no pillar stage still owes this entity work — the Oracle's completion barrier.
///
/// Reconcile after a publisher commits exact completion. The last completing pillar observes
/// no outstanding rows and offers Oracle; `enqueue` coalesces concurrent offers.
///
/// `status = 'failed'` counts as SETTLED at every attempt level. This is the existing partial-read
/// policy: a retryable failure may temporarily leave one card missing, while a later successful
/// retry offers Oracle again; a terminal dead-letter also cannot silence the other five voices.
/// `application::oracle::load_pillars` tolerates either kind of missing product.
pub async fn pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
) -> Result<bool> {
    let stages: Vec<&str> = PILLAR_STAGES.iter().map(|s| s.as_str()).collect();

    let settled: bool = sqlx::query_scalar(
        r#"
        SELECT NOT EXISTS (
            SELECT 1
              FROM pipeline_work
             WHERE entity_type = $1
               AND entity_id   = $2
               AND sport       = $3
               AND stage       = ANY($4)
               AND status <> 'failed'
        )
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(&stages)
    .fetch_one(pool)
    .await
    .with_context(|| format!("pillars_settled {entity_type}/{entity_id}"))?;

    Ok(settled)
}

/// fail marks a leased item 'failed', records the cause, bumps attempts, and
/// schedules a backoff before it is claimable again. At `max_attempts` the row
/// is parked far in the future — a visible dead-letter, not an infinite retry.
/// Acts only on the caller's current claim. Returns false for a stale execution.
pub async fn fail(
    pool: &PgPool,
    it: &Item,
    cause: &str,
    backoff: Duration,
    max_attempts: i32,
) -> Result<bool> {
    let claim_token = it.require_claim_token()?;
    let result = sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'failed',
               attempts = attempts + 1,
               last_error = $7,
               updated_at = NOW(),
               available_at = CASE
                   WHEN attempts + 1 >= $8 THEN NOW() + INTERVAL '100 years'
                   ELSE NOW() + make_interval(secs => $9)
               END
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
           AND claim_token = $5::uuid
           AND running_input_version IS NOT DISTINCT FROM $6
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(claim_token)
    .bind(it.input_version.as_deref())
    .bind(truncate(cause, 2000))
    .bind(max_attempts)
    .bind(backoff.as_secs_f64()) // make_interval(secs => float8) — no overload ambiguity
    .execute(pool)
    .await
    .with_context(|| format!("fail {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(result.rows_affected() == 1)
}

/// Returns a progressing item to pending without an attempt penalty.
/// The row remains visible to the Oracle barrier and the note is stored in `last_error`.
///
/// **The caller owes a progress guarantee.** `attempts` does not move, so nothing in this function
/// bounds the number of rounds: an item that defers without resolving anything defers forever.
/// Defer only after durable progress, and fall back to [`fail`]'s ladder when a round achieved
/// nothing.
pub async fn defer(pool: &PgPool, it: &Item, delay: Duration, note: &str) -> Result<bool> {
    let claim_token = it.require_claim_token()?;
    let result = sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'pending',
               available_at = NOW() + make_interval(secs => $7),
               updated_at = NOW(),
               last_error = $8
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
           AND claim_token = $5::uuid
           AND running_input_version IS NOT DISTINCT FROM $6
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(claim_token)
    .bind(it.input_version.as_deref())
    .bind(delay.as_secs_f64())
    .bind(truncate(note, 2000))
    .execute(pool)
    .await
    .with_context(|| format!("defer {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(result.rows_affected() == 1)
}

/// release returns a leased item to 'pending' with no attempt penalty — the
/// shutdown path hands unprocessed claims straight back so the next boot picks
/// them up immediately instead of waiting out stale-lease recovery. Acts only
/// on the caller's current claim. Returns false for a stale execution.
pub async fn release(pool: &PgPool, it: &Item) -> Result<bool> {
    let claim_token = it.require_claim_token()?;
    let result = sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'pending', updated_at = NOW(), available_at = NOW()
         WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
           AND status = 'running'
           AND claim_token = $5::uuid
           AND running_input_version IS NOT DISTINCT FROM $6
        "#,
    )
    .bind(it.stage.as_str())
    .bind(it.entity_type.as_str())
    .bind(it.entity_id)
    .bind(it.sport.as_str())
    .bind(claim_token)
    .bind(it.input_version.as_deref())
    .execute(pool)
    .await
    .with_context(|| format!("release {} {}/{}", it.stage, it.entity_type, it.entity_id))?;
    Ok(result.rows_affected() == 1)
}

/// requeue_stale flips 'running' rows whose lease has expired (updated_at older
/// than `lease`) back to 'pending', recovering work abandoned by a crashed
/// worker. Returns the number of rows recovered.
pub async fn requeue_stale(pool: &PgPool, lease: Duration) -> Result<u64> {
    let res = sqlx::query(
        r#"
        UPDATE pipeline_work
           SET status = 'pending',
               running_input_version = NULL,
               claim_token = NULL,
               updated_at = NOW(),
               available_at = NOW()
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
pub async fn enqueue<'e>(
    pool: impl sqlx::Executor<'e, Database = Postgres>,
    it: &Item,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        VALUES ($1, $2, $3, $4, 'pending', $5, NOW(), NOW())
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status        = 'pending',
            attempts      = 0,
            -- A still-pending row keeps its place in the FIFO (mirrors Go): restamping
            -- to NOW() sent every re-noticed entity to the back of the line, starving
            -- the hottest entities behind quiet ones that aged to the front.
            available_at  = CASE WHEN pipeline_work.status = 'pending'
                                 THEN pipeline_work.available_at
                                 ELSE NOW() END,
            updated_at    = NOW(),
            last_error    = NULL,
            input_version = EXCLUDED.input_version,
            running_input_version = NULL,
            claim_token = NULL
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

    #[test]
    fn claim_sensitive_operations_require_a_claimed_item() {
        let item = Item {
            stage: Stage::Vibe,
            entity_type: "team".to_string(),
            entity_id: 7,
            sport: "ZZ_TEST".to_string(),
            input_version: Some("v1".to_string()),
            attempts: 0,
            claim_token: None,
        };
        assert!(item.require_claim_token().is_err());

        let claimed = Item {
            claim_token: Some("00000000-0000-0000-0000-000000000007".to_string()),
            ..item
        };
        assert_eq!(
            claimed.require_claim_token().unwrap(),
            "00000000-0000-0000-0000-000000000007"
        );
    }

    /// The barrier waits on exactly the five pillars — one per character. Sigil must never be in
    /// the list: it is what the barrier RELEASES, and including it would make the Oracle wait on
    /// itself and never crown anything.
    #[test]
    fn pillar_stages_are_the_five_characters_and_exclude_sigil() {
        let names: Vec<&str> = PILLAR_STAGES.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            names,
            vec!["narratives", "rating", "vibe", "momentum", "transfers"]
        );
        assert!(!PILLAR_STAGES.contains(&Stage::Sigil));
    }
}

/// Exact queue-contract acceptance against an isolated, migrated Postgres test database. These
/// tests opt in through TEST_DATABASE_URL; ordinary unit runs do not need a database. They
/// exercise the production SQL rather than a simulated state machine.
#[cfg(test)]
mod postgres_claim_fencing_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated database with migration 256 applied");
        PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect TEST_DATABASE_URL")
    }

    fn pending(stage: Stage, entity_id: i64, sport: &str, revision: &str) -> Item {
        Item {
            stage,
            entity_type: "team".to_string(),
            entity_id,
            sport: sport.to_string(),
            input_version: Some(revision.to_string()),
            attempts: 0,
            claim_token: None,
        }
    }

    async fn clean(pool: &PgPool, sport: &str) {
        sqlx::query("DELETE FROM pipeline_work WHERE sport = $1")
            .bind(sport)
            .execute(pool)
            .await
            .expect("clean claim-fencing test rows");
    }

    async fn complete(pool: &PgPool, item: &Item) -> Result<bool> {
        let mut tx = pool.begin().await?;
        let completed = complete_in_transaction(&mut tx, item).await?;
        tx.commit().await?;
        Ok(completed)
    }

    async fn one_claim(pool: &PgPool, stage: Stage) -> Item {
        let mut items = claim(pool, stage, 1).await.expect("claim test row");
        assert_eq!(items.len(), 1, "isolated test stage should have one row");
        items.remove(0)
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn reclaimed_lease_fences_every_older_acknowledgement() {
        let pool = pool().await;
        let sport = "ZZ_RUST_STALE_CLAIM";
        clean(&pool, sport).await;

        let offered = pending(Stage::Graph, 9_100_001, sport, "v1");
        enqueue(&pool, &offered).await.expect("enqueue v1");
        let old = one_claim(&pool, Stage::Graph).await;

        sqlx::query(
            "UPDATE pipeline_work SET updated_at = NOW() - INTERVAL '1 hour' \
             WHERE stage = 'graph' AND entity_id = $1 AND sport = $2",
        )
        .bind(offered.entity_id)
        .bind(sport)
        .execute(&pool)
        .await
        .expect("age old claim");
        assert_eq!(
            requeue_stale(&pool, Duration::from_secs(30 * 60))
                .await
                .expect("recover old claim"),
            1
        );
        let current = one_claim(&pool, Stage::Graph).await;
        assert_ne!(old.claim_token, current.claim_token);

        assert!(!complete(&pool, &old).await.expect("stale complete"));
        assert!(!fail(&pool, &old, "late", Duration::ZERO, MAX_ATTEMPTS)
            .await
            .expect("stale fail"));
        assert!(!defer(&pool, &old, Duration::ZERO, "late")
            .await
            .expect("stale defer"));
        assert!(!release(&pool, &old).await.expect("stale release"));
        assert!(complete(&pool, &current).await.expect("current complete"));

        clean(&pool, sport).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL"]
    async fn newer_revision_survives_older_execution_and_becomes_next_claim() {
        let pool = pool().await;
        let sport = "ZZ_RUST_RUNNING_REVISION";
        clean(&pool, sport).await;

        let v1 = pending(Stage::FixtureBoxscore, 9_100_002, sport, "v1");
        enqueue(&pool, &v1).await.expect("enqueue v1");
        let old = one_claim(&pool, Stage::FixtureBoxscore).await;

        let v2 = pending(Stage::FixtureBoxscore, v1.entity_id, sport, "v2");
        enqueue(&pool, &v2).await.expect("enqueue v2 during v1");
        assert!(!complete(&pool, &old).await.expect("stale v1 complete"));
        assert!(!fail(&pool, &old, "late v1", Duration::ZERO, MAX_ATTEMPTS)
            .await
            .expect("stale v1 fail"));

        let current = one_claim(&pool, Stage::FixtureBoxscore).await;
        assert_eq!(current.input_version.as_deref(), Some("v2"));
        assert_ne!(old.claim_token, current.claim_token);
        assert!(complete(&pool, &current).await.expect("complete v2"));

        clean(&pool, sport).await;
    }
}

#[cfg(test)]
mod claim_order_tests {
    use super::Stage;

    /// Every stage that writes a card a subscriber reads drains teams first.
    ///
    /// 2026-08-22: Narratives, Vibe and Sigil had this and their team cards were current;
    /// Rating, Momentum and Transfers did not and their team cards were up to six days stale
    /// behind 8,416 queued items, most of them player-grain. The split in behaviour matched
    /// the split in this function exactly.
    #[test]
    fn the_product_stages_all_drain_teams_first() {
        for s in [
            Stage::Narratives,
            Stage::Vibe,
            Stage::Sigil,
            Stage::Rating,
            Stage::Momentum,
            Stage::Transfers,
        ] {
            assert!(
                s.claim_order()
                    .starts_with("CASE entity_type WHEN 'team' THEN 0"),
                "{s} writes a card and must drain teams first"
            );
        }
        // The Editor still drains best-first: its budget is finite and Google already ranked
        // the articles, so rank beats grain there.
        assert!(Stage::Editor.claim_order().contains("feed_rank"));
    }
}

#[cfg(test)]
mod voice_order_tests {
    use super::{Stage, PILLAR_STAGES, VOICE_ORDER};

    /// The order is a dependency order, so the two consumers must sit behind their producers.
    #[test]
    fn consumers_register_after_everything_they_read() {
        let pos = |s: Stage| {
            VOICE_ORDER
                .iter()
                .position(|x| *x == s)
                .expect("in VOICE_ORDER")
        };

        // The Analyst reads the Scout's card and the Influencer's.
        assert!(pos(Stage::Momentum) > pos(Stage::Rating));
        assert!(pos(Stage::Momentum) > pos(Stage::Vibe));

        // The Oracle reads all five pillars, so it is last outright.
        assert_eq!(pos(Stage::Sigil), VOICE_ORDER.len() - 1);
        for p in PILLAR_STAGES {
            assert!(pos(Stage::Sigil) > pos(p), "the Oracle must run after {p}");
        }

        // And the three voices with no voice-dependencies lead.
        assert_eq!(
            [VOICE_ORDER[0], VOICE_ORDER[1], VOICE_ORDER[2]],
            [Stage::Narratives, Stage::Vibe, Stage::Rating]
        );
    }

    /// Every pillar is a voice, and every voice but the Oracle is a pillar.
    #[test]
    fn the_roster_matches_the_pillars() {
        for p in PILLAR_STAGES {
            assert!(
                VOICE_ORDER.contains(&p),
                "{p} is a pillar and must be ordered"
            );
        }
        assert_eq!(VOICE_ORDER.len(), PILLAR_STAGES.len() + 1);
    }
}
