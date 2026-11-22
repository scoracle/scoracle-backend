//! Statistical data points and their interpretation rules.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "These statistical data points describe the target this season. Read each action from the target’s point of view. A foul or penalty drawn means an opponent committed it and the target benefited. A foul or penalty against the target means the target was penalized.",
    fields: &[
        ("season", "profile season."),
        ("sample", "target's recorded games/appearances, not peer count or playing time."),
        ("values.facet", "area of play assigned by the rating engine."),
        ("values.label", "display category."),
        ("values.measure", "actual statistic, with its supplied unit/rate."),
        ("values.value", "recorded amount."),
        ("values.better_value", "whether a larger or a smaller raw value is favorable for the target."),
        ("values.percentile", "Scoracle has already accounted for which raw amount is better. A high percentile is favorable for the target, even for a stat where fewer is better. Read each percentile with the measure on the same row."),
        ("values.cohort", "number of eligible members of the comparison group for this measurement."),
        ("values.band", "standing of this measure only."),
        ("values.prior_percentile", "prior-season standing for this measure."),
        ("values.standing_change", "computed rank direction for this measure, not change in ability."),
        ("supports_cross_season", "comparison eligibility, not direction."),
        ("composite", "overall standardized score; peer mean 50, not a percentile or individual measure's band."),
        ("limit.kind", "coverage constraint."),
        ("limit.appearances", "known target sample count."),
        ("limit.minimum", "comparison threshold."),
    ],
};

#[derive(serde::Serialize)]
pub(super) struct Fresh<'a> {
    comparison_group: &'a str,
    #[serde(flatten)]
    profile: &'a super::performance::Profile,
}

pub(super) fn context(parts: &super::request::Parts) -> crate::tools::prompt::Context<Fresh<'_>> {
    let mut guide = GUIDE;
    if parts.subject.sport == "NFL" && parts.subject.entity_type == "team" {
        guide.relationship = "These statistical data points describe the target team this season. Read every row from that team’s point of view. Penalty Yards For (penalty yards drawn) counts yards the opponents lost on their own penalties. More yards here help the target team. Penalty Yards Against counts yards the target team lost on its own penalties. Fewer yards here help the target team.";
    }
    crate::tools::prompt::Context {
        guide,
        data: Fresh {
            comparison_group: match parts.subject.entity_type.as_str() {
                "team" => "teams",
                "player" => "players",
                other => other,
            },
            profile: &parts.profile,
        },
    }
}
