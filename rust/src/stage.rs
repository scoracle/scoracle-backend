//! StageHandler — the plug-in point for Rust-owned derivation logic.
//!
//! The Go layer produces `pipeline_work` rows and serves already-persisted products. The
//! Rust worker is the only live queue consumer that performs model inference. Each handler
//! receives the `Harness`: Postgres pool, model router, and the shared
//! `extract`/persist/resolve/embed primitives.

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
    /// Contract note for implementers: persist to the live product tables with fail-closed
    /// semantics (NULL markers, `is_rumor` NULL -> never served, debounce hashes), then enqueue
    /// downstream durable work when the product contract requires it.
    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()>;

    /// How many items this stage may claim per rotation through the drain. The default of 1 is
    /// right for any stage whose cost is a model call: the drain is sequential, so a big batch on
    /// a GPU stage would starve every stage behind it.
    ///
    /// A stage with NO model call should override this. After the teardown deleted scrub's
    /// relevance gate (§2.1) that stage became pure SQL plus set intersection — microseconds per
    /// item — but it was still being rotated one item at a time behind stages that take minutes,
    /// so a 7,165-item backlog would have drained over weeks rather than seconds. Cheap stages
    /// must not be paced like expensive ones.
    fn rotation_batch(&self) -> i64 {
        1
    }

    /// How many items of THIS stage may be in flight at once under the concurrent drain — the
    /// cap that stops one stage owning the whole `COGNITION_DRAIN_CONCURRENCY` budget.
    ///
    /// It is not the same question as [`rotation_batch`], which is how many rows to claim in one
    /// SQL round trip. A stage can want a big claim batch and still deserve one slot: scrub asks
    /// for 256 rows because a round trip per microsecond-item is absurd, but it must not hold 256
    /// slots — its items do no model work, so every slot it holds is a slot the GPU cannot use.
    ///
    /// Default 1. Raise it only for a stage that must keep MULTIPLE slots of one backend busy:
    /// locally that is The Reader and graph, the two stages on gemma3:4b, which between them have
    /// to fill Archbox's 4 parallel slots (2 + 2). A remote stage should stay at 1 — the Mac runs
    /// one request at a time by design, so extra in-flight items there only queue on its
    /// semaphore, holding leases and burning their handler-timeout clock for no throughput.
    fn max_in_flight(&self) -> usize {
        1
    }
}
