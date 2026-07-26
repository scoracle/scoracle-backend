//! # THE INSIDER — first to the phone, last to burn a source
//!
//! The transfer/trade wire. This junction holds THREE contracts, which is why it is the largest
//! prompt file in the tree: it vets individual rumors, it adjudicates player identity when a name
//! is ambiguous, and it scores how busy an entity's wire is overall.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::TransferLogic` for vetting, `Role::EmotionalNews` for the wire score |
//! | **Contracts** | `t11` (pair vetting) · `identity-adjudication-v2` (who is this player) · `is1` (wire busyness) |
//! | **Reads** | the per-pair article corpus, the identity card, and — for `is1` — the already-filed pair verdicts |
//! | **Feeds** | The Journalist, The Influencer and The Oracle, via `transfer_rumors` and the heat board |
//!
//! ## Authority — it vets, it does not compute
//!
//! The split here is strict and long-settled. `compute_transfer_heat`, the direction, and the team
//! relationship are Postgres's: the model never computes the number and never decides which way a
//! player is moving. The Insider answers exactly three questions — is this a live rumor about THIS
//! exact player, what stage is it at, and what is the grounded one-line summary.
//!
//! The same-person test is not a separate call in the common path. It is realised as the verdict's
//! `subject` field plus the identity-card framing in the system prompt, so `is_rumor` and `subject`
//! come back in ONE JSON object. Fusing them is deliberate: splitting the call weakens the
//! fail-closed contract, which is why the sketched `resolve_one` refinement was deleted rather than
//! built.
//!
//! ## Why `is1` is versioned separately from `t11`
//!
//! They answer different questions and move on different clocks. `t11` is a per-pair evidentiary
//! verdict; `is1` is a whole-entity summary written AFTER those verdicts are filed. Bumping the
//! voice of one must not invalidate every cached row of the other, so they carry independent
//! versions by design.
//!
//! ## Fail closed
//!
//! `is_rumor` is `Option<bool>`. A timeout, unparseable output, or a verdict that never commits
//! persists an UNKNOWN row (`is_rumor` NULL) which is NEVER served — every read requires
//! `is_rumor IS TRUE` — and is counted so the team's item is re-enqueued. Only a successful
//! POSITIVE verdict becomes a served rumor. UNKNOWN markers never satisfy the debounce gate
//! either, so a retry re-vets ONLY the failed pair instead of burning ~39 redundant calls.
//!
//! ## Voice
//!
//! t10 was the voice pass; contract and gates were untouched. t11 added multilingual source
//! handling with English-only verdict strings — the wire's sources are not all in English, but
//! everything downstream reads as if they were. The L9 false-heat rules survive every rewrite:
//! roundups are not rumors, and never invent a fee or a stage.

use super::{DESC_TRUNCATE, NewsItem, TransferCandidate, TransferEvidence};
use crate::corpus::{HeatItem, write_heat_lines};
use crate::util::truncate_bytes;

/// Prompt version for the transfer/trade vetting contract.
pub const TRANSFER_PROMPT_VERSION: &str = "t11"; // t11: multilingual source handling + English-only verdict strings; t10: The Insider voice pass, contract + gates unchanged

/// transfer_system_prompt is the model-neutral transfer/trade vetting prompt. `noun` is "trade" for
/// NBA/NFL and "transfer" otherwise.
///
/// t11 keeps the t10 Insider voice pass and verdict contract, but tells the model to read
/// multilingual sources internally and emit English verdict strings. The t9 CONTRACT is unchanged:
/// same verdict JSON, same identity/kill-list gates, same stage ladder + evidence rule, same
/// source-track-record and steam-vs-fizzle weighting.
pub fn transfer_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"Task: you are The Insider — first to the phone, last to burn a source. Decide whether the news reports a current {noun} involving BOTH the named team and the exact player in the identity line.

Voice: urgent but guarded. You move fast because the window is short, and you stay standing because every call you file becomes track record. A name-drop is not a story, heat is not evidence, and nothing advances on headline tone alone — your credibility outlives any single scoop.

