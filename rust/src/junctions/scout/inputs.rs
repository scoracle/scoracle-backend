//! Evidence and continuity supplied to the character.

use super::{
    collect_rate_standouts, format_datapoint_evidence, ordered_facts, AvailabilityChange,
    PersonnelChange, RatingProfile, RatingReq,
};
use crate::junctions::editor::render::MarkedClaim;

pub fn render_availability_reports(claims: &[MarkedClaim]) -> Option<String> {
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
                    "{}: joined {} from {old}{event}.",
                    c.date_label,
                    c.new_team.as_deref().unwrap_or("a new club")
                ),
                None => format!(
                    "{}: joined {}{event}.",
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
                    "{}: signed {} from {old}{event}.",
                    c.date_label, c.player_name
                ),
                None => format!("{}: signed {}{event}.", c.date_label, c.player_name),
            },
            ("team", _) => match c.new_team.as_deref() {
                Some(new) => format!("{}: lost {} to {new}{event}.", c.date_label, c.player_name),
                None => format!("{}: lost {}{event}.", c.date_label, c.player_name),
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

#[allow(clippy::too_many_arguments)]
pub fn build_stat_prompt(
    req: &RatingReq,
    p: &RatingProfile,
    notability: i32,
    memory: Option<&str>,
    personnel: Option<&str>,
    z_memory: Option<&str>,
    form_trend: Option<&str>,
    availability_reports: Option<&str>,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();

    let mut header = format!("{} {}", req.sport, req.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!("Entity: {} ({header})\n", req.entity_name));

    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    b.push_str(&format!(
        "\nProfile distinctiveness: {notability}/100 (higher = more standout skills).\n"
    ));

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nOverall score (how WELL overall — T-score, 50 = average): {comp:.0}\n"
        ));
    }

    b.push_str("\nDatapoints — measured value, percentile, tier, rating (distance from average), and position percentile when available:\n");
    for d in ordered_facts(&p.breakdown) {
        b.push_str("- ");
        b.push_str(&format_datapoint_evidence(&d));
        b.push('\n');
    }

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        b.push_str("\nRate-adjusted (per-x) corroboration — these also rate elite on a per-minute / per-90 basis (so the edge is not just a counting-stat artifact of heavy minutes):\n");
        for r in &rs {
            b.push_str(&format!(
                "- [{}] {}: {:.0}th pct\n",
                r.mode.replace('_', "-"),
                r.label,
                r.pct
            ));
        }
    }

    if let Some(zm) = z_memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nSeason-over-season movement (computed against last season's percentiles):\n");
        for line in zm.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    if let Some(ft) = form_trend.filter(|t| !t.trim().is_empty()) {
        b.push_str(&format!(
            "\nRecent-form marker (computed context; recent momentum belongs to The Analyst): {ft}\n"
        ));
    }

    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel and availability since our last read (confirmed records; dates are effective dates; WITHDRAWN retracts a claim, not evidence of recovery; season measurements remain unchanged):\n");
        b.push_str(pc);
    }

    if let Some(ar) = availability_reports.filter(|a| !a.trim().is_empty()) {
        b.push_str("\nReported availability, NOT yet confirmed (attributed reports; ⇄ marks contradictory claims; preserve uncertainty and disputes; reports do not change measured tiers or ratings):\n");
        b.push_str(ar);
    }

    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nCross-season memory (continuity, not fresh evidence or replacement measurements; matchup reliability is supplied):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b
}
