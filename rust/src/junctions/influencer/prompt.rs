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
pub static VIBE_SYSTEM_PROMPT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        r#"Task: you are The Influencer — the one who knows what the room is feeling before the room does. Read the supplied stories, find the emotion already running through them, and post the felt read: a score, a hook, and the vibe.

Voice: you live in the feed. Vivid, present tense, first to the feeling — your craft is turning what the crowd already feels into one clean take. You feel it first but you never fake it: no manufactured outrage, no borrowed drama, no bait. When the room is loud, capture the roar; when it is flat, a true quiet read beats a loud false one.

SCORE (1-100). 1 is grim or in freefall, 50 is quiet or genuinely mixed, 100 is euphoric. Weigh stories by impact and let impact set the amplitude — big feelings need big stories, and a quiet cycle stays near 50 however it leans. Reserve under 15 and over 90 for a week that rewrites the entity's story; if you cannot name the seismic event, you are not there. Anchors: a benching with low-heat trade chatter in a quiet week is 35-45; a slump with no departure signal is doubt, not grief, so high 30s to 40s; protests plus a winless month is the 20s; a routine win in a flat week is 45-55; a genuine surge with receipts is the 70s-80s. When a PREVIOUS VIBE is shown it is your prior — move from it deliberately, not freshly.

{card}

HOOK: write it as a tweet, and it is the form's LEAD. 140 characters at most, and shorter lands harder. Present tense. Name the feeling and who carries it, and earn the tap. Punctuation is yours — a colon, a question mark, a twist all land if they earn their place. No caps-lock. The one thing it may not do is run past the card.

{selection}

{form}

{wire}

VIBE: the body of the post, in THE FORM. Your claims are FEELINGS — each paragraph's claim names an emotion and who carries it, its evidence is the stories that prove the room feels it, and its close lands the feeling again. One claim is a card; two is a full one; only a week that rewrites the entity's story earns three. Present tense, written to be read. Name the entity inside the body — it travels without the hook. Name the actual players, clubs and moves behind the dominant threads and let minor items go. Stretch the feeling; never stretch the evidence: you may not invent an event, a number, a quote or a suitor, and you may not imply the room is louder than the signals show. If the cycle is genuinely dead, one honest quiet paragraph is the read.

Reply in plain text, no Markdown:
SCORE: <integer 1-100>
HOOK: <the tweet — 140 characters at most>
VIBE: <the body in THE FORM — paragraphs separated by blank lines>"#,
        card = crate::junctions::form::card_face("HOOK", "a VIBE"),
        selection = crate::junctions::form::CLAIM_SELECTION,
        form = crate::junctions::form::STORY_FORM,
        wire = crate::junctions::form::WIRE_COPY
    )
});

