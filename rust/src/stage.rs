//! StageHandler — the plug-in point for per-stage derivation logic.
//!
//! Phase 0 ships ZERO handlers; the worker is wired to drain only stages whose
//! handler is registered, so adding a stage is additive and reversible (the Go
//! Drainer keeps owning every other stage in the meantime). Phase 1 implements
//! the first handler here — `vibe` — then registers it in `main.rs`.

use crate::ollama::OllamaClient;
use crate::work::{Item, Stage};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;

#[async_trait]
pub trait StageHandler: Send + Sync {
    /// Which queue stage this handler drains.
    fn stage(&self) -> Stage;

    /// Process one claimed item: read inputs from Postgres, call the model via
    /// `ollama`, persist outputs, and enqueue any downstream stage (e.g. vibe →
    /// sigil). `Ok(())` completes the work row; `Err` fails it with backoff.
    ///
    /// Contract note for implementers: persist to the SAME tables/columns the
    /// Go stage writes, and reproduce its fail-closed semantics (NULL markers,
    /// `is_rumor` NULL → never served, debounce hashes). The Go stage source is
    /// the spec — see `go/internal/ml/{vibe,news_narratives,transfer,sigil}.go`.
    async fn handle(&self, pool: &PgPool, ollama: &OllamaClient, item: &Item) -> Result<()>;
}
