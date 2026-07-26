//! # THE ANALYST — the detached reader of form
//!
//! The junction that says which way an entity is moving and how hard, in the voice of someone who
//! reads form for a living and is not a fan of anybody.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::MomentumLogic` |
//! | **Contract** | `momentum-s7` |
//! | **Reads** | The Scout's PEAK report, The Influencer's vibe, the deterministic momentum snapshot |
//! | **Feeds** | The Oracle, as the Momentum pillar, via `momentum_summaries` |
//!
//! ## Authority — it narrates a decision it did not make
//!
//! This is the **omen pattern**, and it is the whole shape of this seat. Direction is not the
//! model's call: `momentum_direction_from_score` decides rising/steady/falling arithmetically from
//! `momentum_score` against `MOMENTUM_STEADY_BAND`, and that verdict is handed to the prompt as a
//! settled fact. The Analyst's job is to say *why* — to ground a number in the entity's actual
//! form. It may not contradict the direction, and nothing downstream asks it to.
//!
//! So the authority here is narrow and worth stating plainly: The Analyst owns the SCORE's
//! explanation and the READ's prose. It does not own the score, and it does not own the direction.
//! `momentum_scores` remains the numeric backbone for leaderboards and ranking either way — this
//! junction adds the client-surfaced read on top of it, never underneath it.
//!
//! ## What is deliberately kept out of the hash
//!
//! The scouting paragraph and the relational memory card are rendered into the prompt but excluded
//! from the `input_hash`. Both are derived commentary that moves on its own; hashing them would
//! make the stage re-trigger on someone else's rewording. Memory is continuity, never
//! corroboration — the echo-chamber rule — and it is never a licence to re-litigate the direction.
//!
//! ## Voice
//!
//! The trader/market metaphor was retired at s6: the identity is the form reader, and *form and
//! feeling* replace the old two-markets frame. s7 adds the English-only output guard, because the
//! upstream material this junction summarizes now routinely arrives in other languages.

use super::{momentum_direction_from_score, MOMENTUM_STEADY_BAND};
use crate::junctions::oracle::{SynthMomentum, SynthRating, SynthVibe};

/// s7 keeps the s6 Analyst voice pass and s5 SCORE/READ contract, but adds the English-only output
/// guard for upstream material derived from multilingual sources. The trader/market metaphor
/// remains retired: the identity is the form reader; form and feeling replace the two-markets frame.
pub const MOMENTUM_SYSTEM_PROMPT: &str = r#"Task: you are The Analyst — the detached reader of form. Write the Momentum read from the supplied PEAK and Vibe trajectory context.

Voice: detached, directional, comparative; results-only. You read two inputs — form (the PEAK/rating trajectory) and feeling (the Vibe/news trajectory) — and you narrate where it is heading versus where it was, with conviction and without attachment. No hype, no fan logic, no melodrama. Steady is an honest answer. Ground every claim in the supplied numbers.

Language handling: upstream news/vibe context may have come from multilingual articles. Write the Momentum READ in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical.

Definitions:
- PEAK trajectory = recent movement in statistical performance / rating signal (form).
- Vibe trajectory = recent movement in narrative sentiment (feeling).
- The decided direction (rising, falling, or steady) is computed upstream by the deterministic trajectory engine and supplied in the prompt. It is a fact, not your call.
- SCORE is signed conviction in the decided direction, not overall player/team quality.

Output exactly:
SCORE: <integer -5 to 5>
READ: <one concise paragraph>

Rules:
- The decided direction is final. Never contradict it or re-litigate it in the READ.
- SCORE sign must agree with the decided direction: rising is 1 to 5, falling is -5 to -1, steady is -1 to 1. Magnitude is how clean and strong the move is in the supplied numbers: a clean move on healthy samples earns 3 or more; 1 is for barely-there moves. Commit to the decided direction — never describe a rising or falling entity as steady.
- READ narrates the decided direction: what is moving (form vs feeling), how hard, and what tension exists between the two. Name the signals by their product names — PEAK for form, Vibe for feeling — when saying what moved.
- When form and feeling disagree, name the conflict inside the READ and let the score magnitude reflect the mixed tape.
- The READ is served to fans: never recite internal machinery — no momentum-score numbers, no "steady band", no rubric phrases. Translate the numbers into the sport.
- Do not chase sentiment hype when the form does not confirm it.
- Do not cling to stale PEAK strength when the recent numbers have moved on.
- RELATIONAL MEMORY lines are arc context: use them to name what is actually moving for THIS entity. They are never evidence for new claims and never override the decided direction.
- Do not invent games, rankings, injuries, trades, or stats not in the prompt."#;

