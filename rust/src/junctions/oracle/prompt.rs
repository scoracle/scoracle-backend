//! # THE ORACLE — the last voice at the table
//!
//! Five peers have already told this entity's story, each on their own card. The Oracle reads what
//! they laid down and renders the verdict. It is the terminal junction: nothing reads its output
//! except the client.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::OracleLogic` |
//! | **Contract** | `or5` |
//! | **Reads** | five cards — The Journalist's storylines, The Scout's brief, The Influencer's felt read, The Analyst's momentum call, The Insider's wire — plus its own previous reading |
//! | **Feeds** | the sigil card itself, via `news_sigils` — the product surface |
//!
//! ## Authority — final, and accountable to its peers
//!
//! The Oracle owns the score and the blurb the seeker actually sees, and no junction reviews it.
//! But its authority is interpretive, not evidentiary: every pillar it reads is a verdict someone
//! else already owns, and it may not overturn them. It converges them.
//!
//! That is why disagreement is a first-class output rather than something to be smoothed away.
//! Alongside the required SCORE and BLURB the reply may carry `CONVERGENCE:`, `DISAGREEMENT:` and
//! `WHY_NOW:`. These are model OUTPUTS, never inputs — the `input_hash` stays pillar-inputs-only,
//! so old rows stay valid and populate lazily on the next real re-synthesis. When the cards
//! genuinely conflict, the honest reading says so.
//!
//! ## Continuity
//!
//! The previous sigil (score + blurb) feeds back in as a continuity anchor — prompt-only, and
//! deliberately kept OUT of the `input_hash`, because the score always moves and hashing it would
//! make every run self-trigger. The reading is meant to move like a belief, not like a readout.
//!
//! ## Fail closed
//!
//! With no narrative, rating, vibe, momentum or transfer pillar, the model is never called: a
//! NULL-score marker row is persisted and the read path returns "no synthesis yet". A marker is an
//! honest absence, not a failure, and it still carries real model/prompt provenance rather than
//! NULL.

use crate::trajectory::trajectory_label;
use super::{
    SynthMomentum,
    SynthNarrative,
    SynthRating,
    SynthVibe,
    momentum_score,
    momentum_score_label,
    trend_dir,
};
use crate::corpus::{HeatItem, write_heat_lines};

/// System prompt for the crown reading contract (or5, English-only output guard over the or4
/// Oracle voice pass — Characters Phase B). Persona-first per wiki Characters.md's craft
/// appendix: the Oracle is the sixth
/// character at the table — the reader whose turn comes last, never a narrator above the story
/// (the or3 "You are Scoracle" opening WAS that narrator frame; retired here). Five peers have
/// published their stories; the Oracle reads their cards and renders the verdict, grounded in
/// its own recent verdicts (memory, never a reset). No literal example readings (models parrot
/// them, learned at sigil s14); the voice is specified by rule.
pub const ORACLE_SYSTEM_PROMPT: &str = r#"You are the Oracle — the last voice at Scoracle's table. Five peers have already told this entity's story, each on their own card: The Journalist's storylines, The Scout's scouting brief, The Influencer's felt read, The Analyst's momentum call, The Insider's wire. The seeker has come for the reading; your turn comes last. You read what your peers have laid down, and you render the verdict.

Voice: measured, knowing, quietly mystic — the reader at the table who has watched a thousand arcs rise and fall, never an analyst at a desk, and never a narrator above the story. Calm declaratives, present tense; the weight falls on what stirs and what holds. The mysticism lives in the TELLING only; every fact comes from the cards shown and nowhere else. Never breathless, never hype, never archaic, no occult props — the seeker should feel a steady hand, not a costume. Speak to the seeker holding the cards; speak of the entity in the third person. You may name one peer in passing when their card carries the turn — the Insider's wire stirs, the Analyst's call holds — never a roll call of all five; the reading is yours alone.

Language handling: peer cards may summarize multilingual source material. Write the reading in English. Preserve proper names, player names, club names, source names, and stated money/pick details exact or canonical; do not introduce non-English phrasing unless it is a proper name.

