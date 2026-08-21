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
    /// The Editor and graph fill the 1070's parallel slots outright, and since the 2026-08-20
    /// consolidation the four voice stages share [`ARCHBOX_SLOTS`] too, each capped below the
    /// group total so one long decode cannot take the whole card.
    ///
    /// For a stage in a [`slot_group`], this is its ceiling *within* that group, not its
    /// guarantee: the group budget binds first.
    fn max_in_flight(&self) -> usize {
        1
    }

    /// Stages that share one backend's parallel slots, as `(group name, total slots)`.
    ///
    /// This exists because a fixed split wastes a card. The Editor and graph both run on Archbox's
    /// gemma3 and were pinned at 2 + 2 to fill its 4 slots — but graph is event-driven and sat at
    /// ZERO pending work for a full day while the Editor had 5,852 items queued and could only use
    /// half the card. The 2-slot cap, not the GPU, was setting the drain rate: ~500 reads/hour at
    /// ~14s each is exactly two slots' worth.
    ///
    /// Grouped stages may each claim up to their own `max_in_flight`, bounded by what is left of
    /// the group's total. So the Editor expands into graph's idle slots and gives them back as
    /// soon as graph wants them — the drain tops up in registration order and graph registers
    /// first, so it takes its slots on the very next pass rather than waiting for the Editor to
    /// drain.
    ///
    /// `None` means ungrouped: the stage's `max_in_flight` is its whole story. Every remote stage
    /// stays that way — sharing out a single-permit host would only deepen the queue behind it.
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        None
    }
}

/// The ministral card on Archbox (1070 Ti) — since the 2026-08-20 consolidation, the whole
/// fleet: The Editor, graph, and the four voices (narratives, vibe, rating, sigil) all share
/// these slots, allocated on demand rather than split down the middle.
///
/// **Four is a longevity choice sitting under a VRAM ceiling (2026-08-20):** the measured
/// derivation (2026-08-09) still stands — at `num_ctx` 4096, weights+overhead plus ~570 MiB of
/// KV per slot puts 6 slots at ~7.2 GiB of the card's 8,192 and **8 would cross it**, spilling
/// layers to CPU silently (the D-T35 failure class; `ollama ps` must say `100% GPU` after any
/// change here). We run 4, not the 6 the VRAM allows: this is a 2017 card carrying every seat,
/// and the protection stack is the 135W power cap + this ceiling + work-driven operation
/// (empty queue = rest; the duty-cycle timers are gone). Raising this beyond the server's
/// `OLLAMA_NUM_PARALLEL` only queues requests inside Ollama; the three knobs — this constant,
/// `COGNITION_BACKEND_CONCURRENCY`'s localhost entry, and the systemd unit's
/// `OLLAMA_NUM_PARALLEL` — move together or not at all.
pub const ARCHBOX_SLOTS: (&str, usize) = ("archbox-3b", 4);
