//! Editor-owned newsroom maintenance.
//!
//! The durable host supplies only scheduling and shutdown. This module owns the
//! domain work, its cadence, sports, debounce behavior, and diagnostic messages.

use crate::evidence::news::packet;
use crate::studio::plugin::ScheduledOperation;
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const BASELINE_INTERVAL: Duration = Duration::from_secs(60);
const DORMANCY_SWEEP_INTERVAL_SECS: i64 = 3_600;
const COMPILE_BATCH: i64 = 200;

pub(super) fn operations(pool: PgPool, packet_compile: bool) -> Vec<Arc<dyn ScheduledOperation>> {
    vec![
        Arc::new(DeskMaintenance {
            pool: pool.clone(),
            packet_compile,
            last_lifecycle_sweep: AtomicI64::new(0),
        }),
        Arc::new(ExactTitleDedup { pool }),
    ]
}

struct DeskMaintenance {
    pool: PgPool,
    packet_compile: bool,
    last_lifecycle_sweep: AtomicI64,
}

#[async_trait]
impl ScheduledOperation for DeskMaintenance {
    fn name(&self) -> &'static str {
        "editor.desk"
    }

    async fn run(&self, cause: &'static str) -> Duration {
        let now = unix_seconds();
        let last = self.last_lifecycle_sweep.load(Ordering::Acquire);
        if last == 0 || now.saturating_sub(last) >= DORMANCY_SWEEP_INTERVAL_SECS {
            self.last_lifecycle_sweep.store(now, Ordering::Release);
            match super::storyline::mark_dormant(&self.pool).await {
                Ok(n) if n > 0 => info!(dormant = n, cause, "storylines went dormant"),
                Ok(_) => debug!(cause, "dormancy sweep: nothing quiet enough"),
                Err(e) => error!(error = %format!("{e:#}"), cause, "dormancy sweep failed"),
            }

            // These are explicit domain subjects, not generic scheduler policy.
            for sport in ["FOOTBALL", "NBA", "NFL"] {
                match sqlx::query_as::<_, (i32, i32)>(
                    "SELECT closing_enqueued, weeks_sealed FROM public.seal_weeks($1)",
                )
                .bind(sport)
                .fetch_one(&self.pool)
                .await
                {
                    Ok((enqueued, sealed)) if enqueued > 0 || sealed > 0 => info!(
                        sport,
                        closing_enqueued = enqueued,
                        weeks_sealed = sealed,
                        cause,
                        "week seal advanced"
                    ),
                    Ok(_) => debug!(sport, cause, "week seal: nothing to close"),
                    Err(e) => debug!(
                        sport,
                        error = %format!("{e:#}"),
                        cause,
                        "week seal unavailable (mig 241 not applied?)"
                    ),
                }
            }
        }

        if self.packet_compile {
            match packet::compile_dirty(&self.pool, COMPILE_BATCH).await {
                Ok(n) if n > 0 => info!(packets = n, cause, "packets compiled"),
                Ok(_) => debug!(cause, "packet compile: nothing dirty and quiet"),
                Err(e) => error!(error = %format!("{e:#}"), cause, "packet compile failed"),
            }
        }

        let velocity = match storyline_velocity(&self.pool).await {
            Ok(velocity) => velocity,
            Err(e) => {
                warn!(
                    error = %format!("{e:#}"),
                    "failed to query storyline velocity, using baseline interval"
                );
                5
            }
        };
        let interval = velocity_adaptive_interval(velocity);
        debug!(
            velocity,
            interval_secs = interval.as_secs(),
            "desk loop cadence"
        );
        interval
    }
}

struct ExactTitleDedup {
    pool: PgPool,
}

#[async_trait]
impl ScheduledOperation for ExactTitleDedup {
    fn name(&self) -> &'static str {
        "editor.exact-title-dedup"
    }

    async fn run(&self, cause: &'static str) -> Duration {
        // 72h matches the novelty gate. Anything older was already swept.
        let result: Result<i32, sqlx::Error> = sqlx::query_scalar(
            "SELECT public.collapse_exact_title_duplicates(interval '72 hours', 30)",
        )
        .fetch_one(&self.pool)
        .await;
        match result {
            Ok(n) if n > 0 => info!(collapsed = n, cause, "exact-title duplicates collapsed"),
            Ok(_) => debug!(cause, "dedup sweep: nothing to collapse"),
            Err(e) => error!(error = %format!("{e:#}"), cause, "dedup sweep failed"),
        }
        Duration::from_secs(3_600)
    }
}

async fn storyline_velocity(pool: &PgPool) -> anyhow::Result<i64> {
    let count = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM storylines
        WHERE last_seen_at > NOW() - INTERVAL '1 hour'
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

fn velocity_adaptive_interval(velocity: i64) -> Duration {
    match velocity {
        value if value > 10 => Duration::from_secs(120),
        value if value < 3 => Duration::from_secs(30),
        _ => BASELINE_INTERVAL,
    }
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::velocity_adaptive_interval;
    use std::time::Duration;

    #[test]
    fn storyline_velocity_owns_the_desk_cadence() {
        assert_eq!(velocity_adaptive_interval(0), Duration::from_secs(30));
        assert_eq!(velocity_adaptive_interval(2), Duration::from_secs(30));
        assert_eq!(velocity_adaptive_interval(3), Duration::from_secs(60));
        assert_eq!(velocity_adaptive_interval(10), Duration::from_secs(60));
        assert_eq!(velocity_adaptive_interval(11), Duration::from_secs(120));
    }
}