/// Prompt version for the generated Momentum card.
pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s7"; // s7: English-only output guard for multilingual upstream source material; s6: The Analyst voice pass, contract + decided-direction rules unchanged

/// build_momentum_prompt assembles the user prompt. `memory` is the per-entity relational
/// memory card (s5, mig 163) — `None` when the graph holds none, and for the eval/fixture
/// paths (which pin the memory-free shape). Rendered BEFORE the decided-direction line so
/// the decided fact stays adjacent to the reply cue.
pub fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    memory: Option<&str>,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {entity_name} ({sport} {entity_type})\n\n"
    ));
    b.push_str("=== PEAK TRAJECTORY ===\n");
    match rating {
        Some(r) => {
            b.push_str(&format!("PEAK label: {}\n", empty_dash(&r.divined_peak)));
            b.push_str(&format!("Notability: {}/100\n", r.notability));
            // The scouting paragraph was always loaded here but never rendered (Phase 1 fix):
            // it is the richest available answer to "WHY is this entity trending" — the label
            // alone gave the model nothing to ground the READ line in. Excluded from the
            // input_hash (derived commentary, like sigil's continuity block), so a re-worded
            // body alone never triggers a regeneration.
            if !r.body.trim().is_empty() {
                b.push_str(&format!(
                    "Scouting read: {}\n",
                    crate::util::truncate_bytes(r.body.trim(), 600)
                ));
            }
            if !r.peak_trajectory_label.trim().is_empty() {
                b.push_str(&format!("PEAK trajectory: {}\n", r.peak_trajectory_label));
            } else {
                b.push_str(&format!("PEAK trajectory: {}\n", r.peak_trajectory));
            }
        }
        None => b.push_str("(no PEAK report available)\n"),
    }
    b.push_str("\n=== VIBE TRAJECTORY ===\n");
    match vibe {
        Some(v) => {
            b.push_str(&format!("Sentiment: {}/100\n", v.sentiment));
            if !v.prompt.trim().is_empty() {
                b.push_str(&format!("Vibe prompt: {}\n", v.prompt));
            }
        }
        None => b.push_str("(no vibe prompt available)\n"),
    }
    b.push_str("\n=== MOMENTUM SNAPSHOT ===\n");
    if let Some(score) = mom.momentum_score {
        b.push_str(&format!("Momentum score: {:.1}\n", score));
    }
    if let Some(s) = mom.rating_slope {
        b.push_str(&format!(
            "PEAK/rating slope: {:.1} over {} samples\n",
            s, mom.rating_samples
        ));
    }
    if let Some(s) = mom.vibe_slope {
        b.push_str(&format!(
            "Vibe slope: {:.1} over {} samples\n",
            s, mom.vibe_samples
        ));
    }
    if mom.empty() {
        b.push_str("(no durable momentum snapshot)\n");
    }
    // Relational memory card (s5, mig 163): per-entity arc context — prior stories with
    // outcomes, live stories with likelihood, ground-truth moves. CONTINUITY, NOT
    // CORROBORATION (the echo-chamber rule), and never a re-litigation of the decided
    // direction: it grounds WHAT is moving in real stories so the READ names this
    // entity's actual arc instead of generic filler. NOT part of the input_hash.
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\n=== RELATIONAL MEMORY (computed history) ===\n");
        b.push_str("Arc context for the READ: what fizzled before, what is live now, what actually happened. Never evidence for a new claim, never a reason to contradict the decided direction.\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }
    // The decided fact the model narrates (never decides) — the omen pattern. The band
    // rationale is spelled out so the READ can ground "how hard is it moving" in the
    // same scale the decision used.
    match mom.momentum_score {
        Some(score) => b.push_str(&format!(
            "\nDirection (decided upstream, final): {} (momentum score {:+.1}, steady band ±{:.0})\n",
            momentum_direction_from_score(Some(score)),
            score,
            MOMENTUM_STEADY_BAND
        )),
        None => b.push_str(
            "\nDirection (decided upstream, final): steady (no durable momentum snapshot)\n",
        ),
    }
    b.push_str("\nWrite the Momentum read now.");
    b
}

fn empty_dash(s: &str) -> &str {
    if s.trim().is_empty() {
        "-"
    } else {
        s
    }
}
