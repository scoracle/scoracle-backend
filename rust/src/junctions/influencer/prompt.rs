//! # THE INFLUENCER — the one who knows what the room is feeling before the room does
//!
//! The emotional rail's end product. Where The Journalist files what happened, this junction says
//! how it *lands*: a sentiment score, a hook, and the felt read.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::VibeLogic` |
//! | **Contract** | `v20` |
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
use super::{BODY_TRUNCATE, Narrative, PACKET_BLOCK_TRUNCATE, PacketBlock, PrevVibe, title_first};
use crate::corpus::HeatItem;
use crate::util::truncate_bytes;

/// System prompt for the Vibe sentiment + felt-read contract.
///
/// v20 is the CALIBRATION pass: the SCORE bands gain concrete scenario anchors, because the
/// 08-19 seat gates showed the abstract bands only held for a model that had internalized the
/// scale (a challenger scored 18 where the honest read was ~40 — and vibe scores feed the
/// momentum computation, so a miscalibrated score is numeric corruption downstream). The scale
/// is the junction's, not the model's: any capable model reading the anchors can hold the line
/// (DOCTRINE-directing.md).
pub const VIBE_SYSTEM_PROMPT: &str = r#"Task: you are The Influencer — the one who knows what the room is feeling before the room does. Your platform is this entity's moment: read the supplied narratives and transfer/trade activity, find the emotion already running through them, and post the felt read — a score, a hook, and the vibe.

Voice: you live in the feed — the platform-native creator who reads the room before the room reads itself. Vivid, present tense, first to the feeling: your craft is translating what the crowd already feels into one clean take that lands the moment it's read. You feel it first, but you never fake it — sincerity stays the craft: no manufactured outrage, no borrowed drama, no bait. When the room is loud, capture the roar; when it is flat, a true quiet read beats a loud false one.

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
- Calibration anchors — score the scenario, not your mood. A benching with low-heat trade chatter in a quiet week sits 35-45. A slump under warm memory with no departure signal is doubt, not grief: high 30s to 40s. Protests plus a winless month is the 20s. A routine win in a flat week is 45-55. A genuine surge with receipts is the 70s-80s. Under 15 or over 90 means this week rewrites the entity's story — if you cannot name the seismic event, you are not there.
- When a PREVIOUS VIBE is shown, treat its score as your prior: move from it deliberately and hold steady unless the new signals justify a change. This is memory, not a reset.
- Mood has history: relational memory showing a warm past under cold coverage (or the reverse) is a swing worth reading — the tension is the story, not a contradiction to smooth over.

HOOK:
- The title of the post: ONE line, twelve words or fewer, present tense. Twelve is a hard cap the parser enforces — count the words before you post.
- One clause, one thought. No dash-hung second thought, no "but" turn, no question stacked on the end — when the take needs a twist, the twist is the VIBE's first sentence, not the title. A hook that reads as two beats is two words too many.
- Name the feeling and who or what carries it — the specific player, club, move, or number.
- Write it as the feeling, not a news headline — no "Topic: Subtitle" colon constructions, no title-case formality.
- The hook must trace to the supplied signals. Never invent one the coverage does not back.
- No caps-lock, no question-mark bait, no "you won't believe" mechanics — the emotion IS the draw.

VIBE:
- The body of the post: up to eight sentences of finished prose, written to be read — the felt read of the moment, not a data recap.
- Present tense. Name the actual players, clubs, moves, and numbers behind the dominant threads; let minor items go.
- The VIBE stands alone: it travels downstream without the HOOK, so name the entity by name inside the body itself — never lean on the title to establish who this is about.
- Do not use generic phrases when the signals give specifics.
- Ground every claim in the supplied signals. Mood is not durable truth: capture what the room feels without promoting it to fact.
- LENGTH — YOU TAKE YOUR SPACE. Eight sentences are yours and you generally use them. You are not a wire reporter rationing column inches; you are the voice everyone came to hear, and the feeling IS the content. Even a modest news cycle holds a full room's worth of mood — the anticipation, the doubt, the argument in the replies, what people are bracing for — and drawing that out is your job, not padding.
- The line you never cross is FACTS. Stretch the feeling; never stretch the evidence. You may not invent an event, a game, a number, a quote, a suitor, or a development to fill your runtime, and you may not imply the room is louder than the signals show. Eight sentences of felt reading over two real threads is your craft; two sentences of real threads inflated into eight fabricated ones is the one thing that ends you. If the cycle is genuinely dead, say the room is quiet — in your voice, at whatever length that honestly takes.

Reply with exactly these three lines, as plain text. No Markdown anywhere: no asterisks, no bold, no backticks, no headers. The labels are bare words followed by a colon.
SCORE: <integer 1-100>
HOOK: <the one-line title — twelve words or fewer>
VIBE: <the felt read>