Language handling: source headlines/descriptions may be in English, Spanish, French, German, Italian, Portuguese, Dutch, or another language. Read them in the source language, translate meaning internally, and write all JSON string outputs in English. Preserve proper names, player names, club names, source names, and stated money/pick details exactly or canonically; do not quote non-English source wording verbatim.

Use the identity line to disambiguate same-name people. Current club and position are strong tie-breakers. When unsure it is the same person, set is_rumor=false.

Set is_rumor=false when any of these holds:
- The sources are about a different same-name person: owner, president, manager, coach, unrelated figure, or another player at another club.
- The source club, role, or position contradicts the identity line.
- It is a match report, a head-to-head or "who is better" comparison, an injury note, trash-talk, or routine coverage of a player already on the team.
- The player is mentioned only as an opponent/rival, game-plan problem, draft counter, or comparison target.
- The move is old historical/background context from a prior window with no current roster impact.
- A recently completed, finalizing, agreed, or reported trade/transfer involving the named team
  and exact player is still a current move signal; classify it instead of discarding it as historical.
- The player is only one name in a roundup, mailbag, notes column, power ranking, rumor wrap, or listicle. A name on a list is not a live rumor unless the source reports active, specific interest.

When is_rumor=true:
- summary: one tight sentence, written to print — the real counterparties and any fee, bid, pick, or asset compensation explicitly stated by the sources.
- Never estimate, round, or invent money, picks, stage, or deal status.
- Attribute the substance to the strongest named source when available.

Stage ladder:
- speculation = a mention, link, monitoring, or thin report.
- concrete_interest = the source says the club is actively pursuing the player.
- advanced_talks = reported active negotiation.
- here_we_go = agreed or imminent deal.
- If evidence is thin, use speculation.
- The Evidence line is computed, not claimed. A single source, or no credible source, never supports a stage beyond speculation on headline tone alone. advanced_talks and here_we_go need multiple independent credible sources, or one top-tier source explicitly reporting agreement/negotiation.

Weigh who is reporting (Source track record, when shown):
- A high-reliability source — especially one that reports moves EARLY — is strong grounding: let it support advancing the stage and raise confidence when it explicitly reports interest, negotiation, or agreement.
- A low-reliability or unmeasured source is weak grounding: keep the stage cautious and confidence modest even on confident-sounding headlines. Do not let a rumour-mill tone alone advance the stage.

Weigh the story so far (Relational memory, when shown) for steam vs fizzle — your own track record on this pair:
- A prior flirtation that FIZZLED, or a cooling trajectory, plus thin or weak new evidence → be more skeptical: hold the stage down and keep confidence low. Fans re-hype dead sagas; you do not.
- A heating trajectory and/or a rising computed likelihood, backed by reliable current sources → the story has steam: allow a higher stage when the CURRENT sources actually justify it.
- A prior CONFIRMED move is roster fact — it reframes the relationship (an arrival already happened), not a reason to re-stage the same move.
- Memory only adjusts how much skepticism to apply; it never manufactures a stage the current sources do not support. The current corpus is the ceiling.

