//! Transfer verification and identity adjudication contracts.

use super::{NewsItem, TransferCandidate, TransferEvidence, DESC_TRUNCATE};
use crate::util::truncate_bytes;

pub const TRANSFER_PROMPT_VERSION: &str = "t12";

pub const TRANSFER_PROMPT_VERSION_PERSON: &str = "t12-person";

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
    packet_framing: Option<&str>,
) -> String {
    let player_name = &c.player_name;
    let noun_cap = if c.subject_type == "person" {
        "Person"
    } else {
        "Player"
    };
    let noun = if c.subject_type == "person" {
        "person"
    } else {
        "player"
    };
    let mut b = String::new();
    b.push_str(&format!(
        "Sport: {sport}\nTeam: {team_name}\n{noun_cap}: {player_name}\n"
    ));

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
    b.push_str(&format!("Identity (the ONE specific {noun} to judge): "));
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

    if let Some(sr) = source_reliability.filter(|s| !s.trim().is_empty()) {
        b.push_str("\nSource track record (measured — how these reporters' prior transfer claims resolved; weigh a claim by who is making it: a strong, early-calling source supports advancing the stage, a poor or unmeasured one keeps it cautious):\n");
        for line in sr.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    if let Some(m) = memory.filter(|m| !m.trim().is_empty()) {
        b.push_str("\nRelational memory (computed history for this exact pair — weigh it: a story that fizzled before deserves more skepticism on thin evidence; a prior confirmed move changes the roster framing):\n");
        for line in m.lines() {
            b.push_str("- ");
            b.push_str(line);
            b.push('\n');
        }
    }

    let packet_framing = packet_framing.filter(|f| !f.trim().is_empty());
    if let Some(f) = packet_framing {
        b.push_str("\nThe story these reports belong to (assembled from the reads — framing, not evidence):\n");
        b.push_str(f);
        if !f.ends_with('\n') {
            b.push('\n');
        }
    }

    b.push_str(if packet_framing.is_some() {
        "\nWhat the reports claim (⇄ = contradicted by another report below; both stand):\n"
    } else {
        "\nNews headlines:\n"
    });
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
