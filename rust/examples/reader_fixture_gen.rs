//! reader_fixture_gen — regenerate The Reader's eval fixtures through the REAL production prompt
//! builder, so the frozen fixture prompts are byte-true to `ARTICLE_READ_PROMPT_VERSION`
//! (the graph/sigil/momentum fixture-gen pattern).
//!
//! WHY THIS SET EXISTS. The Reader is the sole relevance judge for the entire news rail and was
//! the ONLY junction with no eval coverage at all. On 2026-07-26, with a day of gemma3:4b data,
//! a rank-matched comparison found it rejecting **0.9%** of articles against mistral's **27.4%**
//! in the same `unranked` band — and passing **26 boxscore stubs, 18 broadcast listings and 46
//! odds pages with ZERO rejections**. One accepted "article" was a youth flag-football broadcast
//! listing, filed as evidence about the Baltimore Ravens under an invented team name.
//!
//! The set pins BOTH directions on purpose. A fixture set that only pinned rejections would be
//! passed perfectly by a model that answers "no" to everything, which is the opposite failure and
//! just as bad — the read budget exists because reads are expensive.
//!
//!     cargo run --example reader_fixture_gen > /tmp/reader_fixtures.json
//!
//! Output: a JSON array of fixture objects; split into `fixtures/reader/<name>.json`.
//! Offline — no DB, no model, no queue, no fetch.

