//! Narrow durable follow-up recovery for claim-aware application publication.
//!
//! `vibe_completed` reconciles the Influencer's Momentum offer and then asks the Oracle barrier.
//! `momentum_completed` needs only that Oracle barrier. These are the two concrete, idempotent
//! obligations represented here; the event is deleted only after its dispatch succeeds.

use crate::runtime::harness::Harness;
use crate::runtime::util::truncate;
use crate::runtime::work::{retry_backoff, Item};
use anyhow::{bail, Context, Result};
use sqlx::{Postgres, Row, Transaction};
use tracing::{debug, warn};

const VIBE_COMPLETED: &str = "vibe_completed";
const MOMENTUM_COMPLETED: &str = "momentum_completed";

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
        ON CONFLICT (kind, source_stage, source_claim_token) DO NOTHING
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
pub async fn drain(hx: &Harness, limit: usize) -> Result<usize> {
    let mut handled = 0;
    for _ in 0..limit {
        let mut tx = hx.pool.begin().await.context("begin outbox dispatch")?;
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
        .bind([VIBE_COMPLETED, MOMENTUM_COMPLETED])
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

        let result = dispatch(hx, &event).await;
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

async fn dispatch(hx: &Harness, event: &Event) -> Result<()> {
    match event.kind.as_str() {
        VIBE_COMPLETED => dispatch_vibe_completed(hx, event).await,
        MOMENTUM_COMPLETED => dispatch_oracle_barrier(hx, event).await,
        kind => bail!("unsupported application outbox kind {kind:?}"),
    }
}

async fn dispatch_vibe_completed(hx: &Harness, event: &Event) -> Result<()> {
    if !crate::application::analyst::enqueue_momentum_if_needed(
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
    dispatch_oracle_barrier(hx, event).await
}

async fn dispatch_oracle_barrier(hx: &Harness, event: &Event) -> Result<()> {
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
