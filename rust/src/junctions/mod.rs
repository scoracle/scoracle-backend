//! Seats still migrating to Studio and transitional application adapters.
//! Analyst creation lives in `studio::analyst`; its IO remains here. Influencer creation
//! lives in `studio::influencer` and its IO in `application::influencer`.
//! Editor, Investigator, Graph, Scout, Journalist, Insider and Oracle still have
//! model-facing work here. Retire this directory once all nine seats and adapters move.

pub mod analyst;
pub mod editor;
pub mod graph;
pub mod insider;
pub mod investigator;
pub mod journalist;
pub mod oracle;
pub mod scout;

pub use crate::composition::form::{CLAIM_SELECTION, STORY_FORM, WIRE_COPY};
