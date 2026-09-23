//! Narrow durable follow-up recovery for claim-aware application publication.
//!
//! Product-bearing Vibe and Rating completions reconcile the Momentum offer and then ask the
//! Oracle barrier. Momentum, a debounced Rating, and a completed Narratives claim need only that
//! Oracle barrier. The event is deleted only after its concrete, idempotent dispatch succeeds.

use crate::application::queue::work::{self, retry_backoff, Item, Stage};
use crate::util::truncate;
use anyhow::{bail, Context, Result};
use sqlx::{Postgres, Row, Transaction};
use tracing::{debug, warn};

const VIBE_COMPLETED: &str = "vibe_completed";
const MOMENTUM_COMPLETED: &str = "momentum_completed";
const RATING_COMPLETED: &str = "rating_completed";
const RATING_DEBOUNCED: &str = "rating_debounced";
const NARRATIVES_COMPLETED: &str = "narratives_completed";
const TRANSFER_PUBLISHED: &str = "transfer_published";
const TRANSFER_IDENTITY_APPLIED: &str = "transfer_identity_applied";

pub(crate) async fn record_vibe_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    record_completion(tx, VIBE_COMPLETED, item)
        .await
        .context("record vibe completion outbox")
}

pub(crate) async fn record_momentum_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    record_completion(tx, MOMENTUM_COMPLETED, item)
        .await
        .context("record momentum completion outbox")
}

pub(crate) async fn record_rating_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    has_product: bool,
) -> Result<()> {
    let kind = if has_product {
        RATING_COMPLETED
    } else {
        RATING_DEBOUNCED
    };
    record_completion(tx, kind, item)
        .await
        .context("record rating completion outbox")
}

pub(crate) async fn record_narratives_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    record_completion(tx, NARRATIVES_COMPLETED, item)
        .await
        .context("record narratives completion outbox")
}

pub(crate) async fn record_transfer_published(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    entity_type: &str,
    entity_id: i32,
    input_version: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO application_outbox (
            kind, source_stage, source_claim_token,
            entity_type, entity_id, sport, source_input_version
        ) VALUES ($1, $2, $3::uuid, $4, $5, $6, $7)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(TRANSFER_PUBLISHED)
    .bind(item.stage.as_str())
    .bind(item.require_claim_token()?)
    .bind(entity_type)
    .bind(entity_id)
    .bind(&item.sport)
    .bind(input_version)
    .execute(&mut **tx)
    .await
    .context("record transfer publication outbox")?;
    Ok(())
}

pub(crate) async fn record_transfer_identity_applied(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    entity_type: &str,
    entity_id: i32,
    rating_input_version: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO application_outbox (
            kind, source_stage, source_claim_token,
            entity_type, entity_id, sport, source_input_version
        ) VALUES ($1, $2, $3::uuid, $4, $5, $6, $7)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(TRANSFER_IDENTITY_APPLIED)
    .bind(item.stage.as_str())
    .bind(item.require_claim_token()?)
    .bind(entity_type)
    .bind(entity_id)
    .bind(&item.sport)
    .bind(rating_input_version)
    .execute(&mut **tx)
    .await
    .context("record transfer identity rating obligation")?;
    Ok(())
}

