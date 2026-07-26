//! # THE INFLUENCER — the one who knows what the room is feeling before the room does
//!
//! The emotional rail's end product. Where The Journalist files what happened, this junction says
//! how it *lands*: a sentiment score, a hook, and the felt read.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::VibeLogic` |
//! | **Contract** | `v14` |
//! | **Reads** | The Journalist's storylines with their impact and trajectory, The Insider's vetted heat, its own previous read, the relational memory card |
//! | **Feeds** | The Analyst and The Oracle, via `vibe_scores` |
//!
//! ## Authority — emotion, and only emotion
//!
//! The Influencer owns the sentiment number and the hook. It owns no facts. Every storyline it
//! reacts to is The Journalist's, every transfer fact is The Insider's, and it may not introduce an
//! event that no one else filed. Its whole contribution is the read on material it did not gather —
//! which is exactly why it sits downstream of both.
//!
//! ## Continuity is the whiplash killer
//!
//! Since v12 the previous vibe read anchors the prompt, alongside the per-entity relational memory
//! card. Both are prompt-only and deliberately excluded from the `input_hash` — the same decision
//! as n8 and t8. The point is stated plainly in the doctrine: the felt read should move like a
//! belief, not like a readout of the day's headlines. An entity whose news went quiet should cool
//! off, not snap to neutral.
//!
//! ## Fail closed
//!
//! No narratives AND no transfer heat means no model call: a NULL-sentiment marker row is written
//! and the read path returns "no data". Marker rows carry the empty-material hash, so quiet
//! entities debounce instead of re-marking every cycle. A completed vibe enqueues The Analyst
//! before the terminal convergence — and does so even on a debounce-skip, so a previously missed
//! hand-off self-heals without spending a model call.

use crate::trajectory::trajectory_label;
use super::{BODY_TRUNCATE, Narrative, PrevVibe, title_first};
use crate::corpus::{HeatItem, write_heat_lines};
use crate::util::truncate_bytes;

/// System prompt for the Vibe sentiment + felt-read contract.
///
/// v14 keeps the v13 Influencer voice pass and v12 SCORE contract, but adds the English-only
/// output guard for upstream material derived from multilingual sources. VIBE stays card-quality
/// prose and the reply still carries the HOOK line.
pub const VIBE_SYSTEM_PROMPT: &str = r#"Task: you are The Influencer — the one who knows what the room is feeling before the room does. Your platform is this entity's moment: read the supplied narratives and transfer/trade activity, find the emotion already running through them, and post the felt read — a score, a hook, and the vibe.

Voice: vivid, present tense, engagement-first. You ride the feeling because it is real, never because it clicks — sincerity is the craft: no manufactured outrage, no borrowed drama, no bait. When the room is loud, capture the roar; when it is flat, a true quiet read beats a loud false one.

Language handling: supplied narratives or transfer/trade lines may have been derived from multilingual sources. Write HOOK and VIBE in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical; do not introduce non-English phrasing unless it is a proper name.

SCORE (1-100):
- 1 = grim or in freefall.
- 50 = quiet, unclear, or genuinely mixed.
- 100 = euphoric or surging.
- Weigh narratives by impact.
- Let impact set the amplitude: when the strongest storyline's impact is 4 or less, the cycle is quiet — score it 40-65 whichever way it leans. Big feelings need big stories.
- Transfer/trade activity is energy, not automatically good or bad.
- If little is happening, stay near 50 — a routine result in a quiet week is calm, not a surge. Never inflate a flat cycle to make content.
- Reserve the extremes (under 15, over 90) for genuinely seismic moments — the room is rarely all the way up or all the way down.
- When a PREVIOUS VIBE is shown, treat its score as your prior: move from it deliberately and hold steady unless the new signals justify a change. This is memory, not a reset.
- Mood has history: relational memory showing a warm past under cold coverage (or the reverse) is a swing worth reading — the tension is the story, not a contradiction to smooth over.

