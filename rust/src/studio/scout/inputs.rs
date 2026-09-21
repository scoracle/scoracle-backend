//! Prepared evidence and continuity supplied to the Scout.

use super::{
    collect_rate_standouts, format_datapoint_evidence, ordered_facts, RatingProfile, Subject,
};
use std::collections::BTreeMap;

pub(super) const MIN_CROSS_SEASON_APPEARANCES: f64 = 10.0;
pub(super) const MAX_COMPARISON_FACTS: usize = 4;
pub(super) const MAX_HELD_COMPARISON_FACTS: usize = 2;

pub(super) fn sample_appearances(p: &RatingProfile) -> Option<f64> {
    p.sample.iter().find_map(|(label, value)| {
        matches!(
            label.trim().to_ascii_lowercase().as_str(),
            "appearances" | "games played" | "games_played" | "matches played" | "matches_played"
        )
        .then_some(*value)
    })
}

pub(crate) fn supports_cross_season_comparison(p: &RatingProfile) -> bool {
    sample_appearances(p).is_some_and(|n| n >= MIN_CROSS_SEASON_APPEARANCES)
}

pub fn build_stat_prompt(
    subject: &Subject,
    p: &RatingProfile,
    personnel: Option<&str>,
    comparisons: Option<&BTreeMap<String, super::SkillChange>>,
    form_trend: Option<&str>,
    current_reports: Option<&str>,
    memory_context: Option<&str>,
) -> String {
    let mut b = String::new();
    let appearances = sample_appearances(p);
    let thin_sample = appearances.is_some_and(|n| n < MIN_CROSS_SEASON_APPEARANCES);

    let mut header = format!("{} {}", subject.sport, subject.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!(
        "Entity: {} ({header}); season {}\n",
        subject.entity_name, p.season
    ));

    if let Some(context) = memory_context {
        b.push('\n');
        b.push_str(context);
        b.push('\n');
    }

    b.push_str(&format!(
        "Stats updated: {}; sample: {}\n",
        p.observed_at.as_deref().unwrap_or("unknown"),
        if thin_sample {
            "stored thin sample; participation details omitted".into()
        } else {
            render_sample(&p.sample)
        }
    ));
    if let Some(change) = comparisons.and_then(|changes| changes.values().next()) {
        b.push_str(&format!(
            "Comparison: season {}; stats updated: {}; sample: {}\n",
            change.prior_season,
            change.prior_observed_at.as_deref().unwrap_or("unknown"),
            render_sample(&change.prior_sample)
        ));
        b.push_str("Cross-season boundary: these are season-to-date snapshots and their minutes or appearances may cover different windows. Percentile movement describes relative standing only. Do not claim changes in ability, role, minutes, fitness, availability, tactics or opponent plans unless an attributed report states them.\n");

        let mut movements = p
            .breakdown
            .iter()
            .filter_map(|datapoint| {
                let current_pct = datapoint.pct?;
                let change = comparisons?.get(&datapoint.label)?;
                Some((
                    datapoint.label.as_str(),
                    current_pct,
                    change.prior_pct,
                    current_pct - change.prior_pct,
                ))
            })
            .collect::<Vec<_>>();
        movements.sort_by(|a, b| {
            b.3.abs()
                .partial_cmp(&a.3.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
        if !movements.is_empty() {
            b.push_str("Compatible cross-season measurements (same measure; the stated direction is arithmetic relative percentile standing, not ability). Use only these stated directions; a measure absent from this block has no supported direction:\n");
            let shown = movements
                .iter()
                .take(MAX_COMPARISON_FACTS)
                .map(|movement| movement.0)
                .collect::<std::collections::HashSet<_>>();
            for (label, current_pct, prior_pct, delta) in
                movements.iter().copied().take(MAX_COMPARISON_FACTS)
            {
                let direction = if delta > 1.0 {
                    "ROSE"
                } else if delta < -1.0 {
                    "FELL"
                } else {
                    "HELD"
                };
                let current = p
                    .breakdown
                    .iter()
                    .find(|datapoint| datapoint.label == label)
                    .map(format_datapoint_evidence)
                    .unwrap_or_else(|| format!("{label}: current percentile {current_pct:.1}"));
                b.push_str(&format!(
                    "- Direction: {direction}. {current}; prior season percentile {prior_pct:.1}; {}\n",
                    relative_standing(delta)
                ));
            }
            let mut held = movements
                .iter()
                .copied()
                .filter(|movement| movement.3.abs() <= 1.0 && !shown.contains(movement.0))
                .collect::<Vec<_>>();
            held.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(b.0))
            });
            if !held.is_empty() {
                b.push_str("Compatible held anchors (same measure, within one percentile point). Describe these as held; their current quality band is not evidence that they improved or declined:\n");
                for (label, current_pct, prior_pct, delta) in
                    held.into_iter().take(MAX_HELD_COMPARISON_FACTS)
                {
                    let current = p
                        .breakdown
                        .iter()
                        .find(|datapoint| datapoint.label == label)
                        .map(format_datapoint_evidence)
                        .unwrap_or_else(|| format!("{label}: current percentile {current_pct:.1}"));
                    b.push_str(&format!(
                        "- Direction: HELD. {current}; prior season percentile {prior_pct:.1}; {}\n",
                        relative_standing(delta)
                    ));
                }
            }
        }
    }

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nOverall standardized score (50 = average): {comp:.0}\n"
        ));
    }

    b.push_str("Selected evidence: measures omitted from this assignment are not thereby zero or unavailable in the source.\n");
    b.push_str("Quality scale: raw-value direction is stated for each measure. Percentiles and quality z already account for that direction; do not invert them again. Higher quality is better for both positive and negative stats. Quality z is distance from the peer mean in standard deviations: 0 is average, positive favorable, negative unfavorable. For a measure with a supplied quality z, its magnitude describes standardized distance; percentile alone describes relative standing, not the size of the difference. Missing polarity or z stays unknown.\n");

    if subject.sport == "NBA" {
        b.push_str("Values: per-game averages, except percentages.\n");
    } else {
        b.push_str("Values: season totals, except percentages and named adjustments.\n");
    }
    if comparisons.is_none_or(|changes| changes.is_empty()) {
        let identified = p
            .breakdown
            .iter()
            .filter(|datapoint| !datapoint.measure.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        if identified.iter().any(|datapoint| datapoint.pct.is_some()) {
            b.push_str("\nCurrent-snapshot measurements. Percentiles, when present, rank the same measure among eligible entities in this sport and season; higher is better. Missing ranks and season comparisons are unmeasured.\n");
        } else if p.breakdown.is_empty() {
            b.push_str("\nNo current rating measurements are supplied in this assignment.\n");
        } else if identified.is_empty() {
            b.push_str("\nCurrent rating measurements are withheld because their underlying measurement identity is unavailable. Use only the explicitly named measures in the context above.\n");
        } else {
            b.push_str("\nCurrent-snapshot raw measurements. These values are not ranks and have no historical match in this block. Do not describe them as unchanged, improved or declined.\n");
        }
        let rates = collect_rate_standouts(p);
        for d in ordered_facts(&identified) {
            b.push_str("- ");
            b.push_str(&format_datapoint_evidence(&d));
            for rate in rates
                .iter()
                .filter(|r| r.label == d.label && r.measure == d.measure)
            {
                b.push_str(&format!(
                    "; {} percentile {:.1}",
                    rate.mode.replace('_', "-"),
                    rate.pct
                ));
            }
            b.push('\n');
        }
    }

    if let Some(ft) = form_trend.filter(|t| !t.trim().is_empty()) {
        b.push_str(&format!(
            "\nRecent performance trend (computed from recent overall ratings): {ft}\n"
        ));
    } else {
        b.push_str("\nRecent performance trend: unavailable in this assignment; recent direction is unknown, not steady.\n");
    }

    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel and availability since our last read (confirmed records; dates are effective dates; WITHDRAWN retracts a claim, not evidence of recovery; season measurements remain unchanged):\n");
        b.push_str(pc);
    }

    if let Some(ar) = current_reports.filter(|a| !a.trim().is_empty()) {
        b.push_str("\nCurrent attributed reports (performance, roster and availability claims; ⇄ marks contradictions; preserve uncertainty; reports do not alter measured statistics):\n");
        b.push_str(ar);
    }

    let one_appearance_sample = appearances.is_some_and(|n| n <= 1.0);
    if one_appearance_sample {
        b.push_str("\nEvidence boundary for this output: the stored current sample has at most one appearance. It is source coverage, not proof of actual or limited playing time. Attributed reports may describe other fixtures or competitions; do not merge them into the stored appearance or aggregate without a verified fixture link. Do not calculate unstated values or describe improvement, decline or stability across seasons. Center the reading on the separately attributed current actions and state that a directional comparison is unsupported.\n");
    } else if appearances.is_some_and(|n| n < MIN_CROSS_SEASON_APPEARANCES) {
        b.push_str("\nEvidence boundary for this output: the current sample has fewer than 10 appearances, so no cross-season change was computed. Describe the current snapshot and separately attributed reports. Do not claim improvement, decline, stability, changed ability, changed role, reduced minutes, fitness or tactical causes across seasons. Use identity as context for the supported playing characteristics. Make the absence of a supported cross-season comparison clear without prescribing a direction. Never say mid-season. Do not call the identity or affiliation unresolved unless this prompt explicitly says it is unresolved. Do not infer emotion, motive or a psychological effect from a quote or report. Write the sporting read itself; do not mention printed measurements, labels, bands or these instructions. Keep the body at most 800 characters.\n");
    }

    b
}

fn relative_standing(delta: f64) -> String {
    if delta > 1.0 {
        format!("relative standing rose by {delta:.1} percentile points")
    } else if delta < -1.0 {
        format!(
            "relative standing fell by {:.1} percentile points",
            delta.abs()
        )
    } else {
        format!("relative standing held within one percentile point ({delta:+.1})")
    }
}

fn render_sample(sample: &std::collections::BTreeMap<String, f64>) -> String {
    if sample.is_empty() {
        return "unknown".into();
    }
    sample
        .iter()
        .map(|(label, value)| format!("{label} {}", super::trim_float(*value)))
        .collect::<Vec<_>>()
        .join(", ")
}
