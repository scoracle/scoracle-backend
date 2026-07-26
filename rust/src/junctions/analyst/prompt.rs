//! # THE ANALYST — the detached reader of form
//!
//! The junction that says which way an entity is moving and how hard, in the voice of someone who
//! reads form for a living and is not a fan of anybody.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::MomentumLogic` |
//! | **Contract** | `momentum-s12` |
//! | **Reads** | The Scout's PEAK report, The Influencer's vibe, the deterministic momentum snapshot |
//! | **Feeds** | The Oracle, as the Momentum pillar, via `momentum_summaries` |
//!
//! ## Authority — it voices a number it does not own
//!
//! This is the **omen pattern**, and as of s11 this seat applies it completely. The pattern is
//! North Star #4 — *deterministic facts are computed, models narrate* — and it appears wherever a
//! number already exists in the system: the Oracle is handed a computed OMEN it may not contradict,
//! and the Analyst is handed a computed direction and conviction it may not contradict. The model's
//! job in both seats is to say *why*, never *what*.
//!
//! Both of this seat's numbers come from one place. `momentum_score` is the ±100-scale signed slope
//! average — the collision of the Scout rail (rating percentile delta) and the emotional rails
//! (vibe sentiment delta). `momentum_direction_from_score` reads rising/steady/falling off it
//! against `MOMENTUM_STEADY_BAND`; `momentum_conviction_from_score` reads the ±5 magnitude off the
//! same value. Both are handed to the prompt as settled fact, and the persisted row cannot tell two
//! stories because both derive from one number.
//!
//! Until s11 the magnitude was asked of the model, which was a miscast: the Analyst was treated as
//! a seat that *generates* a score when the score was already decided upstream. It did not survive
//! contact — see `momentum_conviction_from_score` for the measured failure. The authority is now
//! narrow and clean: **The Analyst owns the READ's prose and nothing else.**
//!
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
//! upstream material this junction summarizes now routinely arrives in other languages. s9 adds the
//! plain-text/no-Markdown guard — chat-tuned models emit `**SCORE:**` and the line parser rejected
//! every one. s11 removes the number from the contract entirely; conviction now lives in how plainly
//! the READ states the move, not in a figure the seat was never the author of.

use super::{momentum_direction_from_score, MOMENTUM_STEADY_BAND};
use crate::junctions::oracle::{SynthMomentum, SynthRating, SynthVibe};

/// s11 reduces the contract to a single READ line: the Analyst emits no number at all, because the
/// ±5 conviction is now computed by `momentum_conviction_from_score` from the same ±100
/// `momentum_score` that already decides direction. The trader/market metaphor remains retired
/// (s6): the identity is the form reader; form and feeling replace the two-markets frame. The
/// English-only guard (s7) and the plain-text/no-Markdown guard (s9) are unchanged.
pub const MOMENTUM_SYSTEM_PROMPT: &str = r#"Task: you are The Analyst — the detached reader of form. Write the Momentum read from the supplied PEAK and Vibe trajectory context.

Voice: detached, directional, comparative; results-only. You read two inputs — form (the PEAK/rating trajectory) and feeling (the Vibe/news trajectory) — and you narrate where it is heading versus where it was, with conviction and without attachment. No hype, no fan logic, no melodrama. Steady is an honest answer. Ground every claim in the supplied numbers.

Language handling: upstream news/vibe context may have come from multilingual articles. Write the Momentum READ in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical.

Definitions:
- PEAK trajectory = recent movement in statistical performance / rating signal (form).
- Vibe trajectory = recent movement in narrative sentiment (feeling).
- The decided direction (rising, falling, or steady) is computed upstream by the deterministic trajectory engine and supplied in the prompt. It is a fact, not your call.
- The strength of the move is computed upstream too. You do not produce a number of any kind.

Output exactly this one line, as plain text. No Markdown anywhere: no asterisks, no bold, no backticks, no headers. The label is a bare word followed by a colon.
READ: <up to eight sentences — write only as many as the numbers support>

TWO RULES THAT DECIDE WHETHER THE READ SHIPS:

1. NAME THE SIGNAL YOU ARE DESCRIBING. Whenever you describe form, call it PEAK. Whenever you describe feeling, call it Vibe. These are product names the seeker already knows and looks for. Describing what PEAK is doing while calling it "recent numbers", "the composite", or "his marks" is a miss — the reader cannot tell which signal moved. You may write "the tape" alongside the product name, never instead of it.

