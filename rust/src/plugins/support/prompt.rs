//! Shared correction policy and identity framing used by active plugins.

use super::form::SurfaceError;

pub const IDENTITY_CARD_FRAMING: &str = "Identity context, not event evidence. Distinguish current roles from career history; dated reporting may supersede these records. Unknown means unknown.";

pub fn publishing_correction(error: &anyhow::Error) -> Option<String> {
    if error.is::<SurfaceError>() {
        return Some(format!(
            "{error} Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input."
        ));
    }
    if error.is::<crate::studio::model::IncompleteOutput>() {
        return Some("the response ran out of space. Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input.".to_string());
    }
    None
}

pub fn structured_correction(error: &anyhow::Error) -> Option<String> {
    error
        .is::<crate::studio::model::IncompleteOutput>()
        .then(|| "the response was truncated. Return the complete requested JSON object from scratch, preserving the supplied evidence and schema.".to_string())
}
