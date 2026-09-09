//! Model junctions. Six characters produce reader-facing cards; the Editor,
//! Investigator and Graph extract and verify evidence.
//!
//! Each character's `prompt.rs` owns its voice and prompt version. `inputs.rs`
//! prepares evidence and continuity. `form.rs` owns the shared prose form and
//! output contracts; `mod.rs` owns execution, parsing and persistence.
//! The Insider's verification tasks live separately in `insider/verification.rs`.

pub mod analyst;
pub mod editor;
pub mod form;
pub mod graph;
pub mod influencer;
pub mod insider;
pub mod investigator;
pub mod journalist;
pub mod oracle;
pub mod scout;

pub use form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
