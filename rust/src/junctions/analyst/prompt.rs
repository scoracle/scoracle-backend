//! # THE ANALYST — the detached reader of form
//!
//! The junction that says which way an entity is moving and how hard, in the voice of someone who
//! reads form for a living and is not a fan of anybody.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::MomentumLogic` |
//! | **Contract** | `momentum-s19` |
//! | **Reads** | The two rails and their collision: the rating slope + trend label, the vibe level + slope, and `momentum_score`. **Peer PROSE is deliberately not among them** — see s19. |
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
//! ## What this seat deliberately does NOT read (s19)
//!
//! The scouting paragraph, the felt read, the compiled storylines and the relational memory card
//! were all rendered into this prompt until s19, and they are the measured reason the seat read
//! like a second Oracle: with four seats' material in front of her the Analyst named a trajectory
//! in 57% of READs while touching the stat profile in 42%, the mood in 42%, the news in 42% and
//! transfers in 28%. Her contract was never the problem — it already asked for exactly the
//! two-rail read. Her inputs were. They are gone, and `only_the_two_rails_reach_the_prompt` keeps
//! them gone.
//!
//! The corollary is that nothing derived is left to keep out of the `input_hash`: the seat reads
//! numbers it already hashes.
//!
//! ## Voice
//!
//! The trader/market metaphor was retired at s6: the identity is the form reader, and *form and
//! feeling* replace the old two-markets frame. s7 adds the English-only output guard, because the
//! upstream material this junction summarizes now routinely arrives in other languages. s9 adds the
//! plain-text/no-Markdown guard — chat-tuned models emit `**SCORE:**` and the line parser rejected
//! every one. s11 removes the number from the contract entirely; conviction now lives in how plainly
//! the READ states the move, not in a figure the seat was never the author of.

use super::{momentum_conviction_from_score, momentum_direction_from_score};
use crate::junctions::oracle::{SynthMomentum, SynthRating, SynthVibe};

/// s11 reduces the contract to a single READ line: the Analyst emits no number at all, because the
/// ±5 conviction is now computed by `momentum_conviction_from_score` from the same ±100
/// `momentum_score` that already decides direction. The trader/market metaphor remains retired
/// (s6): the identity is the form reader; form and feeling replace the two-markets frame. The
/// English-only guard (s7) and the plain-text/no-Markdown guard (s9) are unchanged.
pub const MOMENTUM_SYSTEM_PROMPT: &str = r#"Task: you are The Analyst — the nimble trader reading this entity's tape. Write the Momentum read.

YOUR BEAT IS TRAJECTORY. Two rails run under this entity: the form (recent statistical performance) and the mood (the feeling around them). Where each is HEADING is your subject, both of them. On top of that you are the only seat that reads them AGAINST each other — "the results are poor but the room is high" is your sentence and nobody else's, and a divergence is the most valuable thing you can find.

You do not explain either rail. What the entity IS statistically belongs to another desk; the stories — news, transfers, what the room is upset about — belong to three others. If you want to say WHY a rail moved, you have crossed into someone else's card.

Voice: the desk note. Detached, directional, results-only. No hype, no fan logic. Steady is an honest answer.

The decided direction and its strength are computed upstream and handed to you as words. They are facts, not your call: never contradict them, never re-litigate them, and never call a falling entity steady. When the sample behind a rail is thin, say so.

Not one digit. Every quantity in words — three straight losses, most of the season — never a figure.

Say which signal moved, in the sport's words: the form, the tape, his recent performances; the mood around the club, how the room feels. "Recent numbers" that could be either one is a miss. Never end on what the move is NOT ("this isn't a collapse") — land on what the numbers show.

This prints on a CARD. Two to four sentences, and two is a complete read. Separate what form is doing from what mood is doing, then name the tension between them. Never pad, never restate a move in new words, never reach for a story to fill space.

Output exactly two lines, plain text, no Markdown:
READ: <two to four sentences>
HEADLINE: <card title, twelve words or fewer, naming the entity, every word traceable to your READ>

Example of the shape only, never the content (invented entity):
READ: Nadia Kerr is steady, and the tape earns the word. Her form is flat across healthy samples while the mood around her wobbles day to day. When the feed is louder than the form, the form is the tell.
HEADLINE: Nadia Kerr steady as the form holds its line"#;

