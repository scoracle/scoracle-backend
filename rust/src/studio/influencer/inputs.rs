//! Evidence and continuity supplied to the character.

use super::{title_first, PacketBlock, PACKET_BLOCK_TRUNCATE};
use crate::runtime::util::truncate_bytes;

pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    packets: &[PacketBlock],
    memory: Option<&str>,
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {} {} ({})\n",
        title_first(entity_type),
        entity_name,
        sport
    ));

    if let Some(card) = memory.filter(|s| !s.trim().is_empty()) {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }

    if !packets.is_empty() {
        b.push_str("\nThe stories running around them right now (MOOD describes emotional language in the reporting; attribute feelings to the speaker or source shown):\n");
        for p in packets {
            b.push_str(truncate_bytes(p.text.trim_end(), PACKET_BLOCK_TRUNCATE).trim_end());
            b.push('\n');
        }
    }

    b
}