HOOK:
- The title of the post: ONE line, under twelve words, present tense.
- Name the feeling and who or what carries it — the specific player, club, move, or number.
- Write it as the feeling, not a news headline — no "Topic: Subtitle" colon constructions, no title-case formality.
- The hook must trace to the supplied signals. Never invent one the coverage does not back.
- No caps-lock, no question-mark bait, no "you won't believe" mechanics — the emotion IS the draw.

VIBE:
- The body of the post: two or three sentences of finished prose, written to be read — the felt read of the moment, not a data recap. A truly quiet cycle can be one sentence.
- Present tense. Name the actual players, clubs, moves, and numbers behind the dominant threads; let minor items go.
- Do not use generic phrases when the signals give specifics.
- Ground every claim in the supplied signals. Mood is not durable truth: capture what the room feels without promoting it to fact.

Reply with exactly these three lines:
SCORE: <integer 1-100>
HOOK: <the one-line title>
VIBE: <the felt read>"#;

/// Prompt version for the Vibe sentiment + felt-read contract.
pub const VIBE_PROMPT_VERSION: &str = "v14"; // v14: English-only output guard for multilingual upstream source material; v13: The Influencer voice pass + HOOK card title

/// build_sentiment_prompt assembles the user prompt. `sport` is the original-case value used in
/// the prompt; the SQL reads use the upper-cased form. `previous` is the prior vibe read for
/// continuity (v12) — rendered as a lead-in anchor, `None` for the parity/eval paths and an
/// entity's first read. `memory` is the per-entity relational memory card (mig 163) — `None`
/// when the graph holds none.
pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat: &[HeatItem],
    previous: Option<&PrevVibe>,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();

    b.push_str(&format!(
        "Entity: {} {} ({})\n",
        title_first(entity_type),
        entity_name,
        sport
    ));

    // Previous vibe (v12) — a continuity anchor set BEFORE the fresh signals so the model
    // reads its prior before the new evidence (the sigil Phase-5.2 placement). Omitted
    // entirely when there is no prior read: this section is prompt-only and outside the
    // hash, so it needs no stable no-data placeholder.
    if let Some(p) = previous {
        b.push_str("\n=== PREVIOUS VIBE ===\n");
        b.push_str(&format!("Score: {}/100\n", p.sentiment));
        if !p.vibe_prompt.is_empty() {
            b.push_str(&p.vibe_prompt);
            b.push('\n');
        }
    }

    b.push_str(
        "\nNarratives forming around them (ordered by relevance/topic heat; impact in brackets):\n",
    );
    if narratives.is_empty() {
        b.push_str("- (none this cycle)\n");
    } else {
        for n in narratives {
            let mut tags = format!(
                "{}, {}, topic heat {}",
                n.impact,
                trajectory_label(&n.trajectory),
                n.topic_heat
            );
            // Corroboration + freshness (Phase 1): the felt read should weigh a 5-source
            // storyline from today differently than a single stale one.
            if n.source_count > 0 {
                tags.push_str(&format!(", {} sources", n.source_count));
            }
            if let Some(d) = n.source_age_days {
                tags.push_str(&format!(", latest {d}d ago"));
            }
            b.push_str(&format!(
                "- [{tags}] {}: {}\n",
                n.title,
                truncate_bytes(&n.body, BODY_TRUNCATE)
            ));
        }
    }

    b.push_str("\nCurrent transfer/trade activity (heat 0-100):\n");
    if heat.is_empty() {
        b.push_str("- (none)\n");
    } else {
        write_heat_lines(&mut b, heat);
    }

    // Relational memory card (v12, mig 163): the graph's per-entity history — prior
    // stories with outcomes, current stories with likelihood, ground-truth moves.
    // CONTINUITY, NOT CORROBORATION (the echo-chamber rule): memory frames the arc the
    // felt read sits in; it is never itself evidence for a new claim. Rendered only when
    // the graph holds memory; deliberately NOT part of the input_hash.
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this entity — use for arc and continuity: what fizzled before, what is live now, what actually happened; do NOT treat a prior story as evidence for a new one):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b.push_str("\nRespond now (SCORE line, then HOOK line, then VIBE line).");
    b
}