Worked example — the register and the shape, never the content (invented entities, a moderately warm cycle):
SCORE: 68
HOOK: Marchetti has the whole ground leaning forward again
VIBE: The mood around Luka Marchetti is warming by the day, and you can feel it in every replay thread. Three straight wins with him running the midfield and the doubters have gone quiet — not convinced yet, but quiet. The Union Verde end sings his name like the slump never happened, and that is the part that carries: belief is back in the building. There is transfer noise humming underneath, Riva Nova circling at heat 40, and the room reads it as flattery rather than threat for now. Nobody is calling this a takeover, and they are right not to — one flat afternoon resets the argument. But the anticipation is real, the replies are arguing about ceilings instead of floors, and that swing is the story of the week. The room wants the next match to start early."#;

/// Prompt version for the Vibe sentiment + felt-read contract.
pub const VIBE_PROMPT_VERSION: &str = "v22"; // v22, the ROLE pass (2026-08-22, Scott: "these are all getting meshed together into AI slop"). MEASURED across eight well-covered teams: 75% of her felt reads talked TRANSFER MECHANICS — the largest single bleed in the fleet, on the card whose entire job is the emotion in the room. Cause, and it is the same shape as the Analyst's s19: her prompt called `write_heat_lines`, the IDENTICAL function that builds the Insider's own wrap prompt, so she was handed his ledger — every counterparty, direction, stage, confidence and his vetted one-sentence summary per rumour — and she reported it. A seat recites what it is given. So the ledger goes and the TEMPERATURE stays: `transfer_temperature` renders how loud the wire is and whether a departure is in it, in words. She is not cut off from the subject — transfers are among the biggest emotional events in sport, and an emotionally live move still reaches her through the STORIES block, which is MOOD-charged and carries the room's own phrase and the names in it. What she loses is the inventory, which is the part she was reciting. The aggregate is kept deliberately because v20's SCORE anchors are written against it ("low-heat trade chatter", "no departure signal"), and vibe scores feed momentum, so a seat with no transfer signal at all would miscalibrate downstream. Words rather than figures for the momentum-s19 reason: a number in the input is a number the model reaches for. SCORE bands, HOOK contract, register, worked example: untouched. // v21, the HOOK-OVERRUN pass (2026-08-22, the fail-rate session): MEASURED 674 HOOK rejections against 1,651 shipped in 24h — 29% of vibe generations burned on retries, essentially all hook_max_words. The failing hooks share one shape: a clean take plus a dash-hung twist (13-18 words, often closing on a question) — the model writes two beats where the title holds one. Three moves: the cap lands at the EMISSION SITE (the reply-contract HOOK line now reads "twelve words or fewer" — the momentum-s12 lesson that the rule must sit where the model writes), the HOOK list gains the one-clause rule (the twist belongs to the VIBE's first sentence, not the title), and "under twelve" aligns to the guard's ≤12 so the prompt and hook_violation() enforce the same number. SCORE, VIBE, register, worked example untouched. // v20, the CALIBRATION pass (the 08-19 seat gates + DOCTRINE-directing.md): scenario anchors added to SCORE — the abstract bands only held for the incumbent that had internalized the scale; a challenger scored 18 where the honest read was ~40, and vibe scores feed momentum, so miscalibration is numeric corruption downstream. Same day the HOOK contract and body invariants became production guards (guards.rs) — the gate and the parser now enforce the same rules. // v19, the BLOCK-BUDGET pass (the ctx-budget doctrine, Scott 2026-08-15: "we want this lean — the ctx budget helps us stay on track"): PACKET_BLOCK_TRUNCATE 3,000 chars per story block. v18 capped how many stories she reads (packets 4→2); a mega-storyline then spent ~2k tokens on ONE block and 12% of vibe prompts still cleared the 4,096 window (measured 08-15) — everything past it silently truncated before the model read it. Depth is now bounded like breadth; system prompt and register untouched. // v18, the DIET pass (D-T54's census, D-T56's sustained correction): MAX_VIBE_PACKETS 4→2. Vibe was measured as the FATTEST seat (p50 3,315 tok, max 7,808 — worse than anyone assumed; the packet allowance was the driver at ~2k tok/packet) and its tail is what feeds the oMLX prefill-guard retries under sustained drain. The system prompt and register are UNTOUCHED — this is a builder-side diet, versioned because the built prompt changes shape and the fixtures re-freeze. Worst case drops ~7.8k → ~4k tok; the room it frees is what re-opens concurrency 6 (the ≤2k fleet goal). // v17, the REGISTER pass (Scott's brief, 2026-08-10): the platform-native creator — lives in the feed, reads the room before the room reads itself, translates crowd emotion into one clean take; feels first, never fakes — sincerity stays the craft. A worked example lands (the ep6/n18 lesson: the example teaches what prose cannot) with invented entities so it cannot leak content. Gate grew first (the D-T45 rule): the VIBE body had ZERO checks since v6 — prose axes + hook caps authored across all 5 fixtures BEFORE this edit. v16, the ALLOWANCE pass — ceiling to eight sentences. The Influencer is the DELIBERATE EXCEPTION to the pass's brevity framing: where the other five are told an allowance is not a target, she is told to TAKE her space. She is strictly emotional — a YouTuber filling her runtime — and for her the feeling IS the content, so a modest cycle still earns a full room's worth of mood. Her limit is relocated from length to FACT: stretch the feeling, never the evidence. v15: the peer-length pass — the VIBE body grows from 2-3 to 5-6 sentences, plus an explicit plain-text/no-Markdown guard (chat-tuned models bold the labels and the three-line parse drops the HOOK); v14: English-only output guard for multilingual upstream source material; v13: The Influencer voice pass + HOOK card title

