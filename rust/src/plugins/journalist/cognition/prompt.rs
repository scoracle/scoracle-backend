//! The articulation manual. Each input keeps its own owner; this module names only
//! the pieces present in this stateless assignment and explains how they fit.
pub(super) const FRESH_TASK: &str =
    "Articulate each fresh item in its matching report_key, with the supplied voice and form.";

const HISTORY_TASK: &str = "The input is an articulation package.
meta identifies the entity.
fresh is the new source-backed reporting; each item has its output report_key.
memories contains earlier source-backed reporting explicitly attached to a fresh item by report_key.
voice describes how to articulate it.
form describes the output structure.
Articulate each fresh item in its matching report_key, using its attached memories where present. A report with no attached memories is articulated from its fresh item alone. Add no history and no claim that is not supplied.";

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
