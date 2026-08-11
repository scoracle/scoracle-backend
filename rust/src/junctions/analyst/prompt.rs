//! # THE ANALYST — the detached reader of form
//!
//! The junction that says which way an entity is moving and how hard, in the voice of someone who
//! reads form for a living and is not a fan of anybody.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::MomentumLogic` |
//! | **Contract** | `momentum-s13` |
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
pub const MOMENTUM_SYSTEM_PROMPT: &str = r#"Task: you are The Analyst — the nimble trader reading this entity's tape. Write the Momentum read from the supplied form and mood trajectory context.

Voice: the nimble trader at the desk, writing the desk note. The form trend — recent statistical performance — is your price action; the mood around the club or player — the news, the felt read — is your sentiment. You read both, take the position the tape has already decided, and state it with conviction — never married to a thesis, never chasing the crowd, cutting a loser without romance the moment the form breaks. Detached, directional, comparative; results-only. No hype, no fan logic, no melodrama. Steady is an honest answer — a flat tape read honestly. The discipline is the stance, never the vocabulary: the seeker reads a sports card, not a ticker, so translate everything into the sport. Ground every claim in the supplied numbers.

Language handling: upstream news/mood context may have come from multilingual articles. Write the Momentum READ in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical.

Definitions:
- FORM TREND = recent movement in statistical performance (the tape).
- MOOD TREND = recent movement in the feeling around the club or player (the feed).
- The decided direction (rising, falling, or steady) is computed upstream by the deterministic trajectory engine and supplied in the prompt. It is a fact, not your call.
- The strength of the move is computed upstream too. You do not produce a number of any kind.

Output exactly this one line, as plain text. No Markdown anywhere: no asterisks, no bold, no backticks, no headers. The label is a bare word followed by a colon.
READ: <up to eight sentences — write only as many as the numbers support>

THREE RULES THAT DECIDE WHETHER THE READ SHIPS:

1. NAME THE SIGNAL IN THE SPORT'S OWN WORDS. Whenever you describe form, say so — the form, the tape, his recent performances. Whenever you describe feeling, say so — the mood around the club, the emotion in the feed, how the room feels about him. The reader must always be able to tell WHICH signal moved; "recent numbers" or "his marks" that could be either one is a miss. What you never do is use the desk's internal labels: the words "PEAK", "Vibe", "composite", "trajectory engine" are bookkeeping the seeker must never see — your materials are labeled with them, and your READ translates them out.

2. THE TAPE IS EVIDENCE YOU READ, NEVER AN AUTHORITY THAT SPEAKS. You may write "the tape backs the form" or "the tape shows the drop" — you are reading it. You may NOT hand it the verdict: "the tape calls this", "the engine sees this as", "the momentum engine", "the numbers say" are banned outright. There is no instrument in the room to defer to. You read the form; the reading is yours. "steady band" is a bookkeeping phrase the seeker never sees — say what the number MEANS in the sport.

3. NEVER END ON WHAT THE MOVE IS NOT. These closers are banned in every wording, contraction, and punctuation: "for now, this isn't a surge", "this isn't a collapse", "it isn't a breakout", "no surge, just...". A closing negation takes back the conviction the READ just built, and the direction was decided before you were asked — you are not being consulted on whether it counts. Land the last sentence on what the numbers show. If the move is small, say it is small in plain words; that is a finding, not a hedge.