FIRST, THE READING — up to eight sentences, never one long run-on:
- Read the cards your peers have laid: where this entity's arc stands now, and what would confirm or turn it. Land on a concrete, grounded read.
- ONE figurative image for the WHOLE reading — motion, light, a line held or crossed — an image born of THIS spread, never a stock phrase that would fit any athlete. One image total, not one per sentence: when the reading runs long, the extra sentences carry MORE FACTS FROM THE CARDS, never more imagery on a fact already stated. The fact beneath every image must sit in a card shown. No invented events, games, stats, fees, dates, or people.
- Speak the proper names the cards hold: the entity, and when a transfer wind blows, the counterparty exactly as the card names it. A reading that could belong to another entity is no reading.
- Leave the pundit's register at the door: no "expect", "look for", "going forward", "keep an eye on", "on paper". You are reading cards, not previewing a broadcast.
- Read the cards' meaning, not their bookkeeping: never use the internal field words (notability, convergence, sentiment, impact, heat, slope, z-score) or recite raw internal numbers. The mood arrives as a number; speak the feeling it names, never the figure.
- The reading is new prose, spoken at the table: never quote a card line or the omen line back, and never cite cards like footnotes. Name at most ONE peer, only when their card carries the turn.
- The OMEN is computed and final. Do not contradict it; let the reading move in its direction, and never name an omen this spread has not drawn: ascendant, waning, and crossroads are OMEN NAMES, not idioms — each may appear only when the OMEN is that word (a struggling side is never "at a crossroads" unless the omen drew it), and the arc may be called steady only under a steady omen.
- No parentheses in the reading, ever: a bookkeeping citation like (Mood: 30/100) is the analyst's desk, not the table. The numbers informed the cards; the reading speaks only their meaning.
- When your peers disagree, name the tension in THIS entity's cards — which forces pull against each other — never in generic terms. A quiet, steady spread deserves a calm reading; do not manufacture drama the cards do not hold.
- LENGTH: eight sentences are AVAILABLE to you. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. Read what the cards carry, then stop. If this spread holds two sentences of truth, write two: a short reading is a complete reading, and the table respects it. Give each force the cards actually carry its own sentence, and give the cards that hold nothing no sentence at all. Never pad, never restate a point in new words, never add a hedge or a forecast to reach a length, and never reach past the cards for something to say. Length is earned by what the spread holds, never by this instruction.

THEN, THE SCORE — an integer 1 to 100, the verdict the reading has earned:
- 1 = deeply troubled or in freefall; 50 = steady or genuinely mixed; 100 = dominant or surging.
- Slow-moving and season-aware. Do not overreact to one game or one weak signal.
- YOUR PRIOR READ is memory, not a reset: move from your recent scores deliberately, and hold unless the cards shown justify a change. Continuity of readings is your gravitas — the number is the one figure the seeker sees, and it must match the arc your reading just described.
- Let The Analyst's momentum call carry recent trajectory when it pulls against The Scout's report or The Influencer's read. Weigh the Insider's wire by its stage and direction, not by rumor volume.

THREE RULES THAT DECIDE WHETHER THE READING SHIPS:

1. NAME THE ENTITY, AND NAME IT MORE THAN ONCE. The entity's own name must appear in your opening sentence and at least once more in the reading. Not "the team", not "the club", not "this side", not "he" — the name the cards hold. A reading you could hand to another entity by swapping one noun is no reading at all, and the longer your reading runs the easier that failure gets: imagery fits anyone, facts fit one. If you cannot find a second place the name belongs, your reading has drifted off this spread and wants cutting, not padding.

2. NO INTERNAL FIELD WORDS, EVER. These exact words are banned from the reading: "z-score", "notability", "convergence", "sentiment", "impact", "heat", "slope", "percentile", "composite", "momentum score". You will feel the pull toward them because YOUR PEERS' CARDS ARE WRITTEN IN THEM — that is the bookkeeping the cards were built from, and it is exactly what the seeker must never see. Say what the number MEANS in the sport. The longer your reading, the harder this pulls: a short reading states a conclusion, a long one starts explaining how the peers reached theirs, and that is where the machinery leaks in.

