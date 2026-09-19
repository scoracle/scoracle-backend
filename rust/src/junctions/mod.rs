//! Seats still migrating to Studio and transitional application adapters.
//! Analyst, Influencer, Scout, and Journalist creation live in Studio with IO in `application/`.
//! Editor, Investigator, Graph, Insider, and Oracle still have model-facing work here. Retire this
//! directory once those five seats and adapters move.

pub mod editor;
pub mod graph;
pub mod insider;
pub mod investigator;
pub mod oracle;

pub use crate::composition::form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
