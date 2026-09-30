//! Scout instructions explain the supplied data, relationships and articulation job.
//! Tone lives in `voice.rs`; shared form supplies output structure.

pub const RATING_PROMPT_VERSION: &str = "s64";

/// Instructions accompanying every prepared Scout world.
pub const TASK: &str = "\
The input is an articulation package.
identity names the entity this read is about.
fresh holds the current measured profile: the season, the sample it is computed
over, the selected measurements with their percentiles and bands, any compatible
prior-season comparison, and any limit on what the profile supports.
memory holds what is known from before: measured windows from stored fixtures,
and dated injury, suspension or personnel claims. They are different kinds of
knowledge. A measured window is arithmetic over stored fixtures. A reported
claim is what a publisher said, attributed and dated. Neither is evidence of the
other.
rate_standouts are the same measurements under a different rate basis.
trend is the computed recent direction, or absent when it could not be computed.
voice and form are writing instructions, not facts about the subject.
voice supplies tone; form supplies the output structure and limits.

Describe the supplied measured profile and its limits in compact prose.
Use the supplied relationships to describe what the evidence supports.
A partial profile can support a short description; no additional finding is required.

Every number, band, direction and claim must come from the package. Do not
compute a percentile, a band, a rate or a change the package does not state. A
measure absent from `values` was not selected: that is not the same as unmeasured
or zero, and `not_selected` names what was left out.

A percentile describes relative standing among a stated population. It does not
describe the size of a difference, and it does not establish ability. A
prior-season percentile is arithmetic movement in standing, not a change in
ability, role, minutes, fitness or tactics. `standing_change` is the computed
change for that measurement. `supports_cross_season` only says the sample is
eligible: a comparison exists only where a prior percentile is supplied.
Absence of a comparison is not stability. With no trend or measured window,
recent form is unknown. Aggregate outcomes do not establish playing roles,
technique, physical traits, tactical causes or future outcomes.

The composite is an overall standardized score; composite_peer_mean supplies
its scale baseline. It is not points per game or a percentile, and does not identify individual strengths
or establish balanced offense and defense when no measurements are supplied.

A `quality_z` is a standardized distance from the peer mean: zero is average,
positive favorable, negative unfavorable. Do not invert a measure whose raw
direction is already accounted for.

A limit in the package is a real boundary on this output. Honour it, and where it
changes what the read may claim, say so. A stored sample is source coverage, not
proof of playing time or of absence from it. It does not date the beginning
of a season; the minimum in a limit is a threshold, not an observed count.

A reported claim is a claim. Keep its publisher, its date and its
qualifications, including a withdrawal and any contradiction. A claim may qualify
an expectation; it does not alter a measurement, and it is not evidence of a
cause for one.";

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
