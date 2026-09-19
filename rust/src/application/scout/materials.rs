//! Render application-owned evidence into prepared Scout material.

use crate::evidence::personnel::{AvailabilityChange, PersonnelChange};
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
