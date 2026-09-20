//! Queue execution contract. Implementations bind their own application dependencies.

use crate::runtime::work::{Item, Stage};
use anyhow::Result;
use async_trait::async_trait;

/// The durable disposition of an exact queue claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleOutcome {
    Deferred,
    Completed,
    Superseded,
}

#[async_trait]
pub trait WorkHandler: Send + Sync {
    /// Which queue stage this handler drains.
    fn stage(&self) -> Stage;

    /// Publish under the exact claim and return only after commit, deferral, or supersession.
    /// Errors are retried by the worker. Required follow-ups belong in the publication transaction.
    async fn handle(&self, item: &Item) -> Result<HandleOutcome>;

    /// How many items this stage may claim per rotation through the drain. The default of 1 is
    /// right for model work: small claims let the concurrent drain rotate fairly across stages.
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