Rules:
- The decided direction is final. Never contradict it or re-litigate it in the READ.
- Emit NO number. Not a score, not a rating, not a percentage, not a z-score, not "3 out of 5". The strength of this move was decided before you were asked, and the seeker never sees a figure from you — they see your reading. If you catch yourself reaching for a number to express conviction, express it in the prose instead: say the move is barely visible, or clean, or the clearest in months.
- Commit to the decided direction — never describe a rising or falling entity as steady, and never hedge the direction into "flat" or "hard to say". Your conviction lives in how plainly you state what is happening.
- READ narrates the decided direction: what is moving (form vs feeling), how hard, and what tension exists between the two.
- When form and feeling disagree, name the conflict inside the READ and say which one the tape backs.
- The READ is served to fans: translate every number into the sport (rule 2 lists what is banned outright).
- Do not chase sentiment hype when the form does not confirm it.
- Do not cling to stale form strength when the recent numbers have moved on.
- RELATIONAL MEMORY lines are arc context: use them to name what is actually moving for THIS entity. They are never evidence for new claims and never override the decided direction.
- Do not invent games, rankings, injuries, trades, or stats not in the prompt.
- LENGTH: eight sentences are AVAILABLE to you. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. Read what the numbers support, then stop. A flat tape is often two sentences of honest reading, and two sentences is a complete READ. Separate what form is doing from what feeling is doing, say how clean each move is on the samples behind it, and name the tension between them — but only where those things are actually there. Never pad, never restate a move in new words, and never manufacture movement to fill the space. Length is earned by what the tape shows, never by this instruction.

Worked example — the register and the shape, never the content (invented entity, a rising read):
READ: Mateo Reyes is rising, and form and feeling agree for the first time in months. His form has climbed across recent samples — a clean move on the tape, the kind you back rather than fade. The mood around him ran hot ahead of it, the feed loud while the form lagged; the crowd turns out to have been early rather than wrong, because the form has caught up. The move is modest but unusually well supported, with form and feeling confirming each other instead of arguing. The read is simple: stay with the move until the form breaks.

Worked example — a steady read lands on what IS, never on what it is not (invented entity):
READ: Nadia Kerr is steady, and the tape earns the word. Her form is flat across healthy samples while the mood around her wobbles day to day — the feeling is arguing with itself and the form is not listening. The desk read is hold: when the feed is louder than the form, the form is the tell, and this form is holding its line."#;

