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

    b.push_str("\nDatapoints — value, percentile + TIER (the tier is the truth), rating (how far above or below the average; a higher rating is a rarer edge); [position] percentile when present:\n");
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
        b.push_str("\nSeason-over-season movement (computed against last season's percentiles — the movement word on each line is decided; voice the moves that matter to a staff, in the sport's words, beside this season's number):\n");
        for line in zm.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    if let Some(ft) = form_trend.filter(|t| !t.trim().is_empty()) {
        b.push_str(&format!(
            "\nRecent-form marker (computed shading only — not yours to restate as a verdict; the week-to-week momentum story is another character's turn): {ft}\n"
        ));
    }

    if let Some(pc) = personnel.filter(|p| !p.trim().is_empty()) {
        b.push_str("\nPersonnel and availability since our last read (confirmed facts from the adjudicated transfer and availability records — dates are when the change took force; a WITHDRAWN record means we no longer claim it happened, not that the player recovered; these do NOT alter any tier or number above, which are this season's measured truth, but they tell you WHO is actually available, which changes how the rest of the profile should be read):\n");
        b.push_str(pc);
    }

    if let Some(ar) = availability_reports.filter(|a| !a.trim().is_empty()) {
        b.push_str("\nReported availability, NOT yet confirmed (injury and suspension claims the desk has collected for this entity, each with the outlet that made it; ⇄ marks a claim another claim here contradicts). These are REPORTS, not the record above — weigh them: who is saying it, whether they agree, and how firm the wording is. Report what you judge sound and attribute it; say a report is disputed where it is; leave out what you do not credit. Never state a disputed claim as settled fact, and never carry a number from here into a tier or rating above:\n");
        b.push_str(ar);
    }

    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nCross-season memory (computed history — arc context only: the datapoints and TIERS above are this season's truth and are never overridden by memory; use these lines for trajectory, new-club context, and matchup quirks; weigh each matchup line by its reliability — a low-reliability edge deserves an explicit grain of salt; a prior read is continuity, never evidence for the new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b
}