/// Prompt version for the generated Momentum card.
pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s19"; // s19, the ROLE pass (2026-08-22, Scott's brief: "these are all getting meshed together into AI slop"). MEASURED across eight well-covered teams: the Analyst named a trajectory in 57% of her READs while touching the stat profile in 42%, the mood in 42%, the news in 42% and transfers in 28% — the worst role bleed of the five seats and the only one that failed to lead on its own job. SHE WAS NOT DISOBEYING HER CONTRACT: it already said "READ narrates the decided direction: what is moving (form vs feeling)... and what tension exists between the two", which is the job exactly. She was narrating her INPUTS, four fifths of which belonged to other seats — the Scout's brief at 600 bytes, the Influencer's felt read at 600 bytes, the compiled storylines, and the relational memory card, with her own three numbers rendered last and smallest. This is the s13/s15 postmortem for the third time on this seat (a rule in the list cannot beat a phrase sitting in the model's own input), so s19 removes the INPUT rather than adding a rule: no peer prose, no packets, no memory card. She is off the packet rail entirely, which is also a saved DB round trip per item. What is left is the two rails — where each stands, which way each moves — and their collision, restructured under THE FORM RAIL / THE MOOD RAIL / WHERE THE RAILS COLLIDE. The spec gains an opening statement of the beat (direction, and the relationship between the rails: "the results are poor but the room is high" is her sentence and no other seat's), a rule that the WHY of a move belongs to other cards, and a length cut from eight sentences to five, because the material that justified eight was four other seats'. Register, direction rules, no-digit rule, closers, worked examples: untouched. // s18, the DIGIT-STARVATION pass (2026-08-22, the fail-rate session): MEASURED 1,223 READ rejections against 650 shipped cards in 24h — 65% of momentum generations burned on guard retries, digits_in_read the dominant cause by far (689 of the failed rows), "steady band" second (75). Two moves, both straight from the s13/s15 postmortem (a rule cannot beat a phrase sitting in the model's own input): (a) the prompt's DIRECTION LINE stops handing the READ a figure and the words "steady band" in the same breath it is told to emit neither — "(momentum score +12.3, steady band ±8)" becomes a words-only strength phrase, the ladder computed by momentum_conviction_from_score so the prose and the persisted ±5 conviction can never tell two stories; (b) the no-number rule leaves the buried Rules list for the numbered ships-or-not block (new rule 2, block goes THREE→FOUR) — the promotion treatment that has now taken three times (s9 Markdown, s12 naming/closers, s13 tape-authority). The READ contract line carries "every quantity in words, never a digit" at the emission site. Register, direction rules, closers, allowance framing untouched; intended regen on the periodic cadence as triggers fire (prompt_version is in the pre-image, the s16 precedent). // s17, the HEADLINE pass (drop 1 of the headline/body contract, mig 226): the contract grows a second line — `HEADLINE:` after the READ, twelve words or fewer, every word traceable to the read (the shared hook_violation guard enforces it; absent line ⇒ NULL headline, never a failed generation). Worked examples carry the new line so the shape is pinned. Everything else — register, direction rules, no-number rule, closers, allowance framing — untouched. // s16, the PEAK RETIREMENT rider (2026-08-14): the divined top-skill line leaves the FORM TREND card (the scouting read names standouts itself since scout s19), the hash keys go rating_trajectory(_label) with divined_peak dropped, and the Analyst is confirmed as THE seat that leans on the deterministic recent-form marker (now composite-only on a dynamic window — 10% of the entity's scored events, clamped [3,16] — sized in scout/mod.rs). Contract, register, and worked examples otherwise unchanged; the hash-key change is the intended one-time regen. // s15, the PRODUCT-NAME INVERSION (Scott's brief, 2026-08-10 evening, verbatim: "For the Analyst, it should refence Vibe output as something like 'the emotion around the club' versus 'Vibe'. Same with the PEAK."). Rule 1 INVERTS: s14 mandated "call it PEAK... call it Vibe" as product names the seeker knows; s15 requires the sport's own words — the form/the tape for the stat signal, the mood/emotion around the club for the felt signal — and bans the internal labels from the READ outright. THE s13 POSTMORTEM IS THE DESIGN HERE: two earlier attempts to ban this vocabulary failed (101/109, 98/109) because the words sat in the prompt's own input labels ("PEAK TRAJECTORY", "Composite and PEAK z-scores trending up") and the ban only raised their salience. s15 therefore renames the INPUTS first — the section headers, the trajectory-label strings (z_trajectory_label descrubbed at source in scout/mod.rs, which also feeds the Oracle's card), the slope lines — so the READ's ban no longer fights the material it reads. Both worked examples rewritten in the new register (form/mood), same rising+steady pair (the D-T52 lesson: steady needs its own model). Gate: prose_includes:PEAK/Vibe expects replaced by prose_includes_any synonym sets, plus the case-sensitive no_product_names per-reply invariant (shared with rating/oracle). Direction rules, no-number rule, hedge-closer bans, allowance framing: unchanged. // s14, the REGISTER + DIET pass (Scott's brief, 2026-08-10): the nimble prop trader — PEAK trajectory as price action, Vibe/news as sentiment, position with conviction, cut losers without romance — REINSTATING on Scott's word the trader identity s6 retired; the jargon guard keeps the stance out of the vocabulary (the seeker reads a sports card, not a ticker). A worked example lands — momentum's first (the ep6 lesson; 320 production rows had failed as markdown prose with no READ: line, and the shape had no pin beyond the prose rules). Gate grew first (D-T45): prose_no_digits (the no-number rule was ungated; s13 emitted a digit on transfer-noise), the sentence ceiling, "**" excludes, and the set's first decided-RISING and decided-FALLING fixtures — all 8 prior fixtures decided steady, so the commit-to-the-move rules were never gated (s13 then failed to voice "falling" on the new fixture, and "isn't a collapse" fired live on clean-decline). DIET (same pass, the prefill-guard debt): MAX_MOMENTUM_PACKETS 2→1 and the vibe prose capped at 600 bytes in the prompt (parity with the scouting read's cap) — worst-case prompt drops from ~7k to under ~5k tokens; the oMLX guard was killing the fat tail (86 prefill rejections on 2026-08-10 alone). (NOTE: this s14 is unrelated to the unshipped s14 rescue attempt described in the s13 note below.) // s13: WITHDRAWS the z-score/percentile/composite ban that s13 added to this seat and s14 tried to rescue. It was never a measured defect — s13's actual target ("the tape calls this") went to zero and stayed there — it was added on the reasoning that the Oracle bans the same words. Two revisions of evidence say that reasoning does not transfer: 101/109 at s13, 98/109 at s14, and s14 was worse across the board, bringing back hedge closers and "steady band" that s13 had clean. The mechanism is not defiance but proximity: the words sit in the PEAK trajectory label the prompt itself supplies, and both attempts to forbid them raised their salience instead. The Oracle's version of this ban works because ITS bookkeeping words are on peer cards it is told to translate, not in a line it is handed as its own input. If this leak is worth closing it belongs upstream in what the Scout's label says, not in a rule asking the Analyst to unsee its input. Everything s12 and s13 measurably fixed is kept. // (s13 supersedes two uncommitted attempts, kept out of history: a first pass that added this ban, and a second that tried to rescue it by flagging the label as internal. Gate numbers on 109 checks: 101 then 98. Neither shipped.) // s12-era note: s13 fixed its target defect ("the tape calls this" went to zero) but the z-score/composite ban it added failed 8 of 8 — and the gate output showed why: the model was not inventing that vocabulary, it was copying the PEAK trajectory label handed to it in the prompt ("Composite and PEAK z-scores trending up over recent games", which is what production really sends). A rule in the list cannot beat a phrase sitting in the data. s14 marks the data instead: the label renders as "internal wording — say what it MEANS, never reuse its words". The ban itself is new to this seat at s13 and is kept deliberately: this READ is fan-facing, and the Oracle bans the identical words for the identical reason. // s13: the defect s12's new assertions caught on their first run. stats-down-vibe-up-near-zero closed on "the tape calls this a holding pattern", and the same READ wrote "the z-scores dropping steadily" — bookkeeping vocabulary in fan-facing prose, the identical mechanism as the Oracle's or8 R1 regression. Both were banned mid-list since s10 and neither ban took; both are now rule 2 of the prominent block, which is the third time that promotion has fixed a rule the model was ignoring (s9 Markdown, s12 naming and closers). The rule draws the distinction the flat ban never did: the tape is EVIDENCE you read ("the tape backs the form" stays legal), never an authority that renders a verdict ("the tape calls this" is out). // s12: the first GATED momentum revision — s10 and s11 both shipped ungated. Two defects reproduced byte-identically at temp=0 across two full fixture runs, and both were rules that already existed but sat mid-list among thirteen bullets: (a) rating-surge-vibe-flat described PEAK's move as "recent numbers" and never named the signal, failing prose_includes:PEAK; (b) the s10 hedge ban did not take — "for now, this isn't a surge" closed two of eight READs. s10 recorded that ban as 10 -> 0 occurrences, but the grep that measured it used an ASCII apostrophe and ministral emits U+2019, so the 0 was the instrument, not the model (fixed in contains_ci, same branch). s12 promotes both rules to a numbered block directly under the output contract — the treatment that made the s9 no-Markdown guard stick — and deletes their buried duplicates. // s11: the Analyst stops producing a number. Its ±5 score is now computed by momentum_conviction_from_score() from the same ±100 momentum_score that already decides direction — completing North Star #4 on this seat. The seat was miscast: the number is decided by the collision of the Scout rail and the emotional rails, and the Analyst VOICES it. Removing the ask also retires a defect three revisions could not fix (ministral never left {-1,0,1}). Contract is now a single READ line. // s10: s10: sign-and-magnitude pass. MEASURED: ministral-3:14b never left {-1,0,1} on the -5..5 scale across 8 fixtures and two prompt revisions (nemo reached -2 and 3 on the same inputs), and on a RISING entity it returned -1 — a sign-contract violation, not a hedge. s9 blamed padding and was wrong. s10 sets the sign from the decided direction FIRST as arithmetic, then forces the magnitude to match the READ's own adjectives, and hard-bans the leaked "the engine sees this as" / "for now, this isn't a surge" closers. // s9: s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. s8: the peer-length pass — READ grows from "one concise paragraph" to an explicit 5-6 sentences, plus a plain-text/no-Markdown guard (chat-tuned models emit **SCORE:** and the two-line parse fails outright); s7: English-only output guard for multilingual upstream source material; s6: The Analyst voice pass, contract + decided-direction rules unchanged

