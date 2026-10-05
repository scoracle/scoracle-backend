//! Scout instructions explain the supplied data, relationships and articulation job.
//! Tone lives in Scout's local `voice.rs`; shared form supplies output structure.

pub const RATING_PROMPT_VERSION: &str = "s66-measured-windows";

/// Instructions accompanying every prepared Scout world.
pub const TASK: &str = "\
meta identifies the subject. fresh contains selected season measurements and
their sample and limits. memories contains separate previous and current
measurement windows and dated,
attributed reports. voice sets tone; form sets output shape and limits.

Describe what the supplied measurements establish in compact prose. Use only
supplied numbers and claims. A percentile is relative standing, not ability or
the size of a difference. A prior percentile and standing_change apply only to
that measure; supports_cross_season alone does not establish a comparison.
Missing comparison or measured history is unknown, not stability. A composite
is a standardized overall score with peer mean 50, not a percentile or a
measure of any individual skill.

Each measured window owns its dates, fixture count, measured count and per-match
average. The supplied change compares current with previous, not seasons or
individual games. Missing fixture measurements are unknown, not zero.

Honor any limit. Sample counts describe source coverage, not playing time or
when a season began. Keep reports attributed and dated; a withdrawal retracts
its claim and does not establish recovery. Do not infer a cause, role, tactics,
fitness or future outcome from a measurement or report. A short or null body is
valid when the evidence does not support more.";

/// Retry the same bounded task without silently changing its content policy.
pub fn correction(error: &anyhow::Error) -> Option<String> {
    (error.is::<crate::plugins::support::form::SurfaceError>()
        || error.is::<crate::studio::model::IncompleteOutput>())
        .then(|| format!("{error:#} Return the complete requested JSON within the supplied form limits. Preserve the supplied qualifications and use only the prepared evidence."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correction_preserves_the_provider_failure_under_context() {
        let error = anyhow::Error::new(crate::studio::model::IncompleteOutput(
            "incomplete model output (finish reason: length)".into(),
        ))
        .context("model generate");
        assert!(correction(&error)
            .unwrap()
            .contains("finish reason: length"));
    }
}
