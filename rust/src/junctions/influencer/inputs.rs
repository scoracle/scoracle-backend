//! Evidence and continuity supplied to the character.

use super::{title_first, Narrative, PacketBlock, PrevVibe, PACKET_BLOCK_TRUNCATE};
use crate::corpus::HeatItem;
use crate::util::truncate_bytes;

#[allow(clippy::too_many_arguments)]
pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat: &[HeatItem],
    packets: &[PacketBlock],
    previous: Option<&PrevVibe>,
    memory: Option<&str>,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {} {} ({})\n",
        title_first(entity_type),
        entity_name,
        sport
    ));

    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    if let Some(p) = previous {
        b.push_str("\n=== PREVIOUS VIBE ===\n");
        b.push_str(&format!("Score: {}/100\n", p.sentiment));
        if !p.vibe_prompt.is_empty() {
            b.push_str(&p.vibe_prompt);
            b.push('\n');
        }
    }

    if !packets.is_empty() {
        b.push_str("\nThe stories running around them right now (MOOD describes emotional language in the reporting; attribute feelings to the speaker or source shown):\n");
        for p in packets {
            b.push_str(truncate_bytes(p.text.trim_end(), PACKET_BLOCK_TRUNCATE).trim_end());
            b.push('\n');
        }
    }

    let _ = narratives;

    let _ = heat;

    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this entity — use for arc and continuity: what fizzled before, what is live now, what actually happened; do NOT treat a prior story as evidence for a new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b
}