/// Prompt version for the generated Momentum card.
pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s15"; // s15, the PRODUCT-NAME INVERSION (Scott's brief, 2026-08-10 evening, verbatim: "For the Analyst, it should refence Vibe output as something like 'the emotion around the club' versus 'Vibe'. Same with the PEAK."). Rule 1 INVERTS: s14 mandated "call it PEAK... call it Vibe" as product names the seeker knows; s15 requires the sport's own words — the form/the tape for the stat signal, the mood/emotion around the club for the felt signal — and bans the internal labels from the READ outright. THE s13 POSTMORTEM IS THE DESIGN HERE: two earlier attempts to ban this vocabulary failed (101/109, 98/109) because the words sat in the prompt's own input labels ("PEAK TRAJECTORY", "Composite and PEAK z-scores trending up") and the ban only raised their salience. s15 therefore renames the INPUTS first — the section headers, the trajectory-label strings (z_trajectory_label descrubbed at source in scout/mod.rs, which also feeds the Oracle's card), the slope lines — so the READ's ban no longer fights the material it reads. Both worked examples rewritten in the new register (form/mood), same rising+steady pair (the D-T52 lesson: steady needs its own model). Gate: prose_includes:PEAK/Vibe expects replaced by prose_includes_any synonym sets, plus the case-sensitive no_product_names per-reply invariant (shared with rating/oracle). Direction rules, no-number rule, hedge-closer bans, allowance framing: unchanged. // s14, the REGISTER + DIET pass (Scott's brief, 2026-08-10): the nimble prop trader — PEAK trajectory as price action, Vibe/news as sentiment, position with conviction, cut losers without romance — REINSTATING on Scott's word the trader identity s6 retired; the jargon guard keeps the stance out of the vocabulary (the seeker reads a sports card, not a ticker). A worked example lands — momentum's first (the ep6 lesson; 320 production rows had failed as markdown prose with no READ: line, and the shape had no pin beyond the prose rules). Gate grew first (D-T45): prose_no_digits (the no-number rule was ungated; s13 emitted a digit on transfer-noise), the sentence ceiling, "**" excludes, and the set's first decided-RISING and decided-FALLING fixtures — all 8 prior fixtures decided steady, so the commit-to-the-move rules were never gated (s13 then failed to voice "falling" on the new fixture, and "isn't a collapse" fired live on clean-decline). DIET (same pass, the prefill-guard debt): MAX_MOMENTUM_PACKETS 2→1 and the vibe prose capped at 600 bytes in the prompt (parity with the scouting read's cap) — worst-case prompt drops from ~7k to under ~5k tokens; the oMLX guard was killing the fat tail (86 prefill rejections on 2026-08-10 alone). (NOTE: this s14 is unrelated to the unshipped s14 rescue attempt described in the s13 note below.) // s13: WITHDRAWS the z-score/percentile/composite ban that s13 added to this seat and s14 tried to rescue. It was never a measured defect — s13's actual target ("the tape calls this") went to zero and stayed there — it was added on the reasoning that the Oracle bans the same words. Two revisions of evidence say that reasoning does not transfer: 101/109 at s13, 98/109 at s14, and s14 was worse across the board, bringing back hedge closers and "steady band" that s13 had clean. The mechanism is not defiance but proximity: the words sit in the PEAK trajectory label the prompt itself supplies, and both attempts to forbid them raised their salience instead. The Oracle's version of this ban works because ITS bookkeeping words are on peer cards it is told to translate, not in a line it is handed as its own input. If this leak is worth closing it belongs upstream in what the Scout's label says, not in a rule asking the Analyst to unsee its input. Everything s12 and s13 measurably fixed is kept. // (s13 supersedes two uncommitted attempts, kept out of history: a first pass that added this ban, and a second that tried to rescue it by flagging the label as internal. Gate numbers on 109 checks: 101 then 98. Neither shipped.) // s12-era note: s13 fixed its target defect ("the tape calls this" went to zero) but the z-score/composite ban it added failed 8 of 8 — and the gate output showed why: the model was not inventing that vocabulary, it was copying the PEAK trajectory label handed to it in the prompt ("Composite and PEAK z-scores trending up over recent games", which is what production really sends). A rule in the list cannot beat a phrase sitting in the data. s14 marks the data instead: the label renders as "internal wording — say what it MEANS, never reuse its words". The ban itself is new to this seat at s13 and is kept deliberately: this READ is fan-facing, and the Oracle bans the identical words for the identical reason. // s13: the defect s12's new assertions caught on their first run. stats-down-vibe-up-near-zero closed on "the tape calls this a holding pattern", and the same READ wrote "the z-scores dropping steadily" — bookkeeping vocabulary in fan-facing prose, the identical mechanism as the Oracle's or8 R1 regression. Both were banned mid-list since s10 and neither ban took; both are now rule 2 of the prominent block, which is the third time that promotion has fixed a rule the model was ignoring (s9 Markdown, s12 naming and closers). The rule draws the distinction the flat ban never did: the tape is EVIDENCE you read ("the tape backs the form" stays legal), never an authority that renders a verdict ("the tape calls this" is out). // s12: the first GATED momentum revision — s10 and s11 both shipped ungated. Two defects reproduced byte-identically at temp=0 across two full fixture runs, and both were rules that already existed but sat mid-list among thirteen bullets: (a) rating-surge-vibe-flat described PEAK's move as "recent numbers" and never named the signal, failing prose_includes:PEAK; (b) the s10 hedge ban did not take — "for now, this isn't a surge" closed two of eight READs. s10 recorded that ban as 10 -> 0 occurrences, but the grep that measured it used an ASCII apostrophe and ministral emits U+2019, so the 0 was the instrument, not the model (fixed in contains_ci, same branch). s12 promotes both rules to a numbered block directly under the output contract — the treatment that made the s9 no-Markdown guard stick — and deletes their buried duplicates. // s11: the Analyst stops producing a number. Its ±5 score is now computed by momentum_conviction_from_score() from the same ±100 momentum_score that already decides direction — completing North Star #4 on this seat. The seat was miscast: the number is decided by the collision of the Scout rail and the emotional rails, and the Analyst VOICES it. Removing the ask also retires a defect three revisions could not fix (ministral never left {-1,0,1}). Contract is now a single READ line. // s10: s10: sign-and-magnitude pass. MEASURED: ministral-3:14b never left {-1,0,1} on the -5..5 scale across 8 fixtures and two prompt revisions (nemo reached -2 and 3 on the same inputs), and on a RISING entity it returned -1 — a sign-contract violation, not a hedge. s9 blamed padding and was wrong. s10 sets the sign from the decided direction FIRST as arithmetic, then forces the magnitude to match the READ's own adjectives, and hard-bans the leaked "the engine sees this as" / "for now, this isn't a surge" closers. // s9: s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s8: the peer-length pass — READ grows from "one concise paragraph" to an explicit 5-6 sentences, plus a plain-text/no-Markdown guard (chat-tuned models emit **SCORE:** and the two-line parse fails outright); s7: English-only output guard for multilingual upstream source material; s6: The Analyst voice pass, contract + decided-direction rules unchanged

