//! Source lifecycle jobs owned by headline intake, independent of Editor inference.
use crate::harness::plugin::ScheduledOperation;
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

pub fn operations(pool: PgPool) -> Vec<Arc<dyn ScheduledOperation>> {
    vec![
        Arc::new(StorylineDormancy { pool: pool.clone() }),
        Arc::new(ExactTitleDedup { pool }),
    ]
}

struct StorylineDormancy {
    pool: PgPool,
}

#[async_trait]
impl ScheduledOperation for StorylineDormancy {
    fn name(&self) -> &'static str {
        "harvester.storyline-dormancy"
    }

    async fn run(&self, cause: &'static str) -> Duration {
        // Retained presentation-group lifecycle; existing memberships are untouched.
        match sqlx::query("UPDATE public.storylines SET status='dormant' WHERE status='open' AND last_seen_at < NOW()-INTERVAL '14 days'")
            .execute(&self.pool).await {
            Ok(result) if result.rows_affected() > 0 => info!(dormant=result.rows_affected(), cause, "storylines went dormant"),
            Ok(_) => {},
            Err(error) => error!(cause, error=%format!("{error:#}"), "storyline dormancy failed"),
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