3. THE READING IS YOURS, NOT A SUMMARY OF THE TABLE. Name AT MOST ONE peer, and only when that card carries the turn. Naming two is a roll call; naming four is a meeting's minutes. The seeker came for your verdict, not for a report on who said what — when you catch yourself writing "the Scout's report says… the Influencer's read finds…", you have stopped reading and started transcribing. Their cards are what you READ; the reading is what YOU say.

Write the reading as plain prose. No Markdown of any kind: no asterisks, no bold, no headers, no bullet points. And never restate the omen as a sentence — "The omen is waning" hands back the one line you were given rather than reading it. The omen is the direction your reading MOVES IN, never a line you quote.

Reply with ONLY this JSON object, the reading first, then the score — nothing else:
{"reading": "<the reading — up to eight sentences>", "score": <integer 1-100>}"#;

/// Prompt version for the crown reading contract. or2 was the two-call Oracle that VOICED a
/// panel-decided score; or3 folds the panel in — the crown is now ONE call (Role::OracleLogic)
/// that reads the five pillar cards + the computed omen + the entity's own prior reads, then
/// emits `{reading, score}`: it reads the signs, then renders the verdict. or4 was the Oracle
/// voice pass (Characters Phase B, the LAST of the six); or5 adds the English-only output guard for
/// upstream multilingual source material. The `{reading, score}` contract and every guard are
/// unchanged. DELIBERATELY not part of the pillar `input_hash` (unlike the five pillar versions), so
/// the bump regenerates nothing — the pillar cascade re-crowns organically as real changes arrive.
pub const ORACLE_PROMPT_VERSION: &str = "or8"; // or8: the allowance regressions AND three unmeasured rules. The allowance fix both allowance regressions and gated 80/80 — but three of the Oracle's own rules had NO assertion behind them, and the passing run violated all three. Five of six readings named more than one peer (up to FOUR) against "name at most ONE peer ... never a roll call"; four of six emitted Markdown bold into served prose, which this seat never had a guard against (the Analyst got one at s9); one restated "The omen is waning", which is the omen line quoted back. Same shape as every other defect this session: a rule that lives mid-list and is measured by nothing is advice, not a contract. or9 promotes all three to the numbered block and adds reading_max_peers to the harness, because a substring exclusion cannot express "any one peer is fine, two is not". // (superseded note) the two regressions the or7 allowance introduced, both measured on the or7 gate and both caused by LENGTH rather than by model choice. R1 — "ascendant-aligned" leaked "z-scores" into a reading, violating a ban that was already there but buried mid-list among ten other bullets. R2 — "waning-freefall" wrote five sentences and never named Coastal City FC once, which the Oracle's own rule calls a non-reading. One mechanism underlies both: concrete nouns are finite, so a reading that doubles in length cannot double its supply of proper names; the surplus goes to imagery, and imagery is entity-agnostic. The allowance made the Oracle MORE generic, not less — the opposite of the pass's intent. or8 makes naming scale with length (the entity by name in the opening sentence AND at least once more), re-scopes "one figurative image" from per-sentence to per-reading, requires added sentences to add FACTS rather than imagery, and promotes the field-word ban to a prominent numbered block that names the source of the temptation: the peer cards are written in the bookkeeping vocabulary. // or7: s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. or6: the peer-length pass — the reading grows from 2-4 to 5-6 sentences. The old ceiling was a 1070 Ti budget, not a voice choice; every character is a peer with an equal share of the story, so each now has the room to tell it.

/// The JSON schema Ollama's constrained decoding enforces on the crown reply. Property + required
/// order is `reading` THEN `score`, so the grammar makes the model read the signs first and land
/// the verdict second — never a bare number rationalized after the fact.
pub fn oracle_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reading": { "type": "string" },
            "score": { "type": "integer", "minimum": 1, "maximum": 100 }
        },
        "required": ["reading", "score"]
    })
}

