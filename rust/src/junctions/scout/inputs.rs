//! Evidence and continuity supplied to the character.

use super::{
    collect_rate_standouts, format_datapoint_evidence, ordered_facts, AvailabilityChange,
    PersonnelChange, RatingProfile, RatingReq,
};
use crate::junctions::editor::render::MarkedClaim;

pub fn render_scout_reports(claims: &[MarkedClaim]) -> Option<String> {
    if claims.is_empty() {
        return None;
    }
    let mut b = String::new();
    for c in claims {
        let mark = if c.marked { "⇄ " } else { "- " };
        b.push_str(&format!(
            "{mark}{}: {}\n",
            c.claim.source,
            c.claim.fact.trim()
        ));
    }
    Some(b)
}

pub fn render_personnel_block(
    entity_type: &str,
    entity_id: i32,
    changes: &[PersonnelChange],
    total: usize,
    avail: &[AvailabilityChange],
    avail_total: usize,
) -> Option<String> {
    if changes.is_empty() && avail.is_empty() {
        return None;
    }
    let mut b = String::new();
    for c in changes {
        let event = c
            .event_type
            .as_deref()
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        let line = match (entity_type, c.kind.as_str()) {
            ("player", "reverted") => format!(
                "{}: earlier move to {} REVERTED — that move is not in force{event}.",
                c.date_label,
                c.new_team.as_deref().unwrap_or("another club")
            ),
            ("player", _) => match c.old_team.as_deref() {
                Some(old) => format!(
                    "{}: current-club identity confirmed as {} (previously {old}){event}.",
                    c.date_label,
                    c.new_team.as_deref().unwrap_or("a new club")
                ),
                None => format!(
                    "{}: current-club identity confirmed as {}{event}.",
                    c.date_label,
                    c.new_team.as_deref().unwrap_or("a new club")
                ),
            },
            ("team", "reverted") => format!(
                "{}: {}'s move REVERTED — that move is not in force{event}.",
                c.date_label, c.player_name
            ),
            ("team", _) if c.new_team_id == Some(entity_id) => match c.old_team.as_deref() {
                Some(old) => format!(
                    "{}: {}'s current-club identity confirmed here (previously {old}){event}.",
                    c.date_label, c.player_name
                ),
                None => format!(
                    "{}: {}'s current-club identity confirmed here{event}.",
                    c.date_label, c.player_name
                ),
            },
            ("team", _) => match c.new_team.as_deref() {
                Some(new) => format!(
                    "{}: {}'s current-club identity confirmed as {new}{event}.",
                    c.date_label, c.player_name
                ),
                None => format!(
                    "{}: {}'s current-club identity changed{event}.",
                    c.date_label, c.player_name
                ),
            },
            _ => continue,
        };
        b.push_str("- ");
        b.push_str(&line);
        b.push('\n');
    }
    let personnel_rendered = !b.is_empty();
    if personnel_rendered && total > changes.len() {
        b.push_str(&format!(
            "- (+{} older personnel changes in this window, not shown)\n",
            total - changes.len()
        ));
    }

    let before_availability = b.len();
    for a in avail {
        let expected = a
            .expected_return_label
            .as_deref()
            .map(|d| format!(" — reported back around {d}"))
            .unwrap_or_default();
        let line = match (entity_type, a.kind.as_str()) {
            ("player", "opened") => format!(
                "{}: out with a recorded {}{expected}.",
                a.event_date_label, a.event_kind
            ),
            ("player", "returned") => format!(
                "{}: available again after the {} recorded {}.",
                a.date_label, a.event_kind, a.event_date_label
            ),
            ("player", _) => format!(
                "{}: the {} recorded {} was WITHDRAWN — that record is not in force.",
                a.date_label, a.event_kind, a.event_date_label
            ),
            ("team", "opened") => format!(
                "{}: {} out with a recorded {}{expected}.",
                a.event_date_label, a.player_name, a.event_kind
            ),
            ("team", "returned") => format!(
                "{}: {} available again after the {} recorded {}.",
                a.date_label, a.player_name, a.event_kind, a.event_date_label
            ),
            ("team", _) => format!(
                "{}: {}'s {} recorded {} was WITHDRAWN — that record is not in force.",
                a.date_label, a.player_name, a.event_kind, a.event_date_label
            ),
            _ => continue,
        };
        b.push_str("- ");
        b.push_str(&line);
        b.push('\n');
    }
    if b.len() > before_availability && avail_total > avail.len() {
        b.push_str(&format!(
            "- (+{} older availability events in this window, not shown)\n",
            avail_total - avail.len()
        ));
    }

    if b.is_empty() {
        return None;
    }
    Some(b)
}

pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    personnel: Option<&str>,
    comparisons: Option<&std::collections::HashMap<String, super::SkillChange>>,
    form_trend: Option<&str>,
    current_reports: Option<&str>,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();

    let mut header = format!("{} {}", req.sport, req.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!(
        "Entity: {} ({header}); season {}\n",
        req.entity_name, p.season
    ));

    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    b.push_str(&format!(
        "Stats updated: {}; sample: {}\n",
        p.observed_at.as_deref().unwrap_or("unknown"),
        render_sample(&p.sample)
    ));
    if let Some(change) = comparisons.and_then(|changes| changes.values().next()) {
        b.push_str(&format!(
            "Comparison: season {}; stats updated: {}; sample: {}\n",
            change.prior_season,
            change.prior_observed_at.as_deref().unwrap_or("unknown"),
            render_sample(&change.prior_sample)
        ));
    }

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nOverall standardized score (50 = average): {comp:.0}\n"
        ));
    }

    if req.sport == "NBA" {
        b.push_str("Values: per-game averages, except percentages.\n");
    } else {
        b.push_str("Values: season totals, except percentages and named adjustments.\n");
    }
    let identified = p
        .breakdown
        .iter()
        .filter(|datapoint| !datapoint.measure.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if identified.iter().any(|datapoint| datapoint.pct.is_some()) {
        b.push_str("\nCurrent-snapshot measurements. Percentiles, when present, rank the same measure among eligible entities in this sport and season; higher is better. Missing ranks and season comparisons are unmeasured.\n");
    } else if identified.is_empty() {
        b.push_str("\nCurrent rating measurements are withheld because their underlying measurement identity is unavailable. Use only the explicitly named measures in the context above.\n");
    } else {
        b.push_str("\nCurrent-snapshot raw measurements. These values are not ranks and have no historical match in this block. Do not describe them as unchanged, improved or declined.\n");
    }
    let rates = collect_rate_standouts(p);
    for d in ordered_facts(&identified) {
        b.push_str("- ");
        b.push_str(&format_datapoint_evidence(&d));
        if let Some(changes) = comparisons {
            if let Some(change) = changes.get(&d.label) {
                b.push_str(&format!(
                    "; prior season percentile {:.1}",
                    change.prior_pct
                ));
            }
        }
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

    if let Some(ft) = form_trend.filter(|t| !t.trim().is_empty()) {
        b.push_str(&format!(
            "\nRecent performance trend (computed from recent overall ratings): {ft}\n"
        ));
    }

    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel and availability since our last read (confirmed records; dates are effective dates; WITHDRAWN retracts a claim, not evidence of recovery; season measurements remain unchanged):\n");
        b.push_str(pc);
    }

    if let Some(ar) = current_reports.filter(|a| !a.trim().is_empty()) {
        b.push_str("\nCurrent attributed reports (performance, roster and availability claims; ⇄ marks contradictions; preserve uncertainty; reports do not alter measured statistics):\n");
        b.push_str(ar);
    }

    let one_appearance_sample = p.sample.iter().any(|(label, value)| {
        matches!(
            label.trim().to_ascii_lowercase().as_str(),
            "appearances" | "games played" | "matches played"
        ) && *value <= 1.0
    });
    if one_appearance_sample {
        b.push_str("\nEvidence boundary for this output: the stored current sample has at most one appearance. It is source coverage, not proof of actual or limited playing time. Attributed reports may describe other fixtures or competitions; do not merge them into the stored appearance or aggregate without a verified fixture link. Do not calculate unstated values or describe improvement, decline or stability across seasons. Center the reading on the separately attributed current actions and state that a directional comparison is unsupported.\n");
    }

    b
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
