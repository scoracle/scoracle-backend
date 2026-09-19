//! Seats still migrating to Studio and transitional application adapters.
//! Analyst, Influencer, Scout, Journalist, Oracle, and Insider creation live in Studio with IO in
//! `application/`. Editor, Investigator, and Graph still have model-facing work here. Retire this
//! directory once those three seats and adapters move.

pub mod editor;
pub mod graph;
pub mod investigator;

pub use crate::composition::form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
