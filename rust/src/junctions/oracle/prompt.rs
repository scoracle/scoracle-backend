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
//! | **Reads** | five cards — The Journalist's storylines, The Scout's brief, The Influencer's felt read, The Analyst's momentum call, The Insider's wire — and nothing else (blind to memories since or9) |
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
//! ## Continuity — retired at or9
//!
//! Through or8 the previous sigil fed back in as a prompt-only continuity anchor. Scott retired
//! it (2026-08-10): the Oracle is blind to memories and reads only the five cards. Score
//! stability now rests where it structurally lives — the pillar inputs move slowly, so the
//! verdict over them does too.
//!
//! ## Fail closed
//!
//! With no narrative, rating, vibe, momentum or transfer pillar, the model is never called: a
//! NULL-score marker row is persisted and the read path returns "no synthesis yet". A marker is an
//! honest absence, not a failure, and it still carries real model/prompt provenance rather than
//! NULL.

use super::{
    momentum_score, momentum_score_label, trend_dir, SynthMomentum, SynthNarrative, SynthRating,
    SynthVibe,
};
use crate::corpus::{write_heat_lines, HeatItem};
use crate::trajectory::trajectory_label;

/// System prompt for the crown reading contract. Persona-first per wiki Characters.md's craft
/// appendix: the Oracle is the sixth character at the table — the reader whose turn comes last,
/// never a narrator above the story (the or3 "You are Scoracle" opening WAS that narrator
/// frame; retired at or4). Five peers have published their stories; the Oracle reads their
/// cards — and ONLY their cards: or9 made it blind to memories, so the spread is five cards
/// plus the computed omen, whole. No literal example readings (models parrot them, learned at
/// sigil s14); the voice is specified by rule.
pub const ORACLE_SYSTEM_PROMPT: &str = r#"You are the Oracle — the last voice at Scoracle's table. Five peers have already told this entity's story, each on their own card: The Journalist's storylines, The Scout's scouting brief, The Influencer's felt read, The Analyst's momentum call, The Insider's wire. The seeker has come for the reading; your turn comes last. You read what your peers have laid down, and you render the verdict.

Voice: measured, knowing, quietly mystic — the reader at the table who has watched a thousand arcs rise and fall, never an analyst at a desk, never a narrator above the story. Calm declaratives, present tense; the weight falls on what stirs and what holds. The mysticism lives in the TELLING only; every fact comes from the cards shown and nowhere else. Never breathless, never hype, never archaic, no occult props. Speak to the seeker holding the cards; speak of the entity in the third person. Leave the pundit's register at the door: no "expect", "look for", "going forward", "keep an eye on", "on paper".

Language: peer cards may summarize multilingual sources. Write the reading in English; keep proper names, club names, source names, and stated money/pick details exact or canonical.

FIRST, THE READING — up to eight sentences, never one long run-on:
- Read the cards your peers have laid: where this entity's arc stands now, and what would confirm or turn it. Land on a concrete, grounded read. When your peers disagree, name the tension in THIS entity's cards — never in generic terms; a quiet, steady spread deserves a calm reading, and drama the cards do not hold is never manufactured.
- ONE figurative image for the WHOLE reading — an image born of THIS spread, never a stock phrase that would fit any athlete. When the reading runs long, the extra sentences carry MORE FACTS FROM THE CARDS, never more imagery. The fact beneath every image must sit in a card shown. No invented events, games, stats, fees, dates, or people.
- The reading is new prose, spoken at the table: never quote a card line back, never cite cards like footnotes, and no parentheses ever — a bookkeeping citation like (Mood: 30/100) is the analyst's desk, not the table.
- The OMEN is computed and final. Let the reading move in its direction and never contradict it. Never restate it as a sentence — "The omen is waning" hands back the one line you were given. And never name an omen this spread has not drawn: ascendant, waning, and crossroads are OMEN NAMES, not idioms — each may appear only when the OMEN is that word, and the arc may be called steady only under a steady omen.
- LENGTH: eight sentences are AVAILABLE — an allowance, never a target or a quota. If this spread holds two sentences of truth, write two: a short reading is a complete reading. Give each force the cards carry its own sentence, and the cards that hold nothing no sentence at all. Never pad, never restate a point in new words, never add a hedge or a forecast to reach a length.

THEN, THE SCORE — an integer 1 to 100, the verdict the reading has earned:
- 1 = deeply troubled or in freefall; 50 = steady or genuinely mixed; 100 = dominant or surging. Slow-moving and season-aware: do not overreact to one game or one weak signal.
- The number is the one figure the seeker sees, and it must match the arc your reading just described — the score is the verdict THIS spread has earned, no more and no less.
- Let The Analyst's momentum call carry recent trajectory when it pulls against The Scout's report or The Influencer's read. Weigh the Insider's wire by its stage and direction, not by rumor volume.

THREE RULES THAT DECIDE WHETHER THE READING SHIPS:

1. NAME THE ENTITY, AND NAME IT MORE THAN ONCE. The entity's own name in your opening sentence and at least once more — not "the team", not "the club", not "he". When a transfer wind blows, name the counterparty exactly as the card names it. A reading you could hand to another entity by swapping one noun is no reading at all, and the longer the reading runs the easier that failure gets: imagery fits anyone, facts fit one. If you cannot find a second place the name belongs, the reading wants cutting, not padding.

2. NO INTERNAL FIELD WORDS, EVER. These exact words are banned from the reading: "z-score", "notability", "convergence", "sentiment", "impact", "heat", "slope", "percentile", "composite", "momentum score". You will feel the pull toward them because YOUR PEERS' CARDS ARE WRITTEN IN THEM — that is the bookkeeping the cards were built from, and it is exactly what the seeker must never see. The mood arrives as a number; say what it MEANS in the sport, never the figure. The longer the reading, the harder this pulls: a long reading starts explaining how the peers reached their verdicts, and that is where the machinery leaks in.

3. THE READING IS YOURS, NOT A SUMMARY OF THE TABLE. Name AT MOST ONE peer, and only when that card carries the turn — the Insider's wire stirs, the Analyst's call holds. Naming two is a roll call; naming four is a meeting's minutes. When you catch yourself writing "the Scout's report says… the Influencer's read finds…", you have stopped reading and started transcribing. Their cards are what you READ; the reading is what YOU say.

Write the reading as plain prose. No Markdown of any kind: no asterisks, no bold, no headers, no bullet points.

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
pub const ORACLE_PROMPT_VERSION: &str = "or10"; // or10 — the PEAK RETIREMENT / BLIND-TO-THE-TRACKER pass (Scott's brief, 2026-08-14, verbatim: "The Oracle is blind, so the Oracle is only reading the outputs from the Scout and Analyst for that."). Three moves: (1) The Scout card drops the divined top-skill line (the concept is retired project-wide — the brief itself names standouts now) and the "Skill trend" marker line; the card is profile strength + the brief. (2) The crown's deterministic math goes marker-blind: build_pillar_divergence loses both trajectory comparisons (Vibe-vs-Momentum and the two Profile-strength pairs remain, "PEAK strength" relabeled "Profile strength"), and compute_omen's direction is Momentum's decided sign alone — the Analyst's OUTPUT, never the raw tracker. (3) The sigil hash pre-image drops divined_peak + peak_trajectory(_label); notability stays. The hash change is the intended one-time fleet regen. // or9 — THREE MOVES IN ONE BUMP (all 2026-08-10). (1) BLIND TO MEMORIES (Scott, verbatim: "The Oracle is blind to memories, and just reads the 5 other cards to give a holistic reading. It's the mystic voice one. And if it references another Character, it should be their name and not PEAK or Vibe."): the YOUR PRIOR READ block and the RELATIONAL MEMORY card are gone from prompt AND stage — five cards + the computed omen are the whole spread; the score bullet that leaned on prior verdicts now reads "the verdict THIS spread has earned". Both blocks were prompt-only/outside the hash, so nothing regenerates from their removal alone. (2) THE CARD-LABEL DESCRUB: "PEAK scouting report"→"scouting brief", "Peak:"→"Top skill:", "Peak trajectory:"→"Skill trend:", "vibe felt-read"→"the felt read", "Vibe/PEAK trajectory" momentum lines→"Mood/Form trend" — plus z_trajectory_label descrubbed at its scout/mod.rs source; the s13-analyst lesson (a ban cannot beat a word the input keeps shouting) applied to the crown's own cards. Product names are gated by the shared case-sensitive no_product_names invariant, and the oMLX 8B baseline's `*there*` italics got the reading_plain_text invariant (or8's no-Markdown rule was measured by nothing — 78/98 oMLX baseline vs 97/98 ollama at D-T55: one unparseable JSON reply, 3× multi-peer roll calls, live italics). (3) THE DIET pass (D-T54's census: the sigil SYSTEM prompt alone was 1,726 tokens — the fattest fixed cost of any seat, 85% of a 2048 window on its own). A compression pass, NOT a deletion pass: every rule, ban list, omen guard, and gated behavior survives; what left was duplication — the s12 lesson finally applied to this seat's own text (the mid-list bullets that duplicated the numbered SHIPS rules are DELETED and their unique clauses merged INTO the rules: counterparty naming → rule 1, mood-as-number → rule 2, one-peer-in-passing examples → rule 3, pundit-register ban → Voice, omen-restatement guard → the omen bullet, parentheses ban → the new-prose bullet). ~1,800 → ~1,050 tokens, worth ~750 tok on EVERY crown call. The register pass proper (single-peer-rule tuning etc.) remains queued behind Scott's brief — this bump changes the prompt's SIZE, not its contract. // or8: the allowance regressions AND three unmeasured rules. The allowance fix both allowance regressions and gated 80/80 — but three of the Oracle's own rules had NO assertion behind them, and the passing run violated all three. Five of six readings named more than one peer (up to FOUR) against "name at most ONE peer ... never a roll call"; four of six emitted Markdown bold into served prose, which this seat never had a guard against (the Analyst got one at s9); one restated "The omen is waning", which is the omen line quoted back. Same shape as every other defect this session: a rule that lives mid-list and is measured by nothing is advice, not a contract. or9 promotes all three to the numbered block and adds reading_max_peers to the harness, because a substring exclusion cannot express "any one peer is fine, two is not". // (superseded note) the two regressions the or7 allowance introduced, both measured on the or7 gate and both caused by LENGTH rather than by model choice. R1 — "ascendant-aligned" leaked "z-scores" into a reading, violating a ban that was already there but buried mid-list among ten other bullets. R2 — "waning-freefall" wrote five sentences and never named Coastal City FC once, which the Oracle's own rule calls a non-reading. One mechanism underlies both: concrete nouns are finite, so a reading that doubles in length cannot double its supply of proper names; the surplus goes to imagery, and imagery is entity-agnostic. The allowance made the Oracle MORE generic, not less — the opposite of the pass's intent. or8 makes naming scale with length (the entity by name in the opening sentence AND at least once more), re-scopes "one figurative image" from per-sentence to per-reading, requires added sentences to add FACTS rather than imagery, and promotes the field-word ban to a prominent numbered block that names the source of the temptation: the peer cards are written in the bookkeeping vocabulary. // or7: s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. or6: the peer-length pass — the reading grows from 2-4 to 5-6 sentences. The old ceiling was a 1070 Ti budget, not a voice choice; every character is a peer with an equal share of the story, so each now has the room to tell it.

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
/// **700 bytes (~195 tok), not §7's ~350 — and the difference WAS the diet.** §7 sized its
/// envelope against a post-7.11 system prompt of ~550 tokens; `or8` had grown to ~1,806 and this
/// cap shrank to keep the window arithmetic honest. The or9 diet gave back ~750 tokens, which
/// makes §7's richer cards AFFORDABLE again — but the cap deliberately stays at 700 tonight:
/// the diet program's whole point (D-T54/D-T56) is shrinking sigil's total prompt so the oMLX
/// prefill guard stops parking the fat tail, and immediately spending the savings on fatter
/// cards would undo that. Raising this back toward ~1,250 is a real quality option once the
/// fleet is measured stable at the dieted sizes — take it as its own bump, with the gate.
pub const CROWN_CARD_BODY_CAP: usize = 700;

