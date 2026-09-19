//! Seats still migrating to Studio and transitional application adapters.
//! Analyst, Influencer, Scout, Journalist, and Oracle creation live in Studio with IO in `application/`.
//! Editor, Investigator, Graph, and Insider still have model-facing work here. Retire this
//! directory once those four seats and adapters move.

pub mod editor;
pub mod graph;
pub mod insider;
pub mod investigator;

pub use crate::composition::form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