/// A rail's movement, in words. s19 finishes what s18 started on the direction line.
///
/// s18 stopped handing the READ a figure in the one line that had one, and `digits_in_read`
/// fell from 65% of generations. s19 then removed the prose around the numbers, which made the
/// remaining slope figures the most prominent thing left in the prompt — and the first probe
/// came back with "a 14-point climb over 11 samples", four digits and an instant rejection.
/// Same lesson, third application: the input must not shout what the output may not say. No
/// figure reaches this prompt any more, so rule 2 has nothing left to fight.
///
/// The bands are read off the ±100 slope scale that `momentum_score` shares.
fn rail_movement(slope: Option<f64>, samples: i32) -> String {
    let Some(s) = slope else {
        return "not measured".to_string();
    };
    let size = match s.abs() {
        x if x < 5.0 => "flat",
        x if x < 15.0 => "drifting",
        x if x < 30.0 => "moving clearly",
        _ => "moving hard",
    };
    let way = if s > 0.0 { "up" } else { "down" };
    let confidence = match samples {
        0..=3 => "on a thin sample",
        4..=8 => "on a modest sample",
        _ => "on a healthy sample",
    };
    if size == "flat" {
        format!("flat {confidence}")
    } else {
        format!("{size} {way}, {confidence}")
    }
}

