//! Model junctions. Six characters produce reader-facing cards; the Editor,
//! Investigator and Graph extract and verify evidence.
//!
//! `composition` owns character voices, form and entity memories. Junction
//! `inputs.rs` files prepare current evidence; `mod.rs` owns execution, parsing
//! and persistence. Internal extraction prompts remain with their junctions.
//! The Insider's verification tasks live separately in `insider/verification.rs`.

pub mod analyst;
pub mod editor;
pub mod graph;
pub mod influencer;
pub mod insider;
pub mod investigator;
pub mod journalist;
pub mod oracle;
pub mod scout;

pub use crate::composition::form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
