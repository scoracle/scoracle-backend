//! Narrow durable follow-up recovery for claim-aware application publication.
//!
//! The first event is `vibe_completed`: reconcile the Influencer's Momentum offer, then ask the
//! Oracle barrier. Both operations are idempotent database adapters. The event is deleted only
//! after both succeed, so a process crash cannot strand a published card without its follow-up.

use crate::runtime::harness::Harness;
use crate::runtime::util::truncate;
use crate::runtime::work::{retry_backoff, Item};
use anyhow::{Context, Result};
use sqlx::{Postgres, Row, Transaction};
use tracing::{debug, warn};

const VIBE_COMPLETED: &str = "vibe_completed";

pub(crate) async fn record_vibe_completed(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO application_outbox (
            kind, source_stage, source_claim_token,
            entity_type, entity_id, sport, source_input_version
        ) VALUES ($1, $2, $3::uuid, $4, $5, $6, $7)
        ON CONFLICT (kind, source_stage, source_claim_token) DO NOTHING
        "#,
    )
    .bind(VIBE_COMPLETED)
    .bind(item.stage.as_str())
    .bind(item.require_claim_token()?)
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(&item.sport)
    .bind(item.input_version.as_deref())
    .execute(&mut **tx)
    .await
    .context("record vibe completion outbox")?;
    Ok(())
}

struct Event {
    id: String,
    entity_type: String,
    entity_id: i32,
    sport: String,
    source_input_version: Option<String>,
    attempts: i32,
}

/// Drain at most `limit` ready reconciliation events. Each event holds only its own row lock while
/// the idempotent adapters run. A failure is durably backed off and does not poison later events.
pub async fn drain(hx: &Harness, limit: usize) -> Result<usize> {
    let mut handled = 0;
    for _ in 0..limit {
        let mut tx = hx.pool.begin().await.context("begin outbox dispatch")?;
        let row = sqlx::query(
            r#"
            SELECT id::text, entity_type, entity_id, sport, source_input_version, attempts
              FROM application_outbox
             WHERE kind = $1 AND available_at <= NOW()
             ORDER BY available_at, created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
            "#,
        )
        .bind(VIBE_COMPLETED)
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
            entity_type: row.get(1),
            entity_id: row.get(2),
            sport: row.get(3),
            source_input_version: row.get(4),
            attempts: row.get(5),
        };

        let result = dispatch_vibe_completed(hx, &event).await;
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
                    "vibe completion reconciliation failed; durable retry scheduled"
                );
            }
        }
    }
    Ok(handled)
}

async fn dispatch_vibe_completed(hx: &Harness, event: &Event) -> Result<()> {
    if !crate::junctions::analyst::enqueue_momentum_if_needed(
        hx,
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
            "vibe outbox: momentum enqueue skipped unchanged/empty context"
        );
    }
    crate::junctions::oracle::enqueue_oracle_if_pillars_settled(
        &hx.pool,
        &event.entity_type,
        i64::from(event.entity_id),
        &event.sport,
        event.source_input_version.clone(),
    )
    .await?;
    Ok(())
}
