//! The articulation manual. Each input keeps its own owner; this module names only
//! the pieces present in this stateless assignment and explains how they fit.
pub(super) const FRESH_TASK: &str =
    "Articulate each fresh item in its matching report_key, with its supplied voice.";

const HISTORY_TASK: &str = "The input is an articulation package.
identity identifies the entity.
history is source-backed reporting from before the fresh reporting.
fresh is the new source-backed reporting; each item has its output report_key.
voice describes how to articulate it.
form describes the output structure.
Each output report combines the fresh item identified by its report_key with the history that contextualizes that item; history is part of the report text, not a separate output.
Articulate those prepared pieces for the entity in the supplied voice and form.";

pub(super) fn task(has_history: bool) -> &'static str {
    if has_history {
        HISTORY_TASK
    } else {
        FRESH_TASK
    }
}
