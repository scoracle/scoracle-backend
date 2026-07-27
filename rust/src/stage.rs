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
    /// to fill Archbox's 4 parallel slots. A remote stage should stay at 1 — the Mac runs
    /// one request at a time by design, so extra in-flight items there only queue on its
    /// semaphore, holding leases and burning their handler-timeout clock for no throughput.
    ///
    /// For a stage in a [`slot_group`], this is its ceiling *within* that group, not its
    /// guarantee: the group budget binds first.
    fn max_in_flight(&self) -> usize {
        1
    }

    /// Stages that share one backend's parallel slots, as `(group name, total slots)`.
    ///
    /// This exists because a fixed split wastes a card. The Reader and graph both run on Archbox's
    /// gemma3 and were pinned at 2 + 2 to fill its 4 slots — but graph is event-driven and sat at
    /// ZERO pending work for a full day while the Reader had 5,852 items queued and could only use
    /// half the card. The 2-slot cap, not the GPU, was setting the drain rate: ~500 reads/hour at
    /// ~14s each is exactly two slots' worth.
    ///
    /// Grouped stages may each claim up to their own `max_in_flight`, bounded by what is left of
    /// the group's total. So the Reader expands into graph's idle slots and gives them back as
    /// soon as graph wants them — the drain tops up in registration order and graph registers
    /// first, so it takes its slots on the very next pass rather than waiting for the Reader to
    /// drain.
    ///
    /// `None` means ungrouped: the stage's `max_in_flight` is its whole story. Every remote stage
    /// stays that way — sharing out a single-permit host would only deepen the queue behind it.
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        None
    }
}

/// The gemma3 card on Archbox, shared by The Reader and graph. Four parallel slots
/// (`max_concurrent=4`), allocated on demand rather than split down the middle.
pub const ARCHBOX_GEMMA_SLOTS: (&str, usize) = ("archbox-gemma3", 4);