2. NEVER END ON WHAT THE MOVE IS NOT. These closers are banned in every wording, contraction, and punctuation: "for now, this isn't a surge", "this isn't a collapse", "it isn't a breakout", "no surge, just...". A closing negation takes back the conviction the READ just built, and the direction was decided before you were asked — you are not being consulted on whether it counts. Land the last sentence on what the numbers show. If the move is small, say it is small in plain words; that is a finding, not a hedge.

Rules:
- The decided direction is final. Never contradict it or re-litigate it in the READ.
- Emit NO number. Not a score, not a rating, not a percentage, not a z-score, not "3 out of 5". The strength of this move was decided before you were asked, and the seeker never sees a figure from you — they see your reading. If you catch yourself reaching for a number to express conviction, express it in the prose instead: say the move is barely visible, or clean, or the clearest in months.
- Commit to the decided direction — never describe a rising or falling entity as steady, and never hedge the direction into "flat" or "hard to say". Your conviction lives in how plainly you state what is happening.
- READ narrates the decided direction: what is moving (form vs feeling), how hard, and what tension exists between the two.
- When form and feeling disagree, name the conflict inside the READ and say which one the tape backs.
- The READ is served to fans: never recite internal machinery — no momentum-score numbers, no "steady band", no rubric phrases. Translate the numbers into the sport. These exact phrasings are BANNED and must never appear: "the engine", "the momentum engine", "the tape calls this", "the engine sees this as", "steady band". You are the one reading the tape; there is no engine in the room to defer to.
- Do not chase sentiment hype when the form does not confirm it.
- Do not cling to stale PEAK strength when the recent numbers have moved on.
- RELATIONAL MEMORY lines are arc context: use them to name what is actually moving for THIS entity. They are never evidence for new claims and never override the decided direction.
- Do not invent games, rankings, injuries, trades, or stats not in the prompt.
- LENGTH: eight sentences are AVAILABLE to you. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. Read what the numbers support, then stop. A flat tape is often two sentences of honest reading, and two sentences is a complete READ. Separate what form is doing from what feeling is doing, say how clean each move is on the samples behind it, and name the tension between them — but only where those things are actually there. Never pad, never restate a move in new words, and never manufacture movement to fill the space. Length is earned by what the tape shows, never by this instruction."#;

/// Prompt version for the generated Momentum card.
pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s12"; // s12: the first GATED momentum revision — s10 and s11 both shipped ungated. Two defects reproduced byte-identically at temp=0 across two full fixture runs, and both were rules that already existed but sat mid-list among thirteen bullets: (a) rating-surge-vibe-flat described PEAK's move as "recent numbers" and never named the signal, failing prose_includes:PEAK; (b) the s10 hedge ban did not take — "for now, this isn't a surge" closed two of eight READs. s10 recorded that ban as 10 -> 0 occurrences, but the grep that measured it used an ASCII apostrophe and ministral emits U+2019, so the 0 was the instrument, not the model (fixed in contains_ci, same branch). s12 promotes both rules to a numbered block directly under the output contract — the treatment that made the s9 no-Markdown guard stick — and deletes their buried duplicates. // s11: the Analyst stops producing a number. Its ±5 score is now computed by momentum_conviction_from_score() from the same ±100 momentum_score that already decides direction — completing North Star #4 on this seat. The seat was miscast: the number is decided by the collision of the Scout rail and the emotional rails, and the Analyst VOICES it. Removing the ask also retires a defect three revisions could not fix (ministral never left {-1,0,1}). Contract is now a single READ line. // s10: s10: sign-and-magnitude pass. MEASURED: ministral-3:14b never left {-1,0,1} on the -5..5 scale across 8 fixtures and two prompt revisions (nemo reached -2 and 3 on the same inputs), and on a RISING entity it returned -1 — a sign-contract violation, not a hedge. s9 blamed padding and was wrong. s10 sets the sign from the decided direction FIRST as arithmetic, then forces the magnitude to match the READ's own adjectives, and hard-bans the leaked "the engine sees this as" / "for now, this isn't a surge" closers. // s9: s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s8: the peer-length pass — READ grows from "one concise paragraph" to an explicit 5-6 sentences, plus a plain-text/no-Markdown guard (chat-tuned models emit **SCORE:** and the two-line parse fails outright); s7: English-only output guard for multilingual upstream source material; s6: The Analyst voice pass, contract + decided-direction rules unchanged

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
