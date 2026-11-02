//! Selected measurements and their interpretation rules.

const GUIDE: crate::tools::prompt::Guide = crate::tools::prompt::Guide {
    relationship: "new_content measures the target identified by target. Each measurement keeps its amount, rank and comparison together.",
    fields: &[
        ("season", "profile season."),
        ("sample", "target's recorded games/appearances, not peer count or playing time."),
        ("values.label", "display category."),
        ("values.measure", "actual statistic, with its supplied unit/rate."),
        ("values.value", "recorded amount."),
        ("values.percentile", "relative standing with polarity applied; higher is better, not greater ability."),
        ("values.cohort", "eligible peer count of target.entity_type, not target sample size."),
        ("values.band", "standing of this measure only."),
        ("values.prior_percentile", "prior-season standing for this measure."),
        ("values.standing_change", "computed rank direction for this measure, not change in ability."),
        ("supports_cross_season", "comparison eligibility, not direction."),
        ("composite", "overall standardized score; peer mean 50, not a percentile or individual measure's band."),
        ("limit.kind", "coverage constraint."),
        ("limit.appearances", "known target sample count."),
        ("limit.minimum", "comparison threshold."),
        ("availability", "separate participation reports; their actors and qualifications do not change or explain measurements."),
        ("availability.publisher", "supplying outlet, distinct from speaker."),
        ("availability.headline", "supplied title for verbatim quotation."),
        ("availability.evidence_scope", "available source text only."),
    ],
};

#[derive(serde::Serialize)]
pub(super) struct Fresh<'a> {
    #[serde(flatten)]
    profile: &'a super::performance::Profile,
    availability: serde_json::Value,
}

pub(super) fn context(parts: &super::request::Parts) -> crate::tools::prompt::Context<Fresh<'_>> {
    crate::tools::prompt::Context {
        guide: GUIDE,
        data: Fresh {
            profile: &parts.profile,
            availability: parts.availability.for_writing(),
        },
    }
}
