//! Evidence and continuity supplied to the Insider.

use super::HeatItem;

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

/// write_heat_lines renders heat bullets:
///   `- <counterparty> — heat <heat>[, <direction>][, <stage>][ (confidence 0.N)][ — "<summary>"]`
///
/// All downstream prompts share this transfer-heat line format.
fn write_heat_lines(b: &mut String, heat: &[HeatItem]) {
    for h in heat {
        let mut line = format!("- {} — heat {}", h.counterparty, h.heat);
        if !h.direction.is_empty() {
            line.push_str(", ");
            line.push_str(&h.direction);
        }
        if !h.stage.is_empty() {
            line.push_str(", ");
            line.push_str(&h.stage);
        }
        if let Some(c) = h.confidence {
            line.push_str(&format!(" (confidence {c:.1})"));
        }
        if !h.summary.is_empty() {
            // The summary is written ≤240 bytes and single-sentence; fold any stray newline so
            // one rumor stays one prompt bullet.
            line.push_str(" — \"");
            line.push_str(&h.summary.replace(['\n', '\r'], " "));
            line.push('"');
        }
        b.push_str(&line);
        b.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_lines_render_summary_and_confidence_when_present() {
        let heat = vec![
            HeatItem {
                counterparty: "Lakers".to_string(),
                heat: 80,
                stage: "advanced_talks".to_string(),
                direction: "incoming".to_string(),
                summary: "Lakers pursuing a\nwing upgrade per ESPN".to_string(),
                confidence: Some(0.75),
            },
            // Bare item (a pre-Phase-1 row with no model_summary): the original line shape.
            HeatItem {
                counterparty: "Heat".to_string(),
                heat: 40,
                stage: String::new(),
                direction: String::new(),
                summary: String::new(),
                confidence: None,
            },
        ];
        let mut b = String::new();
        write_heat_lines(&mut b, &heat);
        assert_eq!(
            b,
            "- Lakers — heat 80, incoming, advanced_talks (confidence 0.8) — \"Lakers pursuing a wing upgrade per ESPN\"\n\
             - Heat — heat 40\n"
        );
    }
}