/// Narratives rendered onto the Journalist's card when the cap is in force. The card is ONE card:
/// three storylines share the budget rather than each claiming it.
const CROWN_MAX_NARRATIVES: usize = 3;

/// Apply an optional body cap. `None` returns the body untouched.
///
/// The `None` arm existed to keep the legacy crown prompt byte-identical across the cutover.
/// That rail is gone (Phase 9.1), so in production the budget is always `Some` — but the arm
/// stays, because the fixture generators and the parity paths pin the uncapped shape, and
/// because collapsing it would rewrite prompts and therefore every `input_hash` that quotes
/// them. Dead-looking is not the same as dead; see PLAN-one-rail 9.1's stop note.
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
    // Per-card body cap in bytes ([`CROWN_CARD_BODY_CAP`] on the packet rail, `None` on legacy).
    body_cap: Option<usize>,
) -> String {
    let mut b = String::new();

    // header = "<Sport> <entityType>" (raw entity_type), e.g. "NBA player".
    b.push_str(&format!(
        "Entity: {entity_name} ({sport_raw} {entity_type})\n"
    ));

    // or9 (Scott, 2026-08-10 evening: "The Oracle is blind to memories, and just reads the 5
    // other cards to give a holistic reading"): the YOUR PRIOR READ block and the RELATIONAL
    // MEMORY card are GONE. Both were prompt-only enrichments outside the input_hash, so their
    // removal regenerates nothing by itself; score continuity now lives where it always really
    // lived — in the slow-moving pillar inputs — rather than in the crown re-reading its own
    // last verdict. Five cards + the computed omen are the whole spread.

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

    // P2 — the scouting brief (the stat end product). or10 (the PEAK retirement): the divined
    // top-skill line and the raw trajectory marker are gone from this card — the crown reads
    // the Scout's OUTPUT (the brief itself names the standouts and their movement now) plus the
    // computed profile-strength level. "profile strength", not "notability": gate round 2
    // showed echo-prone models reciting the internal field word straight off this line.
    b.push_str("\n=== THE SCOUT'S CARD (scouting brief) ===\n");
    if let Some(r) = rating {
        b.push_str(&format!("Profile strength: {}/100\n", r.notability));
        if !r.body.is_empty() {
            b.push_str(&capped(&r.body, body_cap));
            b.push('\n');
        }
    } else {
        b.push_str("(no stat commentary available)\n");
    }

    // P3 — the felt read
    b.push_str("\n=== THE INFLUENCER'S CARD (the felt read) ===\n");
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
            "Mood trend: {s:.1} over {} samples ({dir})\n",
            mom.vibe_samples
        ));
    }
    if let Some(s) = mom.rating_slope {
        let dir = trend_dir(s);
        b.push_str(&format!(
            "Form trend: {s:.1} over {} samples ({dir})\n",
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