/// Prompt version for the Vibe sentiment + felt-read contract.
/// # THE STORY FORM PILOT (2026-08-25) — contract changed, version deliberately NOT bumped
///
/// Scott's structure, from teaching English: a lead (the HOOK, already the tweet rule), then
/// one paragraph per claim — claim sentence, one to three evidence sentences, a closing
/// sentence that lands the claim again. *"This is going to work for all our voices… because it
/// works for all reporting."* The form itself is `junctions::STORY_FORM`, ONE shared const —
/// the endgame is that each seat's prompt describes only its VOICE and composes the form.
/// The Influencer pilots it: her body is free prose with no internal labels, so only the
/// reply contract ("exactly three lines" → paragraphs under VIBE:) and the parser (which
/// flattened trailing lines to spaces; it now preserves blank-line paragraph breaks) had to
/// move. The "up to six sentences" ceiling is SUPERSEDED by the form's per-claim shape plus
/// the claims-per-card line — the restrictive prompting the form exists to retire. Version
/// NOT bumped per the Twitter-rule precedent below; cut at 2026-08-25 for this change.
/// Later the same day: the tarot block and the WIRE_COPY register compose from
/// `junctions::form` (Scott's dedicated format/structure file).
///
/// **A worked example was tried the same evening and REMOVED, with the measurement:** a
/// numberless invented-club form paragraph (Harborview) was copied VERBATIM onto a real
/// entity's card at temp 0 — hook and all, fabricating the example's collapses and
/// discounted shirt as Sunderland facts. The per-seat law this measured: an example is safe
/// only where it cannot be mistaken for input. The Scout's survives because his input is
/// numeric and the example is prose; the Influencer's input IS prose, so her example blends
/// into the STORIES block and gets cited as evidence. Her form teaching stays abstract
/// (STORY_FORM + the invisible-frame rule); the salvage strips what leaks.
/// # THE TWITTER RULE (2026-08-24) — contract changed, version deliberately NOT bumped
///
/// Scott: *"we don't need to regenerate the corpus. We need to just apply these rules to the new
/// inbound data."* `prompt_version` is folded into `input_components` (s14), so bumping this
/// string reopens every entity in the fleet. Leaving it means existing cards stand and the new
/// contract reaches everything that regenerates from here on its own — new entities, moved stats,
/// and the debounce-bypassing triggers.
///
/// **THE COST, stated rather than hidden:** two cards can both carry this same version string and
/// have been written under DIFFERENT contracts. `generated_at` is the only discriminator, so any
/// eval split or incident analysis keying on `prompt_version` alone must cut at 2026-08-24.
///
/// **The change:** twelve words + no colon + no question mark becomes 140 characters and nothing
/// else — framed as a tweet on a tarot card whose face the headline and the body must both fit.
/// The guard moved with it (`guards::hook_violation`); punctuation is voice and is no longer
/// policed in production.
pub const VIBE_PROMPT_VERSION: &str = "v24"; // v24, THE WAVE (2026-08-25): versions the same-day unbumped run — THE STORY FORM pilot (claims are feelings; paragraph-preserving parse), CLAIM_SELECTION + WIRE_COPY composition, and the removed worked example (the example law) — and joins the deliberate five-voice bump that reopens the fleet for the granite+form regen wave (momentum-s21's note has the full rationale). The articulator corpus gate requires v24+ AND model_version=granite4.2:3b. // v23, the CAP-RESTORE rider (2026-08-23, the review pass): v22's restructure quietly dropped v21's emission-site cap from the reply-contract HOOK line — the line read "<the one-line title>" bare, and hook overruns were still ~60 per 3h (salvage caught the two-beat ones; single-clause overruns dropped the title). v21 measured the emission-site placement as THE move that cut overruns, so the line carries it again ("twelve words or fewer, one clause"). Nothing else changes. // v22, the ROLE pass (2026-08-22, Scott: "these are all getting meshed together into AI slop"). MEASURED across eight well-covered teams: 75% of her felt reads talked TRANSFER MECHANICS — the largest single bleed in the fleet, on the card whose entire job is the emotion in the room. Cause, and it is the same shape as the Analyst's s19: her prompt called `write_heat_lines`, the IDENTICAL function that builds the Insider's own wrap prompt, so she was handed his ledger — every counterparty, direction, stage, confidence and his vetted one-sentence summary per rumour — and she reported it. A seat recites what it is given. So the ledger goes and the TEMPERATURE stays: `transfer_temperature` renders how loud the wire is and whether a departure is in it, in words. She is not cut off from the subject — transfers are among the biggest emotional events in sport, and an emotionally live move still reaches her through the STORIES block, which is MOOD-charged and carries the room's own phrase and the names in it. What she loses is the inventory, which is the part she was reciting. The aggregate is kept deliberately because v20's SCORE anchors are written against it ("low-heat trade chatter", "no departure signal"), and vibe scores feed momentum, so a seat with no transfer signal at all would miscalibrate downstream. Words rather than figures for the momentum-s19 reason: a number in the input is a number the model reaches for. SCORE bands, HOOK contract, register, worked example: untouched. // v21, the HOOK-OVERRUN pass (2026-08-22, the fail-rate session): MEASURED 674 HOOK rejections against 1,651 shipped in 24h — 29% of vibe generations burned on retries, essentially all hook_max_words. The failing hooks share one shape: a clean take plus a dash-hung twist (13-18 words, often closing on a question) — the model writes two beats where the title holds one. Three moves: the cap lands at the EMISSION SITE (the reply-contract HOOK line now reads "twelve words or fewer" — the momentum-s12 lesson that the rule must sit where the model writes), the HOOK list gains the one-clause rule (the twist belongs to the VIBE's first sentence, not the title), and "under twelve" aligns to the guard's ≤12 so the prompt and hook_violation() enforce the same number. SCORE, VIBE, register, worked example untouched. // v20, the CALIBRATION pass (the 08-19 seat gates + DOCTRINE-directing.md): scenario anchors added to SCORE — the abstract bands only held for the incumbent that had internalized the scale; a challenger scored 18 where the honest read was ~40, and vibe scores feed momentum, so miscalibration is numeric corruption downstream. Same day the HOOK contract and body invariants became production guards (guards.rs) — the gate and the parser now enforce the same rules. // v19, the BLOCK-BUDGET pass (the ctx-budget doctrine, Scott 2026-08-15: "we want this lean — the ctx budget helps us stay on track"): PACKET_BLOCK_TRUNCATE 3,000 chars per story block. v18 capped how many stories she reads (packets 4→2); a mega-storyline then spent ~2k tokens on ONE block and 12% of vibe prompts still cleared the 4,096 window (measured 08-15) — everything past it silently truncated before the model read it. Depth is now bounded like breadth; system prompt and register untouched. // v18, the DIET pass (D-T54's census, D-T56's sustained correction): MAX_VIBE_PACKETS 4→2. Vibe was measured as the FATTEST seat (p50 3,315 tok, max 7,808 — worse than anyone assumed; the packet allowance was the driver at ~2k tok/packet) and its tail is what feeds the oMLX prefill-guard retries under sustained drain. The system prompt and register are UNTOUCHED — this is a builder-side diet, versioned because the built prompt changes shape and the fixtures re-freeze. Worst case drops ~7.8k → ~4k tok; the room it frees is what re-opens concurrency 6 (the ≤2k fleet goal). // v17, the REGISTER pass (Scott's brief, 2026-08-10): the platform-native creator — lives in the feed, reads the room before the room reads itself, translates crowd emotion into one clean take; feels first, never fakes — sincerity stays the craft. A worked example lands (the ep6/n18 lesson: the example teaches what prose cannot) with invented entities so it cannot leak content. Gate grew first (the D-T45 rule): the VIBE body had ZERO checks since v6 — prose axes + hook caps authored across all 5 fixtures BEFORE this edit. v16, the ALLOWANCE pass — ceiling to eight sentences. The Influencer is the DELIBERATE EXCEPTION to the pass's brevity framing: where the other five are told an allowance is not a target, she is told to TAKE her space. She is strictly emotional — a YouTuber filling her runtime — and for her the feeling IS the content, so a modest cycle still earns a full room's worth of mood. Her limit is relocated from length to FACT: stretch the feeling, never the evidence. v15: the peer-length pass — the VIBE body grows from 2-3 to 5-6 sentences, plus an explicit plain-text/no-Markdown guard (chat-tuned models bold the labels and the three-line parse drops the HOOK); v14: English-only output guard for multilingual upstream source material; v13: The Influencer voice pass + HOOK card title

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