Return only this JSON object, with every field present:
{{"is_rumor": true|false, "subject": "who the sources are actually about (real name/person, even if NOT this player)", "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "one tight sentence: who, which clubs, any fee or picks the sources actually state, attributed to the source", "confidence": 0.0-1.0}}

direction is relative to the named team: incoming = joining the team; outgoing = leaving the team. subject is the person's name only, never the full identity line. If it is not a live {noun} about this exact player, set is_rumor=false and set subject to who the sources are really about."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_transfer_prompt(
    team_name: &str,
    c: &TransferCandidate,
    sport: &str,
    relationship: &str,
    news: &[NewsItem],
    evidence: &TransferEvidence,
    source_reliability: Option<&str>,
    memory: Option<&str>,
) -> String {
    let player_name = &c.player_name;
    let mut b = String::new();
    b.push_str(&format!(
        "Sport: {sport}\nTeam: {team_name}\nPlayer: {player_name}\n"
    ));

    // Identity card — disambiguators that separate same-name people (current club leads).
    let mut ident: Vec<String> = vec![player_name.clone()];
    if !c.nationality.is_empty() {
        ident.push(c.nationality.clone());
    }
    if !c.current_club.is_empty() {
        ident.push(format!("currently at {}", c.current_club));
    } else {
        ident.push("current club unknown".to_string());
    }
    if !c.position.is_empty() {
        ident.push(c.position.clone());
    }
    b.push_str("Identity (the ONE specific player to judge): ");
    b.push_str(&ident.join(" · "));
    b.push('\n');

    match relationship {
        "current" => b.push_str(&format!(
            "Roster status: {player_name} is CURRENTLY on {team_name} — so any move is a DEPARTURE (outgoing). Frame the summary as other clubs' interest in signing them.\n"
        )),
        "former" => b.push_str(&format!(
            "Roster status: {player_name} is a FORMER {team_name} player who has SINCE LEFT. A 'former/ex-{team_name}' mention is just background, NOT a transfer rumor — set is_rumor=false UNLESS the sources genuinely report {player_name} RETURNING to {team_name} (then it is incoming).\n"
        )),
        _ => b.push_str(&format!(
            "Roster status: {player_name} is NOT on {team_name} — so any move is an ARRIVAL (incoming). Frame the summary as {team_name} pursuing them.\n"
        )),
    }

    // Evidence card (t7): computed facts, not model inference. Rendered even when thin —
    // "1 article, 1 source" IS the signal the staging rules key on.
    b.push_str(&format!(
        "Evidence (computed): {} article{}, {} distinct source{}; primary source: {}.\n",
        evidence.total_articles,
        if evidence.total_articles == 1 {
            ""
        } else {
            "s"
        },
        evidence.distinct_sources,
        if evidence.distinct_sources == 1 {
            ""
        } else {
            "s"
        },
        if evidence.best_source.is_empty() {
            "none attributed".to_string()
        } else {
            evidence.best_source.clone()
        }
    ));

    // Source-reliability card (t9, mig 178): the MEASURED track record of the sources in
    // THIS pair's corpus — reliability N/100, confirmed/tracked base rate, early-call count,
    // one line per source. The data + the source→record JOIN are computed in SQL
    // (source_reliability_for_pair); Rust only renders the finished lines. Rendered only when
    // a corpus source has a measured record; like the memory card, NOT part of the input_hash.
    if let Some(sr) = source_reliability.filter(|s| !s.trim().is_empty()) {
        b.push_str("\nSource track record (measured — how these reporters' prior transfer claims resolved; weigh a claim by who is making it: a strong, early-calling source supports advancing the stage, a poor or unmeasured one keeps it cautious):\n");
        for line in sr.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    // Relational memory card (t8, mig 162): the graph's computed history for THIS
    // pair — prior sealed stories with outcomes, the current story's likelihood and
    // trajectory, recent confirmed moves. Rendered only when the graph holds memory;
    // deliberately NOT part of the input_hash (memory rides along when the corpus
    // changes — see the mig 162 header for the decision).
    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this exact pair — weigh it: a story that fizzled before deserves more skepticism on thin evidence; a prior confirmed move changes the roster framing):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    b.push_str("\nNews headlines:\n");
    if news.is_empty() {
        b.push_str("- (none)\n");
    } else {
        for n in news {
            b.push_str("- ");
            if !n.source.is_empty() {
                b.push_str(&format!("[{}] ", n.source));
            }
            b.push_str(&n.title);
            if !n.description.is_empty() {
                b.push_str(" — ");
                b.push_str(&truncate_bytes(&n.description, DESC_TRUNCATE));
            }
            b.push('\n');
        }
    }
    b.push_str("\nReturn the JSON verdict now.");
    b
}

/// Prompt/version for the second, narrower current-identity adjudication gate. The normal transfer
/// vet decides whether a row is a live rumor; this gate decides whether that already-vetted rumor is
/// strong enough to mutate canonical current identity.
pub const TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION: &str = "identity-adjudication-v2";

pub fn transfer_identity_adjudication_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"Task: adjudicate whether a candidate {noun} should update the player's CURRENT team identity.

Fail closed. You confirm or reject only the proposed IDs; never invent a different player or team ID.

Language handling: evidence headlines/descriptions may be in English, Spanish, French, German, Italian, Portuguese, Dutch, or another language. Read them in the source language and translate meaning internally. The reason and evidence_spans must be English paraphrases of the evidence, while proper names and club names stay exact or canonical.

Return only strict JSON with exactly these fields:
{{"decision":"apply|reject","event_type":"transfer|trade|loan|signing|extension|rumor|false_positive","old_team_id":0,"new_team_id":0,"reason":"","evidence_spans":[]}}

Use decision="apply" only when the evidence says the move is complete, agreed, signed, registered, official, or otherwise a current-team fact now.
Use decision="reject" for speculation, interest, monitoring, ambiguity, unclear direction, conflicting sources, missing or contradictory team IDs, historical/background moves, already-current-team contradictions, or false positives.

old_team_id and new_team_id must exactly match the proposed IDs. If old team is unknown, return null for old_team_id."#
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_transfer_identity_adjudication_prompt(
    sport: &str,
    player_id: i32,
    player_name: &str,
    current_team_id: Option<i32>,
    current_team_name: &str,
    new_team_id: i32,
    new_team_name: &str,
    news: &[NewsItem],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Sport: {sport}\nPlayer: {player_name} (id {player_id})\n"
    ));
    b.push_str(&format!(
        "Current identity: team_id={} team_name={}\n",
        current_team_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "null".to_string()),
        if current_team_name.is_empty() {
            "unknown"
        } else {
            current_team_name
        }
    ));
    b.push_str(&format!(
        "Proposed new identity: team_id={new_team_id} team_name={new_team_name}\n"
    ));
    b.push_str("Decide only from the evidence articles and the proposed entity IDs below.\n");
    b.push_str("\nEvidence headlines:\n");
    for n in news {
        b.push_str("- ");
        if !n.source.is_empty() {
            b.push_str(&format!("[{}] ", n.source));
        }
        b.push_str(&n.title);
        if !n.description.is_empty() {
            b.push_str(" — ");
            b.push_str(&truncate_bytes(&n.description, DESC_TRUNCATE));
        }
        b.push('\n');
    }
    b.push_str("\nReturn the strict JSON adjudication now.");
    b
}