async fn record_completion(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    item: &Item,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO application_outbox (
            kind, source_stage, source_claim_token,
            entity_type, entity_id, sport, source_input_version
        ) VALUES ($1, $2, $3::uuid, $4, $5, $6, $7)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(kind)
    .bind(item.stage.as_str())
    .bind(item.require_claim_token()?)
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(&item.sport)
    .bind(item.input_version.as_deref())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

struct Event {
    id: String,
    kind: String,
    entity_type: String,
    entity_id: i32,
    sport: String,
    source_input_version: Option<String>,
    attempts: i32,
}

/// Drain at most `limit` ready reconciliation events. Each event holds only its own row lock while
/// the idempotent adapters run. A failure is durably backed off and does not poison later events.
pub async fn drain(pool: &sqlx::PgPool, limit: usize) -> Result<usize> {
    let mut handled = 0;
    for _ in 0..limit {
        let mut tx = pool.begin().await.context("begin outbox dispatch")?;
        let row = sqlx::query(
            r#"
            SELECT id::text, kind, entity_type, entity_id, sport, source_input_version, attempts
              FROM application_outbox
             WHERE kind = ANY($1) AND available_at <= NOW()
             ORDER BY available_at, created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
            "#,
        )
        .bind([
            VIBE_COMPLETED,
            MOMENTUM_COMPLETED,
            RATING_COMPLETED,
            RATING_DEBOUNCED,
            NARRATIVES_COMPLETED,
            TRANSFER_PUBLISHED,
            TRANSFER_IDENTITY_APPLIED,
        ])
        .fetch_optional(&mut *tx)
        .await
        .context("claim application outbox event")?;
        let Some(row) = row else {
            tx.rollback()
                .await
                .context("close empty outbox transaction")?;
            break;
        };
        let event = Event {
            id: row.get(0),
            kind: row.get(1),
            entity_type: row.get(2),
            entity_id: row.get(3),
            sport: row.get(4),
            source_input_version: row.get(5),
            attempts: row.get(6),
        };

        let result = dispatch(pool, &event).await;
        match result {
            Ok(()) => {
                sqlx::query("DELETE FROM application_outbox WHERE id = $1::uuid")
                    .bind(&event.id)
                    .execute(&mut *tx)
                    .await
                    .context("delete dispatched outbox event")?;
                tx.commit().await.context("commit outbox dispatch")?;
                handled += 1;
            }
            Err(error) => {
                let backoff = retry_backoff(event.attempts);
                sqlx::query(
                    r#"
                    UPDATE application_outbox
                       SET attempts = attempts + 1,
                           last_error = $2,
                           available_at = NOW() + make_interval(secs => $3)
                     WHERE id = $1::uuid
                    "#,
                )
                .bind(&event.id)
                .bind(truncate(&format!("{error:#}"), 2000))
                .bind(backoff.as_secs_f64())
                .execute(&mut *tx)
                .await
                .context("back off application outbox event")?;
                tx.commit().await.context("commit outbox backoff")?;
                warn!(
                    error = %format!("{error:#}"),
                    entity_type = %event.entity_type,
                    entity_id = event.entity_id,
                    sport = %event.sport,
                    backoff_secs = backoff.as_secs(),
                    kind = %event.kind,
                    "application completion reconciliation failed; durable retry scheduled"
                );
            }
        }
    }
    Ok(handled)
}

async fn dispatch(pool: &sqlx::PgPool, event: &Event) -> Result<()> {
    match event.kind.as_str() {
        VIBE_COMPLETED | RATING_COMPLETED => dispatch_momentum_then_oracle(pool, event).await,
        MOMENTUM_COMPLETED | RATING_DEBOUNCED | NARRATIVES_COMPLETED | TRANSFER_PUBLISHED => {
            dispatch_oracle_barrier(pool, event).await
        }
        TRANSFER_IDENTITY_APPLIED => dispatch_rating(pool, event).await,
        kind => bail!("unsupported application outbox kind {kind:?}"),
    }
}

async fn dispatch_rating(pool: &sqlx::PgPool, event: &Event) -> Result<()> {
    let input_version = event
        .source_input_version
        .clone()
        .context("transfer identity rating obligation missing input version")?;
    work::enqueue(
        pool,
        &Item {
            stage: Stage::Rating,
            entity_type: event.entity_type.clone(),
            entity_id: i64::from(event.entity_id),
            sport: event.sport.clone(),
            input_version: Some(input_version),
            attempts: 0,
            claim_token: None,
        },
    )
    .await
    .context("dispatch transfer identity rating obligation")
}

async fn dispatch_momentum_then_oracle(pool: &sqlx::PgPool, event: &Event) -> Result<()> {
    if !crate::plugins::analyst::adapter::enqueue_momentum_if_needed(
        pool,
        &event.entity_type,
        event.entity_id,
        &event.sport,
    )
    .await?
    {
        debug!(
            entity_type = %event.entity_type,
            entity_id = event.entity_id,
            sport = %event.sport,
            kind = %event.kind,
            "publication outbox: momentum enqueue skipped unchanged/empty context"
        );
    }
    dispatch_oracle_barrier(pool, event).await
}

async fn dispatch_oracle_barrier(pool: &sqlx::PgPool, event: &Event) -> Result<()> {
    crate::plugins::oracle::adapter::enqueue_oracle_if_pillars_settled(
        pool,
        &event.entity_type,
        i64::from(event.entity_id),
        &event.sport,
        event.source_input_version.clone(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod postgres_recovery_tests {
    use super::*;
    use crate::application::queue::work::Stage;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    const SPORT: &str = "ZZ_OUTBOX_RECOVERY";

    async fn pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(4)
            .connect(&std::env::var("TEST_DATABASE_URL").expect("isolated migrated database"))
            .await
            .unwrap()
    }

    async fn clean(pool: &PgPool) {
        for table in ["application_outbox", "pipeline_work"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE sport = $1"))
                .bind(SPORT)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn committed_obligation(pool: &PgPool) -> Event {
        clean(pool).await;
        sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1, 'Outbox recovery test', 2026) ON CONFLICT DO NOTHING")
            .bind(SPORT).execute(pool).await.unwrap();
        // Recording uses the same production statement as atomic seat publication.
        let item = Item {
            stage: Stage::Momentum,
            entity_type: "team".into(),
            entity_id: 9_600_001,
            sport: SPORT.into(),
            input_version: Some("revision".into()),
            attempts: 0,
            claim_token: Some("00000000-0000-4000-8000-000000000001".into()),
        };
        let mut tx = pool.begin().await.unwrap();
        record_momentum_completed(&mut tx, &item).await.unwrap();
        tx.commit().await.unwrap();
        let id = sqlx::query_scalar("SELECT id::text FROM application_outbox WHERE sport = $1")
            .bind(SPORT)
            .fetch_one(pool)
            .await
            .unwrap();
        Event {
            id,
            kind: MOMENTUM_COMPLETED.into(),
            entity_type: item.entity_type,
            entity_id: item.entity_id as i32,
            sport: item.sport,
            source_input_version: item.input_version,
            attempts: 0,
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
    async fn postgres_dispatch_replays_after_dispatch_before_ack_without_a_model_runtime() {
        let pool = pool().await;
        let event = committed_obligation(&pool).await;
        // Simulate dispatch committed, then process death before event acknowledgement.
        dispatch(&pool, &event).await.unwrap();
        let original: (String, String) = sqlx::query_as(
            "SELECT available_at::text, input_version FROM pipeline_work WHERE sport = $1 AND stage = 'sigil'"
        ).bind(SPORT).fetch_one(&pool).await.unwrap();
        drop(event);
        assert_eq!(drain(&pool, 100).await.unwrap(), 1);
        let replayed: (String, String) = sqlx::query_as(
            "SELECT available_at::text, input_version FROM pipeline_work WHERE sport = $1 AND stage = 'sigil'"
        ).bind(SPORT).fetch_one(&pool).await.unwrap();
        assert_eq!(
            replayed, original,
            "replay must coalesce without changing FIFO or revision"
        );
        assert_eq!(drain(&pool, 100).await.unwrap(), 0);
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
    async fn postgres_dispatch_failure_backs_off_and_recovers_the_durable_event() {
        let pool = pool().await;
        let event = committed_obligation(&pool).await;
        // Fail only this test subject's downstream enqueue, leaving other suites untouched.
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION test_outbox_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.sport = 'ZZ_OUTBOX_RECOVERY' THEN RAISE EXCEPTION 'injected outbox enqueue failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_outbox_failure BEFORE INSERT ON pipeline_work FOR EACH ROW EXECUTE FUNCTION test_outbox_failure();")
            .execute(&pool).await.unwrap();
        let result = drain(&pool, 100).await;
        sqlx::raw_sql("DROP TRIGGER test_outbox_failure ON pipeline_work; DROP FUNCTION test_outbox_failure();")
            .execute(&pool).await.unwrap();
        assert_eq!(result.unwrap(), 0);
        let state: (i32, bool, String) = sqlx::query_as(
            "SELECT attempts, available_at > NOW(), last_error FROM application_outbox WHERE id = $1::uuid"
        ).bind(&event.id).fetch_one(&pool).await.unwrap();
        assert_eq!(state.0, 1);
        assert!(state.1);
        assert!(state.2.contains("injected outbox enqueue failure"));
        sqlx::query("UPDATE application_outbox SET available_at = NOW() WHERE id = $1::uuid")
            .bind(&event.id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(drain(&pool, 100).await.unwrap(), 1);
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport = $1 AND stage = 'sigil' AND status = 'pending'")
            .bind(SPORT).fetch_one(&pool).await.unwrap();
        assert_eq!(pending, 1);
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
    async fn applied_identity_rating_fanout_is_atomic_and_dispatchable() {
        let pool = pool().await;
        clean(&pool).await;
        sqlx::query("INSERT INTO sports (id, display_name, current_season) VALUES ($1, 'Outbox recovery test', 2026) ON CONFLICT DO NOTHING")
            .bind(SPORT).execute(&pool).await.unwrap();
        let item = Item {
            stage: Stage::Transfers,
            entity_type: "team".into(),
            entity_id: 9_600_001,
            sport: SPORT.into(),
            input_version: Some("team-revision".into()),
            attempts: 0,
            claim_token: Some("00000000-0000-4000-8000-000000000002".into()),
        };
        let rating_version = "rating:s2026:transfer:77";

        let mut rolled_back = pool.begin().await.unwrap();
        record_transfer_identity_applied(
            &mut rolled_back,
            &item,
            "player",
            9_600_002,
            rating_version,
        )
        .await
        .unwrap();
        rolled_back.rollback().await.unwrap();
        let absent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM application_outbox WHERE sport=$1 AND kind=$2",
        )
        .bind(SPORT)
        .bind(TRANSFER_IDENTITY_APPLIED)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(absent, 0);

        let mut committed = pool.begin().await.unwrap();
        for (entity_type, entity_id) in [
            ("player", 9_600_002),
            ("team", 9_600_001),
            ("team", 9_600_003),
        ] {
            record_transfer_identity_applied(
                &mut committed,
                &item,
                entity_type,
                entity_id,
                rating_version,
            )
            .await
            .unwrap();
        }
        committed.commit().await.unwrap();
        assert_eq!(drain(&pool, 100).await.unwrap(), 3);
        let work: Vec<(String, i32, String)> = sqlx::query_as(
            "SELECT entity_type, entity_id, input_version FROM pipeline_work WHERE sport=$1 AND stage='rating' ORDER BY entity_type,entity_id",
        )
        .bind(SPORT)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            work,
            vec![
                ("player".into(), 9_600_002, rating_version.into()),
                ("team".into(), 9_600_001, rating_version.into()),
                ("team".into(), 9_600_003, rating_version.into()),
            ]
        );
        clean(&pool).await;
    }

    #[tokio::test]
    #[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
    async fn process_crash_dispatch_rehearsal() {
        const CHILD: &str = "SCORACLE_OUTBOX_CRASH";
        if std::env::var_os(CHILD).is_some() {
            let pool = pool().await;
            let mut tx = pool.begin().await.unwrap();
            let id: String = sqlx::query_scalar(
                "SELECT id::text FROM application_outbox WHERE sport=$1 FOR UPDATE",
            )
            .bind(SPORT)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            let event = Event {
                id,
                kind: MOMENTUM_COMPLETED.into(),
                entity_type: "team".into(),
                entity_id: 9_600_001,
                sport: SPORT.into(),
                source_input_version: Some("revision".into()),
                attempts: 0,
            };
            dispatch(&pool, &event).await.unwrap();
            std::process::exit(86);
        }
        let pool = pool().await;
        committed_obligation(&pool).await;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "application::queue::outbox::postgres_recovery_tests::process_crash_dispatch_rehearsal",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        let before: (String, String) = sqlx::query_as(
            "SELECT available_at::text,input_version FROM pipeline_work WHERE sport=$1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(drain(&pool, 1).await.unwrap(), 1);
        let after: (String, String) = sqlx::query_as(
            "SELECT available_at::text,input_version FROM pipeline_work WHERE sport=$1",
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        clean(&pool).await;
    }
}
