//! Evidence and continuity supplied to the character.

use super::{article_context, CorpusItem, NarrativesReq};
use crate::util::truncate_bytes;

pub fn build_narratives_prompt(
    req: &NarrativesReq,
    news: &[CorpusItem],
    memory: Option<&str>,
    score_context: Option<&str>,
    packet_framing: Option<&str>,
    identity: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {} ({} {})\n",
        req.entity_name, req.sport, req.entity_type
    ));
    if let Some(card) = identity {
        b.push('\n');
        b.push_str(card);
        b.push('\n');
    }
    if let Some(f) = packet_framing.filter(|f| !f.trim().is_empty()) {
        b.push_str("\nThe story so far (assembled by the desk from every source below):\n");
        b.push_str(f.trim_end());
        b.push('\n');
    }
    b.push_str("\nRecent news (numbered):\n");
    for (i, n) in news.iter().enumerate() {
        b.push_str(&format!("{}. ", i + 1));
        if !n.source.is_empty() {
            b.push_str(&format!("[{}] ", n.source));
        }
        b.push_str(&n.title);
        let (body, body_cap) = article_context(n);
        if !body.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(body, body_cap));
        }
        b.push('\n');
    }
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this entity — use for arc and continuity: what fizzled before, what is live now, what actually happened; do NOT treat a prior story as evidence for a new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }
    if let Some(sc) = score_context.filter(|s| !s.trim().is_empty()) {
        b.push('\n');
        b.push_str(sc);
        if !sc.ends_with('\n') {
            b.push('\n');
        }
    }
    b
}