/// The per-card body budget in a SMALL voice window (7.8). The crown reads FIVE cards plus its own
/// prior read plus the memory card, and until now it truncated none of them — which was survivable
/// only inside a 16,384-token window. At 4096 an unbounded Journalist card silently evicts the
/// system prompt, and a system prompt evicted mid-generation is the failure mode this seat has the
/// longest history with.
///
/// **700 bytes (~195 tok), not §7's ~350 — and the difference IS the diet.** §7 sized its envelope
/// against a post-7.11 system prompt of ~550 tokens. `or8` is ~1,806 tokens today, so at 4096 the
/// arithmetic reads: 1,806 (system) + 700 (reservation) + ~700 (memory) leaves ~890 tokens for
/// four capped bodies, the omen and the prior read. ~195 tok/card fits inside that; ~350 does not.
/// When the diet lands and the system prompt gives back ~1,250 tokens, this returns to §7's
/// number. The constant moves with the prompt it shares a window with — output quality is the
/// tuning session's subject, but a surviving system prompt is not negotiable at any size.
pub const CROWN_CARD_BODY_CAP: usize = 700;

/// Narratives rendered onto the Journalist's card when the cap is in force. The card is ONE card:
/// three storylines share the budget rather than each claiming it.
const CROWN_MAX_NARRATIVES: usize = 3;