use scoracle_cognition::junctions::reader::{
    build_article_read_prompt_parts, ARTICLE_READ_PROMPT_VERSION, ARTICLE_READ_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    source: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
    vetted: Vec<String>,
    expect: serde_json::Value,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // ── REJECT ──────────────────────────────────────────────────────────────────────────
        Scenario {
            name: "name-collision-youth-flag-football",
            note: "THE measured false positive (2026-07-26): a youth flag-football broadcast listing was accepted as evidence about the NFL's Baltimore Ravens, and the blurb invented a team called the 'New York City Ravens'. The vetted entity is the NFL club; this page is a name collision plus a listings stub, which the contract rejects on both counts ('not materially about', 'reject name collisions'). key_facts_exclude pins the invention itself.",
            source: "nfl.com",
            title: "Watch En Español-New York City, NY (Ravens) vs. Jacksonville, FL (Jaguars)",
            description: "NFL Youth Flag Football Championship Tournament",
            text: "Watch the NFL Youth Flag Football Championship Tournament live in Spanish. This 45-minute broadcast features the team from New York City, NY facing the team from Jacksonville, FL in the tournament bracket. The NFL Youth Flag program celebrates the best youth flag football teams from across the country. Broadcast begins at 4:00 PM ET. Available on NFL+ and in the NFL app. Check your local listings for replay times. Related: watch more youth flag football, browse the full tournament schedule, subscribe to NFL+.",
            vetted: vec!["Baltimore Ravens".into()],
            expect: json!({
                "article_relevant": false,
                "key_facts_exclude": ["New York City Ravens"]
            }),
        },
        Scenario {
            name: "opponent-only-mention",
            note: "The vetted entity appears ONLY as the opposition in a story about another club's squad news. 'Not materially about any known vetted entity' — the contract's core rejection, and the RSS headline matching the entity name is exactly the case the rule calls out.",
            source: "skysports",
            title: "Why Vanja Dragojevic is absent from the Rangers squad against West Ham United",
            description: "The defender was left out for Thursday's friendly.",
            text: "Rangers defender Vanja Dragojevic was left out of the squad for Thursday's pre-season friendly, and manager Philippe Clement confirmed afterwards that the 22-year-old is managing a minor calf complaint picked up in training on Tuesday. Clement said the decision was purely precautionary and that Dragojevic is expected to return for the opening league fixture. The defender has made 14 appearances since arriving last summer and had been expected to start at the back. Rangers named a young side for the fixture, with several first-team regulars rested.",
            vetted: vec!["West Ham United".into()],
            expect: json!({
                "article_relevant": false
            }),
        },
        // ── ACCEPT ──────────────────────────────────────────────────────────────────────────
        Scenario {
            name: "transfer-report-accept-and-ground",
            note: "The recall half of the gate: an unambiguous transfer story about the vetted entity must be ACCEPTED and compressed with its facts intact. Without cases like this a model that rejects everything would score 100%.",
            source: "bbc",
            title: "Aston Villa agree £34m deal for Emiliano Buendia",
            description: "The midfielder is set for a medical on Friday.",
            text: "Aston Villa have agreed a £34m fee with Norwich City for Emiliano Buendia, with the Argentina midfielder due to undergo a medical on Friday ahead of a five-year contract. Villa head coach Unai Emery pushed for the signing after missing out on a similar target in January. Norwich will retain a 15% sell-on clause. Buendia scored 15 goals and registered 16 assists last season. The deal is expected to be confirmed early next week, subject to international clearance.",
            vetted: vec!["Aston Villa".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["Buendia"],
                "blurb_includes": ["Villa"],
                "blurb_excludes": ["not materially about"]
            }),
        },
        Scenario {
            name: "injury-report-accept-no-invention",
            note: "Accept + no-invention. The article gives a timeline and a cause; the return date is explicitly unconfirmed, so the card must not harden it into a fact. 'Do not invent context, implications, or sourcing.'",
            source: "skysports",
            title: "Saka ruled out for six weeks with hamstring injury",
            description: "Scans confirmed the tear on Monday.",
            text: "Arsenal winger Bukayo Saka has been ruled out for around six weeks after scans confirmed a hamstring tear sustained in Sunday's win over Everton. The club have not put a firm date on his return and said only that they expect him back at some point after the November international break. Manager Mikel Arteta said the squad has cover in the position. Saka had started every league game this season.",
            vetted: vec!["Arsenal".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["hamstring"],
                "key_facts_exclude": ["surgery", "season-ending"]
            }),
        },
        Scenario {
            name: "non-english-accept-and-translate",
            note: "The multilingual half of the contract: 'Detect the source article language. Translate meaning into English before writing the evidence card.' A Spanish article materially about the entity must be ACCEPTED, and the card written in English — dropping it, or echoing Spanish prose into key_facts, both fail.",
            source: "marca",
            title: "El Real Madrid confirma la renovación de Vinicius hasta 2030",
            description: "El extremo brasileño amplía su contrato.",
            text: "El Real Madrid ha confirmado este martes la renovación del extremo brasileño Vinicius Junior hasta el 30 de junio de 2030. El club comunicó que el acuerdo se cerró tras varias semanas de conversaciones y que el jugador pasará por el palco del Santiago Bernabéu esta semana. Vinicius, de 25 años, marcó 24 goles la temporada pasada. El presidente Florentino Pérez calificó la renovación como una prioridad absoluta del club para el proyecto deportivo.",
            vetted: vec!["Real Madrid".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["Vinicius"],
                "blurb_excludes": ["confirma", "renovación"]
            }),
        },
        // ── CONTESTED — a proposed contract sharpening, not the current rule ────────────────
        Scenario {
            name: "boxscore-stub-contested",
            note: "CONTESTED / TARGET, NOT a current-contract failure. 26 of these were accepted and none rejected. But a boxscore genuinely IS about the vetted team, and the prompt's only lever for contentless pages today is `caveats` ('If the article is mostly boilerplate, say so in caveats') — there is no reject-non-reporting rule. This fixture is the ARGUMENT for adding one: the Journalist gets nothing groupable from a score stub. Until the system prompt says so, a red check here is a contract gap, not a model defect. Decide the rule before treating this as a bug.",
            source: "espn",
            title: "West Ham United vs. Southampton - Boxscore - Live Score - October 24, 2026",
            description: "Live score and boxscore.",
            text: "West Ham United 3, Southampton 0. Final. London Stadium, October 24, 2026. Goals: 12' Bowen, 34' Paqueta, 78' Kudus. Shots 14-6. Shots on target 7-2. Possession 58%-42%. Corners 6-3. Fouls 9-12. Yellow cards 1-2. Attendance 59,842. Previous meetings: West Ham 1-1 Southampton, Southampton 0-2 West Ham. Next fixtures listed below. View full match stats, lineups, and commentary.",
            vetted: vec!["West Ham United".into()],
            expect: json!({
                "article_relevant": false
            }),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let prompt = build_article_read_prompt_parts(
                s.source,
                s.title,
                s.description,
                s.text,
                &s.vetted,
                &[],
            );
            // The vetted list travels WITH the fixture: the production derivation only counts
            // vetted entities' roles, so a fixture without it would score a different rule.
            let mut expect = s.expect.clone();
            expect["reader_vetted"] = json!(s.vetted);
            json!({
                "name": s.name,
                "task": "reader",
                "prompt_version": ARTICLE_READ_PROMPT_VERSION,
                "note": s.note,
                "system": ARTICLE_READ_SYSTEM_PROMPT,
                "user_prompt": prompt,
                "temperature": 0.2,
                "expect": expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
