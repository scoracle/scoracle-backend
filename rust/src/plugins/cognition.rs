//! The cognition slot. One kind of thing exists here: a plugin.
//!
//! Every plugin has a manifest, an adapter, parts, an assembly, a cognition slot
//! and a publication path. The slot is the only one that varies *in kind*, and it
//! varies by declared contract rather than by exception. There are three kinds:
//!
//! - [`SlotKind::Decision`] — typed predicates in, bounded probabilities out.
//!   Harvester, through a System 1 model.
//! - [`SlotKind::Prose`] — a prepared world in, prose out. The character
//!   plugins, through an articulation model.
//! - [`SlotKind::Absent`] — deterministic. Fixture Boxscore declares this
//!   rather than leaving the slot undefined.
//!
//! The contract type is the enforcement. A Prose plugin has no Decision slot to
//! put a second eligibility call in, which is what makes "no second eligibility
//! task" structural rather than remembered. The converse is the finding this
//! module exists to make visible: **a plugin that reaches a model without a
//! declared, enforced contract has an undefined model role.** That is the
//! condition recorded against Investigator and Graph.
//!
//! This is a vocabulary, not a framework. The worker does not dispatch through
//! it, it holds no policy, and adding a fourth kind of slot is a plan decision
//! rather than an implementation detail.
use crate::studio::plugin::PluginManifest;
use anyhow::Result;

pub mod decision;
pub mod prose;

/// What a plugin's model call may return, and the enforcement of that limit.
///
/// `Request` and `Response` are the plugin's own types. Only `enforce` is shared:
/// it is the one step where a response is checked against something the plugin
/// declared rather than merely parsed. Preparation stays a plugin function,
/// because what a plugin prepares from is plugin-specific.
pub trait Contract {
    /// The bounded request the plugin prepared.
    type Request: Send;
    /// The permitted response, after enforcement.
    type Response: Send;
    /// Fail closed on anything outside the declared contract.
    fn enforce(&self, request: &Self::Request, response: Self::Response) -> Result<Self::Response>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Decision,
    Prose,
    Absent,
}

/// A plugin's declared slot.
///
/// `enforced` is false when the contract is named but the production path does
/// not yet check every response against it. That state is recorded here rather
/// than left to be discovered, and it requires a `gap` naming the window that
/// closes it, so a slot cannot quietly stay unenforced.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub kind: SlotKind,
    pub enforced: bool,
    /// The window that closes an unenforced contract, or why none is needed.
    pub gap: Option<&'static str>,
}

impl Slot {
    /// Declared and checked on every production path.
    pub const fn enforced(kind: SlotKind) -> Self {
        Self {
            kind,
            enforced: true,
            gap: None,
        }
    }
    /// Named, but not yet checked everywhere. A gap is mandatory.
    pub const fn pending(kind: SlotKind, gap: &'static str) -> Self {
        Self {
            kind,
            enforced: false,
            gap: Some(gap),
        }
    }
}

/// Every registered plugin's declared cognition slot.
///
/// A registered manifest missing from this table has an undefined model role and
/// must not reach a model. `slots_cover_every_registered_plugin` fails when that
/// happens, so the omission is a test failure rather than something a reader has
/// to notice.
pub const SLOTS: &[(&PluginManifest, Slot)] = &[
    (
        &crate::plugins::harvester::manifest::MANIFEST,
        Slot::enforced(SlotKind::Decision),
    ),
    (
        &crate::plugins::fixture_boxscore::manifest::MANIFEST,
        Slot::enforced(SlotKind::Absent),
    ),
    (
        &crate::plugins::journalist::manifest::MANIFEST,
        Slot::enforced(SlotKind::Prose),
    ),
    (
        &crate::plugins::influencer::manifest::MANIFEST,
        Slot::enforced(SlotKind::Prose),
    ),
    // Window 4: F4 unified prose decoder, F5 plugin-owned manual.
    (
        &crate::plugins::scout::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 4 F4/F5"),
    ),
    (
        &crate::plugins::insider::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 5"),
    ),
    (
        &crate::plugins::analyst::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 6"),
    ),
    (
        &crate::plugins::oracle::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 7"),
    ),
    // Output becomes canonical evidence and the contract is only a parser: the
    // slot's model role is not yet defined, which is the finding itself.
    (
        &crate::plugins::investigator::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 8"),
    ),
    (
        &crate::plugins::graph::manifest::MANIFEST,
        Slot::pending(SlotKind::Prose, "Window 10"),
    ),
    (
        &crate::plugins::editor::manifest::MANIFEST,
        Slot::pending(
            SlotKind::Prose,
            "Editor is being pruned; close in Window 10",
        ),
    ),
];

/// The declared slot for a registered plugin.
pub fn slot_for(manifest: &PluginManifest) -> Option<Slot> {
    SLOTS
        .iter()
        .find(|(m, _)| m.id == manifest.id)
        .map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cover_every_registered_plugin() {
        for manifest in crate::application::fleet::ALL {
            let slot = slot_for(manifest).unwrap_or_else(|| {
                panic!(
                    "{} reaches no model and declares no cognition slot",
                    manifest.id
                )
            });
            if !slot.enforced {
                assert!(
                    slot.gap.is_some(),
                    "{} declares an unenforced slot with no window to close it",
                    manifest.id
                );
            }
        }
        // A manifest registered twice would hide a missing declaration.
        assert_eq!(SLOTS.len(), crate::application::fleet::ALL.len());
    }

    #[test]
    fn a_prose_plugin_has_no_decision_slot_to_put_a_second_call_in() {
        // The point of the contract type is that eligibility cannot be added to a
        // character plugin by calling the classifier: the vocabulary does not meet.
        assert_ne!(SlotKind::Prose, SlotKind::Decision);
        let harvester = slot_for(&crate::plugins::harvester::manifest::MANIFEST).unwrap();
        assert_eq!(harvester.kind, SlotKind::Decision);
        let fixture = slot_for(&crate::plugins::fixture_boxscore::manifest::MANIFEST).unwrap();
        assert_eq!(fixture.kind, SlotKind::Absent);
    }
}