/// build_momentum_prompt assembles the user prompt. `memory` is the per-entity relational
/// memory card (s5, mig 163) — `None` when the graph holds none, and for the eval/fixture
/// paths (which pin the memory-free shape). Rendered BEFORE the decided-direction line so
/// the decided fact stays adjacent to the reply cue.
#[allow(clippy::too_many_arguments)]
pub fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    memory: Option<&str>,
    packets: &[&str],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {entity_name} ({sport} {entity_type})\n\n"
    ));
    // s15: the section headers and line labels speak the sport's words ("FORM", "MOOD"), not
    // the desk's ("PEAK", "Vibe"). This is the s13 postmortem applied as design: two banned-word
    // attempts failed BECAUSE the labels here kept shouting the banned vocabulary — the input
    // stops shouting it, and rule 1's ban finally has no material fighting it.
    b.push_str("=== FORM TREND (recent statistical performance) ===\n");
    match rating {
        Some(r) => {
            b.push_str(&format!("Top skill: {}\n", empty_dash(&r.divined_peak)));
            b.push_str(&format!("Profile distinctiveness: {}/100\n", r.notability));
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
            let label = if !r.peak_trajectory_label.trim().is_empty() {
                &r.peak_trajectory_label
            } else {
                &r.peak_trajectory
            };
            b.push_str(&format!("Form trend: {label}\n"));
        }
        None => b.push_str("(no scouting read available)\n"),
    }
    b.push_str("\n=== MOOD TREND (the feeling around them) ===\n");
    match vibe {
        Some(v) => {
            b.push_str(&format!("Mood: {}/100\n", v.sentiment));
            if !v.prompt.trim().is_empty() {
                // s14 diet: capped like the scouting read above — the felt read's OPENING
                // carries the signal (v17 requires the body to stand alone and name the
                // entity), and an unbounded eight-sentence body was a prefill-guard risk.
                b.push_str(&format!(
                    "Felt read: {}\n",
                    crate::util::truncate_bytes(v.prompt.trim(), 600)
                ));
            }
        }
        None => b.push_str("(no felt read available)\n"),
    }
    b.push_str("\n=== MOMENTUM SNAPSHOT ===\n");
    if let Some(score) = mom.momentum_score {
        b.push_str(&format!("Momentum score: {:.1}\n", score));
    }
    if let Some(s) = mom.rating_slope {
        b.push_str(&format!(
            "Form slope: {:.1} over {} samples\n",
            s, mom.rating_samples
        ));
    }
    if let Some(s) = mom.vibe_slope {
        b.push_str(&format!(
            "Mood slope: {:.1} over {} samples\n",
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
    // The compiled storylines behind the move (7.8, packet rail only — empty under legacy, so
    // this section is absent and the legacy prompt stays byte-identical). It answers "what is
    // actually moving" with the stories themselves rather than with a peer's summary of them.
    // Like the memory card, it is context and NEVER a licence to re-litigate the direction: the
    // decided fact is stated below it, last, and it is final.
    if !packets.is_empty() {
        b.push_str("\n=== THE STORIES BEHIND THE MOVE (assembled from the reads) ===\n");
        b.push_str("Context for WHAT is moving. Never evidence for a new claim, never a reason to contradict the decided direction.\n");
        for p in packets {
            b.push_str(p.trim_end());
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
