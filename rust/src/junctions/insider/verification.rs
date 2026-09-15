//! Transfer verification and identity adjudication contracts.

use super::{NewsItem, TransferCandidate, TransferEvidence, DESC_TRUNCATE};
use crate::runtime::util::truncate_bytes;

pub const TRANSFER_PROMPT_VERSION: &str = "t13";

pub const TRANSFER_PROMPT_VERSION_PERSON: &str = "t13-person";

pub fn transfer_system_prompt(sport: &str) -> String {
    let noun = if sport == "NBA" || sport == "NFL" {
        "trade"
    } else {
        "transfer"
    };
    format!(
        r#"Determine whether the reporting describes a current {noun} or coaching move involving the named team and exact subject.

Use the identity record to distinguish current role and club from career history. A coach discussing recruitment is not the recruit. Quotes, co-mentions, former playing clubs, opponents and comparisons do not establish a move. Records are dated context; current attributed reporting may supersede them.

Set is_rumor=true only when a source reports this subject joining or leaving this team, including an agreed or recently completed move. Otherwise set false. Do not turn roster status into interest. Keep the actual recruit in subject when the sources concern someone else.

Direction is relative to the named team: incoming, outgoing, or unclear. Stage follows the reporting: speculation = links/monitoring; concrete_interest = active pursuit; advanced_talks = negotiation; here_we_go = agreed/imminent. Source history informs credibility; prior readings and heat do not prove a move. Preserve uncertainty.

Write one attributed summary sentence in English. Preserve names and stated fees, picks or terms; invent none.

Return JSON with every field:
{{"is_rumor": true|false, "subject": "the person whose move is reported", "direction": "incoming"|"outgoing"|"unclear", "stage": "speculation"|"concrete_interest"|"advanced_talks"|"here_we_go", "summary": "attributed finding", "confidence": 0.0-1.0}}"#
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
        "Affiliation: {player_name} is not recorded at {team_name}. This alone establishes no pursuit or move; determine whether either is actually reported.\n"
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
        b.push_str("\nEntity memories for this proposed relationship:\n");
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

pub const TRANSFER_IDENTITY_ADJUDICATION_PROMPT_VERSION: &str = "identity-adjudication-v3";

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

Use decision="apply" only when the supplied evidence says the move is complete, agreed, signed, registered, official, or otherwise a current-team fact now. An explicitly supplied official current-season statistics observation is a current-team fact, but it does not establish a signing date or transaction subtype.
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
    current_team_stats_season: Option<i32>,
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
    b.push_str(
        "Decide only from the supplied source evidence and the proposed entity IDs below.\n",
    );
    if let Some(season) = current_team_stats_season {
        b.push_str(&format!(
            "Official current-season statistics observation: the source feed records this player for proposed team_id={new_team_id} in season {season}. This establishes current-team affiliation as observed during that season. It does not establish a signing date; do not invent one.\n"
        ));
    }
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
