//! Evidence and continuity supplied to the Insider.

use crate::evidence::corpus::{write_heat_lines, HeatItem};

pub fn build_insider_score_prompt(
    entity_name: &str,
    sport: &str,
    entity_type: &str,
    heat: &[HeatItem],
    memory: Option<&str>,
) -> String {
    let mut b = format!("Entity: {entity_name} ({sport} {entity_type})\n");
    if let Some(card) = memory.filter(|s| !s.trim().is_empty()) {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
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