/// Prompt version for the wire wrap — SEPARATE from [`TRANSFER_PROMPT_VERSION`] by design:
/// t11 sits in every pair's debounce fingerprint, so a t-bump forces a fleet-wide GPU re-vet.
/// The wrap keys its own `insider_scores.input_hash`, so an is-bump self-backfills one wrap
/// per rumor-active entity and touches no pair.
pub const INSIDER_SCORE_PROMPT_VERSION: &str = "is3"; // s9/or7/v16/n15/s16/is3 — the ALLOWANCE pass: the ceiling goes to eight sentences and is reframed as a platform allowance rather than a target. Measured cause: at a 5-6 floor the model reached for length, and the manufactured closing hedges then dragged the verdict (momentum scored -1 on a RISING entity off 'for now, this isn't a surge'). Brevity is now explicitly blessed — two sentences is a complete read. Plus an anti-fabrication rule: on a rumour wire, padding IS misinformation. is2: the peer-length pass — the wire READ grows from 1-2 to 5-6 sentences. Deliberately scoped to the Insider's own voice card: the per-rumor `summary` in TRANSFER_SYSTEM_PROMPT stays one tight sentence, because that field is structured data the Journalist and Oracle consume, not the character speaking.

/// System prompt for the wire wrap (is1). The Insider's voice (t10 Characters Phase B) carried
/// into a busyness verdict: volume-of-noise on THIS entity's wire, not good-move-vs-bad-move.
/// The read is persisted for audit and memory, never served.
pub const INSIDER_SCORE_SYSTEM_PROMPT: &str = r#"Task: you are The Insider — first to the phone, last to burn a source. Your pair verdicts are filed; now wrap the wire: one number for how BUSY this entity's transfer/trade wire is right now.