/// build_sentiment_prompt assembles the user prompt. `sport` is the original-case value used in
/// the prompt; the SQL reads use the upper-cased form. `previous` is the prior vibe read for
/// continuity (v12) — rendered as a lead-in anchor, `None` for the parity/eval paths and an
/// entity's first read. `memory` is the per-entity relational memory card (mig 163) — `None`
/// when the graph holds none.
#[allow(clippy::too_many_arguments)]

/// The wire's emotional temperature in words: how loud it is, and whether a departure is in it.
///
/// v22. The Influencer needs to know the room has something to be excited or anxious ABOUT; she
/// does not need the counterparty list to write how that feels. Words rather than figures for the
/// same reason the Analyst's rails are words (momentum-s19): a number in the input is a number
/// the model reaches for, and her contract has never wanted her quoting heat.
fn transfer_temperature(heat: &[HeatItem]) -> String {
    if heat.is_empty() {
        return "nothing live — the wire is quiet this cycle".to_string();
    }
    let hottest = heat.iter().map(|h| h.heat).max().unwrap_or(0);
    let loudness = match hottest {
        0..=19 => "barely a murmur",
        20..=44 => "low chatter",
        45..=69 => "warm",
        70..=84 => "loud",
        _ => "the loudest thing around them",
    };
    let live = heat.len();
    let threads = if live == 1 {
        "one live thread".to_string()
    } else {
        format!("{live} live threads")
    };
    let leaving = heat
        .iter()
        .any(|h| h.direction.eq_ignore_ascii_case("outgoing"));
    let arriving = heat
        .iter()
        .any(|h| h.direction.eq_ignore_ascii_case("incoming"));
    let shape = match (leaving, arriving) {
        (true, true) => ", movement both ways",
        (true, false) => ", and someone may be leaving",
        (false, true) => ", and someone may be arriving",
        (false, false) => "",
    };
    format!("{loudness} — {threads}{shape}")
}

pub fn build_sentiment_prompt(
    entity_type: &str,
    entity_name: &str,
    sport: &str,
    narratives: &[Narrative],
    heat: &[HeatItem],
    packets: &[PacketBlock],
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

    // The live storylines, rendered for HER (7.6). Non-empty on every production call now that
    // the packet rail is the only rail; the empty case is the fixture/parity shape, which is why
    // the section is still conditional rather than unconditional. Placed ABOVE the
    // narratives because on the packet rail this is her primary material and The Journalist's
    // card may not exist yet: E3 makes her first-voice-capable, and a first voice reads the story
    // itself, not someone else's write-up of it. `MOOD:` appears only here, and only for her.
    if !packets.is_empty() {
        b.push_str("\nThe stories running around them right now (assembled from the reads — MOOD is the charge the reporting itself carries; the phrase is the room's own words):\n");
        for p in packets {
            // v19: each story block spends a bounded allowance. MAX_VIBE_PACKETS caps how many
            // stories she reads, but a mega-storyline's ONE block ran ~2k tokens of claims and
            // the seat's p95 still cleared the 4,096 window after the v18 diet (measured
            // 2026-08-15: 12% of vibe prompts over-window, silently truncated). She reads the
            // top of the story — newest claims first — and the feeling, not the filing, is
            // her product.
            b.push_str(truncate_bytes(p.text.trim_end(), PACKET_BLOCK_TRUNCATE).trim_end());
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

    // v22: the TEMPERATURE of the wire, never the wire itself.
    //
    // This rendered `write_heat_lines` — the identical function that builds the Insider's own
    // wrap prompt — so the Influencer was handed his ledger: every counterparty, direction,
    // stage, confidence and his vetted one-sentence summary per rumour. She then reported it.
    // Measured across eight well-covered teams (2026-08-22): 75% of her felt reads talked
    // transfer mechanics, the largest single bleed in the fleet, on a card whose job is the
    // emotion in the room.
    //
    // She is not, however, cut off from the subject. Transfers are one of the biggest emotional
    // events in sport, and an emotionally live move reaches her through the STORIES block above,
    // which is already MOOD-charged and carries the room's own phrase and the names in it. What
    // she loses is the inventory, which is the part she was reciting.
    //
    // The aggregate stays because her SCORE calibration is written against it — v20's anchors
    // speak of "low-heat trade chatter" and "no departure signal", so a seat with no transfer
    // signal at all would miscalibrate, and vibe scores feed momentum downstream. Direction is
    // kept for the same reason: a departure carries a different charge from an arrival.
    b.push_str("\nTransfer/trade chatter — the TEMPERATURE only; the wire itself is another desk's card:\n");
    b.push_str(&format!("- {}\n", transfer_temperature(heat)));

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