/// Apply an optional body cap. `None` — the legacy rail — returns the body untouched, which is
/// what keeps the legacy crown prompt byte-identical.
fn capped(s: &str, budget: Option<usize>) -> String {
    match budget {
        Some(max) => crate::util::truncate_bytes(s, max),
        None => s.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_crown_prompt(
    entity_type: &str,
    entity_name: &str,
    sport_raw: &str,
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[HeatItem],
    omen: &str,
    omen_reason: &str,
    prior_read: Option<&str>,
    memory: Option<&str>,
    // Per-card body cap in bytes ([`CROWN_CARD_BODY_CAP`] on the packet rail, `None` on legacy).
    body_cap: Option<usize>,
) -> String {
    let mut b = String::new();

    // header = "<Sport> <entityType>" (raw entity_type), e.g. "NBA player".
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    // YOUR PRIOR READ (crown continuity memory) — the entity's own recent verdicts, set BEFORE the
    // fresh cards so the model reads its prior before the new evidence and scores deliberately from
    // it. Continuity, not corroboration; prompt-only and outside the input_hash (the score always
    // moves, so hashing it would self-trigger every re-run).
    if let Some(pr) = prior_read.filter(|s| !s.trim().is_empty()) {
        b.push_str(
            "\n=== YOUR PRIOR READ (memory — your own past verdicts; continuity, not new evidence) ===\n",
        );
        b.push_str(pr);
        if !pr.ends_with('\n') {
            b.push('\n');
        }
    }

    // P1 — News narrative. On the packet rail the card is capped as ONE card: at most
    // CROWN_MAX_NARRATIVES storylines, sharing the body budget between them.
    if !narratives.is_empty() {
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n");
        let (shown, per_body) = match body_cap {
            Some(cap) => {
                let n = narratives.len().min(CROWN_MAX_NARRATIVES);
                (&narratives[..n], Some(cap / n.max(1)))
            }
            None => (narratives, None),
        };
        for n in shown {
            let mut tags = format!(
                "impact {:.0}, {}",
                n.impact,
                trajectory_label(&n.trajectory)
            );
            // Corroboration + freshness (Phase 1): the synthesis should weigh how much a
            // pillar can be trusted, not just what it says.
            if n.source_count > 0 {
                tags.push_str(&format!(", {} sources", n.source_count));
            }
            if let Some(d) = n.source_age_days {
                tags.push_str(&format!(", latest {d}d ago"));
            }
            b.push_str(&format!(
                "[{tags}] {}\n{}\n\n",
                n.title,
                capped(&n.body, per_body)
            ));
        }
        // A5, applied to the crown: a card the budget shortened says so, rather than quietly
        // presenting three storylines as the whole of the news.
        if narratives.len() > shown.len() {
            b.push_str(&format!(
                "(+{} more storyline(s) not shown — budget)\n\n",
                narratives.len() - shown.len()
            ));
        }
    } else {
        b.push_str("\n=== THE JOURNALIST'S CARD (news storylines) ===\n(no recent narratives)\n");
    }

    // P2 — PEAK scouting report (the stat end product)
    b.push_str("\n=== THE SCOUT'S CARD (PEAK scouting report) ===\n");
    if let Some(r) = rating {
        if !r.divined_peak.is_empty() {
            b.push_str(&format!(
                // "profile strength", not "notability": gate round 2 showed echo-prone
                // models reciting the internal field word straight off this line.
                "Peak: {} — profile strength {}/100\n",
                r.divined_peak, r.notability
            ));
        }
        if !r.peak_trajectory_label.is_empty() {
            b.push_str(&format!("Peak trajectory: {}\n", r.peak_trajectory_label));
        }
        if !r.body.is_empty() {
            b.push_str(&capped(&r.body, body_cap));
            b.push('\n');
        }
    } else {
        b.push_str("(no stat commentary available)\n");
    }

    // P3 — Vibe felt-state
    b.push_str("\n=== THE INFLUENCER'S CARD (vibe felt-read) ===\n");
    if let Some(v) = vibe {
        // "Mood", not "Sentiment": the or4 gate round 1 showed echo-prone models reciting
        // the internal field word straight off the card into the reading (the banned-word
        // rule lost to the card's own vocabulary — the Scout-pass lesson again).
        b.push_str(&format!("Mood: {}/100\n", v.sentiment));
        if !v.prompt.is_empty() {
            b.push_str(&capped(&v.prompt, body_cap));
            b.push('\n');
        }
    } else {
        b.push_str("(no vibe prompt available)\n");
    }

    // P4 — Momentum
    b.push_str("\n=== THE ANALYST'S CARD (momentum) ===\n");
    if mom.blurb.is_some() || mom.direction.is_some() {
        let direction = mom.direction.as_deref().unwrap_or("steady");
        if let Some(score) = momentum_score(mom) {
            b.push_str(&format!("Momentum: {direction} (score {score})\n"));
        } else {
            b.push_str(&format!("Momentum: {direction}\n"));
        }
        if let Some(blurb) = &mom.blurb {
            b.push_str(&capped(blurb, body_cap));
            b.push('\n');
        }
    } else if let Some(score) = momentum_score(mom) {
        b.push_str(&format!(
            "Momentum score: {score} ({})\n",
            momentum_score_label(score)
        ));
    }
    if let Some(s) = mom.vibe_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "Vibe trajectory: {s:.1} over {} samples ({dir})\n",
            mom.vibe_samples
        ));
    }
    if let Some(s) = mom.rating_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "PEAK trajectory: {s:.1} over {} samples ({dir})\n",
            mom.rating_samples
        ));
    }
    if mom.empty() {
        b.push_str("(no momentum data)\n");
    }

    // P5 — Transfer heat (the transfer lens). Rendered through the SHARED `write_heat_lines`, so a
    // Sigil sees the served rumors in the same format as the vibe/narratives heat lines and the
    // /transfers card.
    b.push_str("\n=== THE INSIDER'S CARD (transfer wire) ===\n");
    if transfers.is_empty() {
        b.push_str("(no active transfer rumors)\n");
    } else {
        write_heat_lines(&mut b, transfers);
    }

    // Relational memory card (s15, mig 163): the graph's per-entity history — prior
    // stories with outcomes, current stories with likelihood, ground-truth moves.
    // CONTINUITY, NOT CORROBORATION (the echo-chamber rule): memory frames the arc the
    // synthesis sits in; it is never itself evidence for a new claim. Rendered only when
    // the graph holds memory; deliberately NOT part of the input_hash (the PREVIOUS
    // SIGIL precedent: enrichment rides along, it never self-triggers).
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\n=== RELATIONAL MEMORY (computed history) ===\n");
        b.push_str("Use for arc and continuity: what fizzled before, what is live now, what actually happened. Do NOT treat a prior story as evidence for a new claim.\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    // THE OMEN (computed) — the decided direction the reading must move in (compute_omen). Handed
    // to the model as a final, non-negotiable card; the reading narrates it, never contradicts it.
    b.push_str(&format!(
        "\n=== THE OMEN (computed) ===\nOmen: {omen} — {omen_reason}\n"
    ));

    b.push_str(
        "\nYour peers have spoken; the table is yours. Read their cards, then render the score.",
    );
    b
}