/// Where a rail STANDS, in words — the level, as distinct from the movement above it.
fn mood_level(sentiment: i32) -> &'static str {
    match sentiment {
        i32::MIN..=20 => "very low",
        21..=40 => "low",
        41..=60 => "middling",
        61..=75 => "warm",
        76..=90 => "high",
        _ => "very high",
    }
}

/// build_momentum_prompt assembles the user prompt: the two rails, where they collide, and the
/// decided direction.
///
/// **s19 deletes most of what this function used to render.** Until s19 the Analyst was handed
/// the Scout's brief, the Influencer's felt read, the compiled storylines and the relational
/// memory card, and was then asked for a trajectory read. Measured across eight well-covered
/// teams (2026-08-22): her READ named a trajectory in 57% of cases while touching the stat
/// profile in 42%, the mood in 42%, the news in 42% and transfers in 28% — the worst role bleed
/// of the five seats and the only one that failed to lead on its own job.
///
/// She was not disobeying her contract. It already said "READ narrates the decided direction:
/// what is moving (form vs feeling)... and what tension exists between the two", which is
/// exactly the job. She was narrating her INPUTS, four fifths of which belonged to other seats.
/// That is the s13/s15 postmortem for the third time on this seat — a rule in the list cannot
/// beat a phrase sitting in the model's own input — so this time the input goes, not the rule.
///
/// What is left is what a direction read needs and nothing else: where each rail stands, which
/// way each is moving, where they collide, and the decided direction. This seat's whole value
/// is the RELATIONSHIP between the rails: "the results are poor but the room is high" is her
/// sentence and no other seat's.
///
/// Gone with the prose: `memory` and `packets` parameters. The Analyst leaves the packet rail
/// entirely — she reports on direction, not on stories, and the storylines belong to the three
/// seats that voice them.
pub fn build_momentum_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Entity: {entity_name} ({sport} {entity_type})\n\n"
    ));
    // s15, still load-bearing: the section headers speak the sport's words ("FORM", "MOOD"),
    // never the desk's ("PEAK", "Vibe"). Two banned-word attempts failed BECAUSE these labels
    // kept shouting the banned vocabulary; the input stopped shouting and the ban finally had
    // no material to fight.
    b.push_str("=== THE FORM RAIL (recent statistical performance) ===\n");
    // ONE statement per rail, and her own measurement wins.
    //
    // Until s19 both were rendered, and they contradict each other whenever the two windows
    // disagree — the first s19 probe was handed "Form is: moving hard down" directly above
    // "Form trend: overall scores holding steady", which is the Scout's label read off HIS
    // dynamic window (10% of the season, clamped 3-16) while the slope comes off the momentum
    // window. A seat asked to voice a direction cannot be handed two of them for the same rail.
    //
    // So the slope is the form rail when it exists, and the Scout's label is the FALLBACK for
    // the rating-only context, which is the one case where it is the only form signal there is
    // (`a_vibe_only_context_builds_a_prompt_that_claims_no_form` pins the mirror case).
    match mom.rating_slope {
        Some(_) => b.push_str(&format!(
            "Form is: {}\n",
            rail_movement(mom.rating_slope, mom.rating_samples)
        )),
        None => match rating {
            Some(r) if !r.rating_trajectory_label.trim().is_empty() => {
                b.push_str(&format!("Form is: {}\n", r.rating_trajectory_label))
            }
            Some(r) if !r.rating_trajectory.trim().is_empty() => {
                b.push_str(&format!("Form is: {}\n", r.rating_trajectory))
            }
            _ => b.push_str("Form is: not measured\n"),
        },
    }
    b.push_str("\n=== THE MOOD RAIL (the feeling around them) ===\n");
    match vibe {
        Some(v) => b.push_str(&format!("Mood stands: {}\n", mood_level(v.sentiment))),
        None => b.push_str("Mood level: not measured\n"),
    }
    b.push_str(&format!(
        "Mood is: {}\n",
        rail_movement(mom.vibe_slope, mom.vibe_samples)
    ));
    // The collision of the two rails IS this seat (s11: the number "is decided by the collision
    // of the Scout rail and the emotional rails, and the Analyst VOICES it"). It gets its own
    // section because the relationship between the rails is the READ's subject, not a footnote
    // to either one.
    // No collision FIGURE: the decided-direction line below already carries the same number in
    // words (direction + strength, both off `momentum_score`), so printing the score here would
    // hand the READ a digit for a fact it is already given in prose.
    if mom.empty() {
        b.push_str("\n(no durable momentum snapshot)\n");
    }
    // The decided facts the model narrates (never decides) — the omen pattern. BOTH arrive as
    // WORDS (s18): until then this line handed the READ a figure and the words "steady band" in
    // the same breath it was told to emit neither, and digits_in_read was 65% of momentum
    // generations. The strength ladder mirrors momentum_conviction_from_score exactly, so the
    // prose and the persisted ±5 conviction can never tell two stories.
    match mom.momentum_score {
        Some(score) => {
            let strength = match momentum_conviction_from_score(Some(score)).abs() {
                0 => "genuinely flat — no measured lean",
                1 => "barely visible — a lean, not yet a move",
                2 => "modest but real",
                3 => "clean and well supported",
                4 => "strong — one of the clearer moves on the slate",
                _ => "emphatic — as hard as the scale measures",
            };
            b.push_str(&format!(
                "\nDirection (decided upstream, final): {} — strength of the move, also decided upstream: {}\n",
                momentum_direction_from_score(Some(score)),
                strength
            ));
        }
        None => b.push_str(
            "\nDirection (decided upstream, final): steady (no durable momentum snapshot)\n",
        ),
    }
    b.push_str("\nWrite the Momentum read now.");
    b
}
