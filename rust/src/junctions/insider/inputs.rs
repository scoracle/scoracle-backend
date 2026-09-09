//! Evidence and continuity supplied to the character.

use crate::corpus::{write_heat_lines, HeatItem};

pub fn build_insider_score_prompt(
    entity_name: &str,
    sport: &str,
    entity_type: &str,
    heat: &[HeatItem],
    prior: Option<&str>,
    identity: Option<&str>,
) -> String {
    let mut b = format!("Entity: {entity_name} ({sport} {entity_type})\n");
    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    if let Some(p) = prior.filter(|s| !s.trim().is_empty()) {
        b.push_str(
            "\nYOUR PRIOR READS (memory — your own past wire wraps; continuity, not new evidence):\n",
        );
        b.push_str(p);
        if !p.ends_with('\n') {
            b.push('\n');
        }
    }
    if heat.is_empty() {
        b.push_str(
            "\nTHE ACTIVE WIRE is empty: every previously vetted rumor has expired or been resolved. File the quiet wire — no live calls, no manufactured movement.\n",
        );
    } else {
        b.push_str(&format!(
            "\nTHE ACTIVE WIRE ({} live vetted rumor(s), latest per counterparty):\n",
            heat.len()
        ));
        write_heat_lines(&mut b, heat);
    }
    b
}
