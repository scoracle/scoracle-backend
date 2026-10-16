//! The shared coverage clock runs independently of model work and queue emptiness.
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tracing::{error, info};

pub(super) fn start(pool: PgPool, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    match std::env::var("COGNITION_COVERAGE_CONTROLLER").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("false") => return Ok(()),
        Ok("true") => {}
        _ => anyhow::bail!("COGNITION_COVERAGE_CONTROLLER must be true or false"),
    }
    tokio::spawn(async move {
        let mut previous = String::new();
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if shutdown.load(Ordering::Acquire) {
                return;
            }
            match tokio::time::timeout(
                Duration::from_secs(10),
                sqlx::query_scalar::<_, String>("SELECT public.coverage_tick()").fetch_one(&pool),
            )
            .await
            {
                Ok(Ok(phase)) => {
                    if phase != previous {
                        info!(phase, "coverage controller phase");
                        previous = phase;
                    }
                }
                Ok(Err(e)) => error!(error = %e, "coverage tick failed; durable phase retained"),
                Err(_) => error!("coverage tick timed out; durable phase retained"),
            }
        }
    });
    Ok(())
}
