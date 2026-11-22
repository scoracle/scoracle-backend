//! Generic durable event delivery for claim-aware application publication.
//!
//! Plugins own event vocabulary, production, and reactions. The host persists each event with
//! its source claim, locks it for delivery, retries bounded failures, and deletes it only after
//! the registered idempotent reaction chain succeeds.

use crate::harness::queue::work::{retry_backoff, Item};
use crate::util::truncate;
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{error, warn};

pub(crate) struct NewEvent<'a> {
    pub(crate) kind: &'static str,
    pub(crate) entity_type: &'a str,
    pub(crate) entity_id: i32,
    pub(crate) source_input_version: Option<&'a str>,
}

pub(crate) async fn record(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    event: NewEvent<'_>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO application_outbox (
            kind, source_stage, source_claim_token,
            entity_type, entity_id, sport, source_input_version, edition_date
        ) VALUES ($1, $2, $3::uuid, $4, $5, $6, $7,
            (SELECT edition_date FROM public.coverage_schedule WHERE enabled AND phase='characters'))
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(event.kind)
    .bind(item.stage.as_str())
    .bind(item.require_claim_token()?)
    .bind(event.entity_type)
    .bind(event.entity_id)
    .bind(&item.sport)
    .bind(event.source_input_version)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) struct Event {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) entity_type: String,
    pub(crate) entity_id: i32,
    pub(crate) sport: String,
    pub(crate) source_input_version: Option<String>,
    attempts: i32,
}

#[async_trait]
pub(crate) trait EventReaction: Send + Sync {
    fn name(&self) -> &'static str;
    fn kinds(&self) -> &[&'static str];
    async fn react(&self, event: &Event) -> Result<()>;
}

/// Complete per-event reaction chains available on this host. An event kind is
/// claimable only when composition registered its whole required chain here.
pub struct ReactionRegistry {
    by_kind: BTreeMap<&'static str, Vec<Arc<dyn EventReaction>>>,
}

impl ReactionRegistry {
    pub(crate) fn new(reactions: Vec<Arc<dyn EventReaction>>) -> Result<Self> {
        let mut by_kind: BTreeMap<&'static str, Vec<Arc<dyn EventReaction>>> = BTreeMap::new();
        for reaction in reactions {
            anyhow::ensure!(
                !reaction.kinds().is_empty(),
                "reaction {} has no event kinds",
                reaction.name()
            );
            for kind in reaction.kinds() {
                anyhow::ensure!(
                    !kind.is_empty(),
                    "reaction {} has an empty event kind",
                    reaction.name()
                );
                by_kind.entry(kind).or_default().push(reaction.clone());
            }
        }
        Ok(Self { by_kind })
    }

    pub fn empty() -> Self {
        Self {
            by_kind: BTreeMap::new(),
        }
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.by_kind.keys().copied().collect()
    }

    #[cfg(test)]
    pub(crate) fn reaction_names(&self, kind: &str) -> Vec<&'static str> {
        self.by_kind
            .get(kind)
            .into_iter()
            .flatten()
            .map(|reaction| reaction.name())
            .collect()
    }

    async fn dispatch(&self, event: &Event) -> Result<()> {
        let reactions = self
            .by_kind
            .get(event.kind.as_str())
            .with_context(|| format!("unsupported application outbox kind {:?}", event.kind))?;
        for reaction in reactions {
            reaction
                .react(event)
                .await
                .with_context(|| format!("reaction {} failed", reaction.name()))?;
        }
        Ok(())
    }
}

/// Drain at most `limit` ready reconciliation events. Each event holds only its own row lock while
/// the idempotent adapters run. A failure is durably backed off and does not poison later events.
pub async fn drain(
    pool: &sqlx::PgPool,
    reactions: &ReactionRegistry,
    limit: usize,
) -> Result<usize> {
    let kinds = reactions.kinds();
    if kinds.is_empty() {
        return Ok(0);
    }
    let mut handled = 0;
    for _ in 0..limit {
        let mut tx = pool.begin().await.context("begin outbox dispatch")?;
        let row = sqlx::query(
            r#"
            SELECT id::text, kind, entity_type, entity_id, sport, source_input_version, attempts
              FROM application_outbox
             WHERE kind = ANY($1) AND attempts < 5 AND available_at <= NOW()
             ORDER BY available_at, created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
            "#,
        )
        .bind(&kinds)
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

        let result = reactions.dispatch(&event).await;
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
                if event.attempts + 1 >= 5 {
                    error!(error=%format!("{error:#}"), kind=%event.kind, entity_id=event.entity_id,
                        sport=%event.sport, "handoff retry budget exhausted; failure retained for reporting");
                } else {
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
    }
    Ok(handled)
}

#[cfg(test)]
#[path = "../../tests/internal/queue/outbox.rs"]
mod postgres_recovery_tests;
