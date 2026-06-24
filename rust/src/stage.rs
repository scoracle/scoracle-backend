//! StageHandler — the plug-in point for per-stage derivation logic.
//!
//! Phase 0 shipped ZERO handlers; the worker is wired to drain only stages whose handler is
//! registered, so adding a stage is additive and reversible (the Go Drainer keeps owning
//! every other stage in the meantime). Phase 1 implemented the first handler — `vibe` — now
//! re-expressed (L1) as a composition of the capability library's primitives.
//!
//! A handler receives the `Harness` — the capability context (pool + model router + the
//! `extract`/persist/resolve/embed primitives) — generalizing the old `(pool, ollama)` pair.

use crate::harness::Harness;
use crate::work::{Item, Stage};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait StageHandler: Send + Sync {
    /// Which queue stage this handler drains.
    fn stage(&self) -> Stage;

    /// Process one claimed item: read inputs from Postgres (via `hx.pool`), call the model
    /// via the harness primitives (`hx.extract` / the `Router`), persist outputs, and enqueue
    /// any downstream stage (e.g. vibe → sigil). `Ok(())` completes the work row; `Err` fails
    /// it with backoff.
    ///
    /// Contract note for implementers: persist to the SAME tables/columns the Go stage writes,
    /// and reproduce its fail-closed semantics (NULL markers, `is_rumor` NULL → never served,
    /// debounce hashes). The Go stage source is the spec — see
    /// `go/internal/ml/{vibe,news_narratives,transfer,sigil}.go`.
    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()>;
}
