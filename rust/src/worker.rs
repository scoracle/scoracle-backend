//! The Cognition Harness worker — a durable `pipeline_work` consumer.
//!
//! On start and on every wakeup it recovers stale leases, then drains each
//! registered stage to empty. Wakeups come from a Postgres LISTEN on
//! `pipeline_work_ready` (fired by the migration-103 trigger) plus a periodic
//! safety-net tick. Mirrors the Go `internal/derive` worker
//! (`go/internal/derive/worker.go`).
//!
//! Phase 0 safety property: with NO handlers registered, `tick` short-circuits
//! before touching the queue — the scaffold only connects, pings Ollama, and
//! LISTENs. It performs zero writes, so it is safe to run against any DB while
//! you review the foundation.

use crate::ollama::OllamaClient;
use crate::stage::StageHandler;
use crate::work::{self, BACKOFF, CLAIM_BATCH, MAX_ATTEMPTS};
use anyhow::Result;
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const NOTIFY_CHANNEL: &str = "pipeline_work_ready";

pub struct Worker {
    pool: PgPool,
    ollama: OllamaClient,
    handlers: Vec<Box<dyn StageHandler>>,
    safety_net: Duration,
    stale_lease: Duration,
}

impl Worker {
    pub fn new(
        pool: PgPool,
        ollama: OllamaClient,
        handlers: Vec<Box<dyn StageHandler>>,
        safety_net: Duration,
        stale_lease: Duration,
    ) -> Self {
        Self {
            pool,
            ollama,
            handlers,
            safety_net,
            stale_lease,
        }
    }

    /// Run until ctrl-c. Drains on start, on NOTIFY, and on the safety-net tick.
    pub async fn run(&self) -> Result<()> {
        let stages: Vec<&'static str> = self.handlers.iter().map(|h| h.stage().as_str()).collect();
        info!(?stages, "cognition harness worker starting");
        if stages.is_empty() {
            warn!("no stage handlers registered — worker idles (Phase 0 scaffold)");
        }

        // Recover + drain once on boot.
        self.tick("startup").await;

        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(NOTIFY_CHANNEL).await?;
        info!(channel = NOTIFY_CHANNEL, "listening for work notifications");

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("shutdown signal received");
                    return Ok(());
                }
                _ = tokio::time::sleep(self.safety_net) => {
                    self.tick("safety-net").await;
                }
                recv = listener.recv() => {
                    match recv {
                        Ok(_note) => self.tick("notify").await,
                        Err(e) => {
                            error!(error = %e, "listener error; reconnecting in 1s");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            match PgListener::connect_with(&self.pool).await {
                                Ok(mut relisten) => match relisten.listen(NOTIFY_CHANNEL).await {
                                    Ok(()) => listener = relisten,
                                    Err(e) => error!(error = %e, "re-listen failed"),
                                },
                                Err(e) => error!(error = %e, "listener reconnect failed"),
                            }
                        }
                    }
                }
            }
        }
    }

    /// One recover-then-drain cycle. No-op when no handlers are registered, so
    /// the scaffold never mutates the queue.
    async fn tick(&self, cause: &str) {
        if self.handlers.is_empty() {
            debug!(cause, "tick: no handlers registered; nothing to do");
            return;
        }
        match work::requeue_stale(&self.pool, self.stale_lease).await {
            Ok(n) if n > 0 => info!(recovered = n, cause, "requeued stale work"),
            Ok(_) => {}
            Err(e) => error!(error = %e, cause, "requeue stale failed"),
        }
        self.drain_all(cause).await;
    }

    /// Drain every registered stage to empty. Iterates in registration order;
    /// when stages are added, register them in dependency order (transfers,
    /// narratives, vibe, sigil) to match the Go `DrainAll` sequence.
    async fn drain_all(&self, cause: &str) {
        for handler in &self.handlers {
            let stage = handler.stage();
            loop {
                let items = match work::claim(&self.pool, stage, CLAIM_BATCH).await {
                    Ok(items) => items,
                    Err(e) => {
                        error!(error = %e, %stage, cause, "claim failed");
                        break;
                    }
                };
                if items.is_empty() {
                    break;
                }
                debug!(%stage, n = items.len(), cause, "draining batch");
                for item in &items {
                    match handler.handle(&self.pool, &self.ollama, item).await {
                        Ok(()) => {
                            if let Err(e) = work::complete(&self.pool, item).await {
                                error!(error = %e, %stage, "complete failed");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, %stage, entity = item.entity_id, "handler failed; backing off");
                            if let Err(e2) =
                                work::fail(&self.pool, item, &e.to_string(), BACKOFF, MAX_ATTEMPTS).await
                            {
                                error!(error = %e2, %stage, "fail bookkeeping failed");
                            }
                        }
                    }
                }
            }
        }
    }
}