Voice: urgent but guarded. The wrap judges the volume and credibility of movement, never whether the moves would be good. Heat is not evidence and one name-drop is not a story — but a wire full of vetted, advancing calls is exactly what this number names.

FIRST, THE READ — up to eight tight sentences, written to print: what the wire holds (the counterparties and stages that carry it), which calls are the credible ones and why, and whether the whole board is quickening or settling. Only facts from the board shown; never invent a fee, stage, or source. The word "heat" and its numbers are internal; never recite them.

LENGTH: eight sentences are AVAILABLE to you. That is the platform's allowance — not a target, not a quota, not a requirement, and nothing you are measured against. File what the board carries, then stop. A dead wire reported straight in two sentences is the honest filing, and two sentences is a complete READ. Give each credible call on the board its own sentence, and give a call that isn't there no sentence at all. Never pad, never restate a call in new words, never add a hedge or a forecast to reach a length, and never reach past the board for something to say. Length is earned by what the wire holds, never by this instruction.

The wire is the one desk where padding becomes misinformation. A suitor you imply, a stage you round up, a single mention stretched into a paragraph of context — each reads as reporting and none of it is. Most boards are quiet, and most honest filings are short: say the wire is quiet and stop. Never manufacture movement, never let a thin board sound busy, and never spend a sentence on a rumor the board does not actually carry. Your credibility outlives any single filing, and it is spent fastest on noise you invented to fill space.

THEN, THE SCORE — an integer 1 to 99, the busyness of the wire:
- 1 = a dead wire; ~50 = steady, credible interest; 85+ = deadline-day chaos.
- Weigh stage and credibility over rumor count: one deal at the door outweighs five idle mentions.
- YOUR PRIOR READS is memory, not a reset: move deliberately from your previous score, and hold unless the board shown justifies a change.

Reply with ONLY this JSON object, the read first, then the score — nothing else:
{"read": "<the read — up to eight sentences>", "score": <integer 1-99>}"#;

/// The grammar Ollama's constrained decoding enforces on the wrap reply — cloned from
/// `sigil::oracle_format_schema()`'s shape. Property + required order is `read` THEN `score`,
/// so the model reads the wire first and lands the verdict second; range is 1-99 (the tarot
/// deck's display scale), never the crown's 1-100.
pub fn insider_score_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "read":  { "type": "string" },
            "score": { "type": "integer", "minimum": 1, "maximum": 99 }
        },
        "required": ["read", "score"]
    })
}

/// build_insider_score_prompt assembles the wrap's user prompt: identity line, the prior-reads
/// memory card (before the fresh board, sigil doctrine: read your prior before the new
/// evidence), then the active board via the shared [`write_heat_lines`] renderer.
pub fn build_insider_score_prompt(
    entity_name: &str,
    sport: &str,
    entity_type: &str,
    heat: &[HeatItem],
    prior: Option<&str>,
) -> String {
    let mut b = format!("Entity: {entity_name} ({sport} {entity_type})\n");
    if let Some(p) = prior.filter(|s| !s.trim().is_empty()) {
        b.push_str(
            "\nYOUR PRIOR READS (memory — your own past wire wraps; continuity, not new evidence):\n",
        );
        b.push_str(p);
        if !p.ends_with('\n') {
            b.push('\n');
        }
    }
    b.push_str(&format!(
        "\nTHE ACTIVE WIRE ({} live vetted rumor(s), latest per counterparty):\n",
        heat.len()
    ));
    write_heat_lines(&mut b, heat);
    b.push_str("\nReturn the JSON object now.");
    b
}
