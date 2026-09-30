//! The Scout's articulation manual.
//!
//! Content direction lives here, in the plugin that owns it. The shared
//! `support::prompt::compose` stack is retired: it was four plugins reading one
//! set of writing instructions, which made the instructions shared by default
//! rather than by decision.
//!
//! This manual names the parts actually present in a prepared world and says how
//! they fit. It does not decide what is true — every numeral, band, direction
//! and claim in the world was computed or selected by the plugin before this
//! text was read.

/// The task instruction for a world carrying the given parts.
///
/// One manual serves every shape. The parts present are named where they
/// appear, and the Scout's only non-negotiable rules are about not inventing a
/// measurement and not turning two adjacent facts into a cause.
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
voice describes how to articulate it.
form describes the output structure.

Articulate the supplied profile into a scouting read: what these measurements
establish about how the subject contributes, and what they leave open.

Every number, band, direction and claim must come from the package. Do not
compute a percentile, a band, a rate or a change the package does not state. A
measure absent from `values` was not selected: that is not the same as unmeasured
or zero, and `not_selected` names what was left out.

A percentile describes relative standing among a stated population. It does not
describe the size of a difference, and it does not establish ability. A
prior-season percentile is arithmetic movement in standing, not a change in
ability, role, minutes, fitness or tactics. Where `supports_cross_season` is
false there is no supported direction at all: describe the current standing and
say the comparison is unsupported. Absence of a comparison is not stability.

A `quality_z` is a standardized distance from the peer mean: zero is average,
positive favorable, negative unfavorable. Do not invert a measure whose raw
direction is already accounted for.

A limit in the package is a real boundary on this output. Honour it, and where it
changes what the read may claim, say so. A stored sample is source coverage, not
proof of playing time or of absence from it.

A reported claim is a claim. Keep its publisher, its date and its
qualifications, including a withdrawal and any contradiction. A claim may qualify
an expectation; it does not alter a measurement, and it is not evidence of a
cause for one.";

/// Retry the same bounded task without silently changing its content policy.
pub fn correction(error: &anyhow::Error) -> Option<String> {
    (error.is::<crate::plugins::support::form::SurfaceError>()
        || error.is::<crate::studio::model::IncompleteOutput>())
        .then(|| format!("{error} Return the complete requested JSON within the supplied form limits. Preserve the supplied qualifications and use only the prepared evidence."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_manual_states_the_rules_the_guards_enforce() {
        // These are the claims a reader can lose meaning through. Each one has a
        // production guard or a plugin-side selection behind it, and the manual
        // has to say the same thing in words.
        // The manual is wrapped prose, so a rule can straddle a line break.
        // Fold before matching rather than pinning a line shape.
        let folded = TASK.split_whitespace().collect::<Vec<_>>().join(" ");
        for rule in [
            "that is not the same as unmeasured",
            "does not describe the size of a difference",
            "Absence of a comparison is not stability",
            "is not evidence of a cause",
            "does not alter a measurement",
            "source coverage, not proof of playing time",
        ] {
            assert!(folded.contains(rule), "manual omits: {rule}");
        }
    }
}
