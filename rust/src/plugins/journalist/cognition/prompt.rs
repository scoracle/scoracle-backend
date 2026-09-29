//! The articulation manual. Each input keeps its own owner; this module names only
//! the pieces present in this stateless assignment and explains how they fit.
pub(super) const FRESH_TASK: &str =
    "Articulate each fresh item in its matching report_key, with its supplied voice.";

const HISTORY_TASK: &str = "The input is an articulation package.
identity identifies the entity.
fresh is the new source-backed reporting; each item has its output report_key.
a fresh item may carry history: the source-backed reporting from before it that the plugin determined belongs to it.
voice describes how to articulate it.
form describes the output structure.
Articulate each fresh item in its matching report_key, using the history supplied with it where present. A report with no history is articulated from its fresh item alone. Add no history and no claim that is not supplied.";

/// The manual for a prepared package. `has_history` is whether any report
/// carries history; the same text serves the mixed case, because it describes
/// history per report rather than assuming every report has it.
pub(super) fn task(has_history: bool) -> &'static str {
    if has_history {
        HISTORY_TASK
    } else {
        FRESH_TASK
    }
}
