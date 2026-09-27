//! Non-editorial lifecycle jobs retained when the Editor worker is removed.
use crate::studio::plugin::ScheduledOperation;
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

pub fn operations(pool: PgPool) -> Vec<Arc<dyn ScheduledOperation>> {
    vec![
        Arc::new(WeekSeal { pool: pool.clone() }),
        Arc::new(ExactTitleDedup { pool }),
    ]
}

struct WeekSeal {
    pool: PgPool,
}

#[async_trait]
impl ScheduledOperation for WeekSeal {
    fn name(&self) -> &'static str {
        "harvester.week-seal"
    }

    async fn run(&self, cause: &'static str) -> Duration {
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
                Err(error) => {
                    error!(sport, cause, error = %format!("{error:#}"), "week seal failed")
                }
            }
        }
        Duration::from_secs(3_600)
    }
}

struct ExactTitleDedup {
    pool: PgPool,
}

#[async_trait]
impl ScheduledOperation for ExactTitleDedup {
    fn name(&self) -> &'static str {
        "harvester.exact-title-dedup"
    }

    async fn run(&self, cause: &'static str) -> Duration {
        match sqlx::query_scalar::<_, i32>(
            "SELECT public.harvester_collapse_exact_title_duplicates(interval '72 hours', 30)",
        )
        .fetch_one(&self.pool)
        .await
        {
            Ok(collapsed) if collapsed > 0 => {
                info!(collapsed, cause, "exact-title duplicates collapsed")
            }
            Ok(_) => debug!(cause, "dedup sweep: nothing to collapse"),
            Err(error) => error!(cause, error = %format!("{error:#}"), "dedup sweep failed"),
        }
        Duration::from_secs(3_600)
    }
}
