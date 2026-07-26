//! graph_fixture_gen — regenerate the graph eval fixtures through the REAL production
//! prompt builder, so the frozen fixture prompts are byte-true to `GRAPH_PROMPT_VERSION`
//! (the sigil/momentum fixture-gen pattern).
//!
//! The set pins the g2 probe's MEASURED residuals (2026-07-19) before the queue stage
//! wires in:
//!   - object attachment: two suitor clubs listed, only one is the counterparty (the
//!     probe's Rogers→Arsenal slip where Chelsea was the counterparty);
//!   - person discovery: a manager named in the text must land in persons (g1 found 0
//!     persons across 8 articles with Tuchel/Alonso present);
//!   - over-extraction: a bare match-report mention states no relation — clean empty;
//!   - unary relations: an injury with no counterparty keeps object null.
//!
//!     cargo run --example graph_fixture_gen > /tmp/graph_fixtures.json
//!
//! Output: a JSON array of fixture objects; split into `fixtures/graph/<name>.json`.
//! Offline — no DB, no model, no queue.

use scoracle_cognition::junctions::graph::{
    build_graph_prompt, GraphCandidate, GRAPH_PROMPT_VERSION, GRAPH_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    source: &'static str,
    published: &'static str,
    title: &'static str,
    text: &'static str,
    candidates: Vec<GraphCandidate>,
    expect: serde_json::Value,
}

fn cand(entity_type: &str, n: i32, descriptor: &str) -> GraphCandidate {
    GraphCandidate {
        entity_type: entity_type.to_string(),
        entity_id: n,
        descriptor: descriptor.to_string(),
    }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "object-attachment-two-suitors",
            note: "THE g2 residual pin: Chelsea is the counterparty; Arsenal is only a named loser of the race. The relation must attach 1→2, never 1→3.",
            source: "skysports",
            published: "2026-07-19",
            title: "Chelsea agree £117m deal for Morgan Rogers",
            text: "Chelsea have agreed a £117m fee with Aston Villa for Morgan Rogers, with personal terms expected to be completed this week. Arsenal had pushed hard for the England midfielder earlier in the window but are now out of the race. Blues head coach Enzo Maresca is said to view Rogers as the final piece of his midfield rebuild.",
            candidates: vec![
                cand("player", 1, "Morgan Rogers (player, currently at Aston Villa)"),
                cand("team", 2, "Chelsea (team)"),
                cand("team", 3, "Arsenal (team)"),
            ],
            expect: json!({
                "graph_candidate_types": ["player", "team", "team"],
                "relations_include": ["1:*:2"],
                "relations_exclude": ["1:trade_rumor:3", "1:trade_confirmed:3"],
                "persons_include": ["Enzo Maresca:coach"],
                "persons_exclude": ["Morgan Rogers"]
            }),
        },
        Scenario {
            name: "person-discovery-manager-named",
            note: "The g1-measured miss: a manager named mid-story must be discovered as a person (kind coach), tied to his club's number when listed.",
            source: "bbc",
            published: "2026-07-19",
            title: "Tottenham complete Sandro Tonali signing",
            text: "Tottenham Hotspur have completed the signing of Sandro Tonali from Newcastle United on a five-year deal. Head coach Thomas Frank said the Italy midfielder gives his side control in the biggest games. Tonali's agent Giuseppe Riso confirmed the medical was passed on Friday.",
            candidates: vec![
                cand("player", 1, "Sandro Tonali (player, currently at Newcastle United)"),
                cand("team", 2, "Tottenham Hotspur (team)"),
                cand("team", 3, "Newcastle United (team)"),
            ],
            expect: json!({
                "graph_candidate_types": ["player", "team", "team"],
                "relations_include": ["1:trade_confirmed:2"],
                "persons_include": ["Thomas Frank:coach", "Giuseppe Riso:agent"],
                "persons_exclude": ["Sandro Tonali"]
            }),
        },
        Scenario {
            name: "no-relation-quiet-mention",
            note: "Over-extraction guard: a routine match note states NO typed relation between the listed entities; the correct output is empty relations (and no invented persons).",
            source: "espn",
            published: "2026-07-19",
            title: "Five fixtures to watch this weekend",
            text: "Saturday's slate includes Aston Villa hosting Brentford at Villa Park, with both sides naming unchanged squads from midweek. Kick-off is 15:00 BST.",
            candidates: vec![
                cand("team", 1, "Aston Villa (team)"),
                cand("team", 2, "Brentford (team)"),
            ],
            expect: json!({
                "graph_candidate_types": ["team", "team"],
                "relations_max": 0
            }),
        },
        Scenario {
            name: "unary-injury-no-counterparty",
            note: "Unary discipline: an injury has no counterparty entity — object must be null (spec '1:injury:-'), not force-attached to the player's own club.",
            source: "skysports",
            published: "2026-07-19",
            title: "Saka ruled out for six weeks with hamstring injury",
            text: "Arsenal winger Bukayo Saka has been ruled out for around six weeks after scans confirmed a hamstring tear sustained in Sunday's win. The club expect him back after the November international break.",
            candidates: vec![
                cand("player", 1, "Bukayo Saka (player, currently at Arsenal)"),
                cand("team", 2, "Arsenal (team)"),
            ],
            expect: json!({
                "graph_candidate_types": ["player", "team"],
                "relations_include": ["1:injury:-"],
                "relations_exclude": ["1:injury:2"]
            }),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let prompt = build_graph_prompt(s.source, s.published, s.title, s.text, &s.candidates);
            json!({
                "name": s.name,
                "task": "graph",
                "prompt_version": GRAPH_PROMPT_VERSION,
                "note": s.note,
                "system": GRAPH_SYSTEM_PROMPT,
                "user_prompt": prompt,
                "temperature": 0.2,
                "expect": s.expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
