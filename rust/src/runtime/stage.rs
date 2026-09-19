//! StageHandler — the plug-in point for Rust-owned derivation logic.
//!
//! Each handler receives the shared [`Harness`] and owns one queue stage.

use crate::runtime::harness::Harness;
use crate::runtime::work::{Item, Stage};
use anyhow::Result;
use async_trait::async_trait;

/// How a handler left the queue claim. Most legacy handlers still ask the worker to complete
/// after `handle`. Claim-aware publishers instead complete inside their publication transaction,
/// or report that a newer revision superseded the execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleOutcome {
    NeedsCompletion,
    Completed,
    Superseded,
}

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
    /// downstream durable work when the product contract requires it. The claimed item carries a
    /// token and captured revision. Override [`StageHandler::handle_claimed`] when publication
    /// validates that ownership and completes the row in the same transaction.
    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()>;

    /// Process one claim and describe who owns completion. This compatibility seam lets seats
    /// migrate one at a time without weakening the claim-aware path. The default preserves the
    /// existing handler contract; a claim-aware handler must return `Completed` only after its
    /// product, required follow-up intent, and exact claim deletion have committed together.
    async fn handle_claimed(&self, hx: &Harness, item: &Item) -> Result<HandleOutcome> {
        self.handle(hx, item).await?;
        Ok(HandleOutcome::NeedsCompletion)
    }

    /// How many items this stage may claim per rotation through the drain. The default of 1 is
    /// right for any stage whose cost is a model call: the drain is sequential, so a big batch on
    /// a GPU stage would starve every stage behind it.
    ///
    /// Stages without model calls may override this with a larger batch.
    fn rotation_batch(&self) -> i64 {
        1
    }

    /// How many items of THIS stage may be in flight at once under the concurrent drain — the
    /// cap that stops one stage owning the whole `COGNITION_DRAIN_CONCURRENCY` budget.
    ///
    /// This is independent of [`rotation_batch`], which controls claim round-trip size.
    ///
    /// For a stage in a [`slot_group`], this is its ceiling *within* that group, not its
    /// guarantee: the group budget binds first.
    fn max_in_flight(&self) -> usize {
        1
    }

    /// Stages that share one backend's parallel slots, as `(group name, total slots)`.
    ///
    /// Grouped stages share spare capacity up to their own `max_in_flight` limits.
    ///
    /// `None` means ungrouped: the stage's `max_in_flight` is its whole story. Every remote stage
    /// stays that way — sharing out a single-permit host would only deepen the queue behind it.
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        None
    }
}

/// Shared slots for models hosted on Archbox. Keep this aligned with the host's
/// `OLLAMA_NUM_PARALLEL` and configured backend concurrency.
pub const ARCHBOX_SLOTS: (&str, usize) = ("archbox-3b", 4);

/// Shared slots for models hosted on the Mac. Slot-group membership must follow routing.
pub const MAC_SLOTS: (&str, usize) = ("mac-3b", 4);
