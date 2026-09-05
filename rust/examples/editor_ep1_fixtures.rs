//! editor_ep1_fixtures — generate the GREENFIELD Editor's eval fixtures through the REAL
//! production prompt builder, so the frozen prompts are byte-true to `EDITOR_CONTRACT_VERSION`
//! (the article_reader/graph/sigil fixture-gen pattern).
//!
//! The set is the legacy seven ported to ep1 expectations plus the five cases Phase 3.6 names:
//! coach-discovery (kyle-shanahan shape), place-collision (Paris/Moulin Rouge — the descriptor
//! prevents the club link), hallucinated-parent (Fortuna Düsseldorf — exact-match refusal),
//! a result-line case (verbatim score), and a namesake tie (Vinicius — refused_ambiguous).
//! The opponent-only case keeps its KEPT-since-ar7 expectation (`relevant=true` — the stale
//! fixture trap). Both directions stay pinned: a set that only pinned rejections would be
//! passed by a model that answers "no" to everything.
//!
//!     cargo run --example editor_ep1_fixtures > /tmp/editor_fixtures.json
//!
//! Output: a JSON array of fixture objects; split into `fixtures/editor/<name>.json`.
//! Offline — no DB, no model, no queue, no fetch.

use scoracle_cognition::junctions::editor::{
    build_editor_prompt_parts, EDITOR_CONTRACT_VERSION, EDITOR_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    source: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
    hypothesis: Vec<String>,
    expect: serde_json::Value,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // ── REJECT ──────────────────────────────────────────────────────────────────────────
        Scenario {
            name: "name-collision-youth-flag-football",
            note: "THE measured false positive (2026-07-26), ported to ep1: a youth flag-football broadcast listing accepted as evidence about the NFL's Baltimore Ravens, blurb inventing a 'New York City Ravens'. Rejected on both counts (listing shape, absent entity); key_facts_exclude pins the invention itself.",
            source: "nfl.com",
            title: "Watch En Español-New York City, NY (Ravens) vs. Jacksonville, FL (Jaguars)",
            description: "NFL Youth Flag Football Championship Tournament",
            text: "Watch the NFL Youth Flag Football Championship Tournament live in Spanish. This 45-minute broadcast features the team from New York City, NY facing the team from Jacksonville, FL in the tournament bracket. The NFL Youth Flag program celebrates the best youth flag football teams from across the country. Broadcast begins at 4:00 PM ET. Available on NFL+ and in the NFL app. Check your local listings for replay times. Related: watch more youth flag football, browse the full tournament schedule, subscribe to NFL+.",
            hypothesis: vec!["Baltimore Ravens".into()],
            expect: json!({
                "article_relevant": false,
                "key_facts_exclude": ["New York City Ravens"]
            }),
        },
        Scenario {
            name: "boxscore-stub-contested",
            note: "Under ep1 this is NOT contested: page_kind score_table is a first-class rejection ground (non-reporting shape), carried from ar6. 26 of these were once accepted with zero rejections. The result_line check rides along: a boxscore DOES state a completed result, and the verbatim line must survive even on a rejected page — Phase 4's box-score fork feeds on it.",
            source: "espn",
            title: "West Ham United vs. Southampton - Boxscore - Live Score - October 24, 2026",
            description: "Live score and boxscore.",
            text: "West Ham United 3-0 Southampton. Final. London Stadium, October 24, 2026. Goals: 12' Bowen, 34' Paqueta, 78' Kudus. Shots 14-6. Shots on target 7-2. Possession 58%-42%. Corners 6-3. Fouls 9-12. Yellow cards 1-2. Attendance 59,842. Previous meetings: West Ham 1-1 Southampton, Southampton 0-2 West Ham. Next fixtures listed below. View full match stats, lineups, and commentary.",
            hypothesis: vec!["West Ham United".into()],
            expect: json!({
                "article_relevant": false
            }),
        },
        Scenario {
            name: "place-collision-paris-moulin-rouge",
            note: "The measured B4 failure, now with the ep1 descriptor as the fix: 'Paris' emitted on a Tour de France city page must NOT become the club link. The surface EXISTS (team 4508 'Paris'), so only the kind gate — fed by the model's own kind_hint — stands between the city and a wrong link. Best outcome: Paris not emitted at all (it is not a club or person); acceptable: emitted with a non-club kind. Either way resolver_never_links must hold, and the page (no hypothesis club in it) is irrelevant.",
            source: "reuters",
            title: "Moulin Rouge dancers welcome Tour de France finale in Paris",
            description: "The race returns to the capital.",
            text: "Dancers from the Moulin Rouge cabaret performed on the Champs-Elysees on Sunday as the Tour de France returned to Paris for its traditional finale, the city hosting the finish for the first time since roadworks diverted the 2024 edition. Crowds lined the avenue from early morning. Organisers said more than half a million spectators watched the sprint finish, won by Dutch rider Jasper Groen. The mayor's office said the event passed without incident, and confirmed talks to keep the finale in the capital through 2030.",
            hypothesis: vec!["Paris".into()],
            expect: json!({
                "article_relevant": false,
                "resolver_surfaces": [
                    {"name": "Paris", "entity_type": "team", "entity_id": 4508}
                ],
                "resolver_links_exclude": ["Paris"]
            }),
        },
        Scenario {
            name: "hallucinated-parent-fortuna",
            note: "The B4 mining-stock case: a page about 'Fortuna Mining Corp' once produced the INVENTED 'Fortuna Düsseldorf' with a blurb asserting the subsidiary relationship. ep1's names[] rule ('only names the text actually contains; never expand a name into a club it resembles') plus exact-match refusal: 'Fortuna Mining Corp' must not resolve to team 1092 'Fortuna', and the German club must not be emitted at all.",
            source: "globenewswire",
            title: "Fortuna announces drilling results from the Séguéla mine",
            description: "Q3 drilling program update.",
            text: "Fortuna Mining Corp announced results from its ongoing drilling program at the Séguéla mine in Côte d'Ivoire, with intercepts including 8.5 g/t gold over 12 metres at the Sunbird deposit. Chief executive Jorge Ganoza said the results support a mine-life extension study planned for next year. The company also reported that processing throughput reached a record 4,100 tonnes per day in the quarter. Shares rose 3.2% in Toronto trading. Fortuna operates five mines across West Africa and Latin America.",
            hypothesis: vec!["Fortuna".into()],
            expect: json!({
                "article_relevant": false,
                "names_exclude": ["Fortuna Düsseldorf"],
                "resolver_surfaces": [
                    {"name": "Fortuna Düsseldorf", "entity_type": "team", "entity_id": 1092},
                    {"name": "Fortuna", "entity_type": "team", "entity_id": 1092}
                ],
                "resolver_links_exclude": ["Fortuna Düsseldorf"]
            }),
        },
        // ── ACCEPT ──────────────────────────────────────────────────────────────────────────
        Scenario {
            name: "opponent-only-mention",
            note: "KEPT since ar7, deliberately — the stale fixture trap: an opponent-only story used to be rejected, and that narrower rule is what collapsed the legacy Editor's yield 73% -> 2%. A match against us is news about us. The discovery axis rides along: the squad-news names must surface with person kinds.",
            source: "skysports",
            title: "Why Vanja Dragojevic is absent from the Rangers squad against West Ham United",
            description: "The defender was left out for Thursday's friendly.",
            text: "Rangers defender Vanja Dragojevic was left out of the squad for Thursday's pre-season friendly, and manager Philippe Clement confirmed afterwards that the 22-year-old is managing a minor calf complaint picked up in training on Tuesday. Clement said the decision was purely precautionary and that Dragojevic is expected to return for the opening league fixture. The defender has made 14 appearances since arriving last summer and had been expected to start at the back. Rangers named a young side for the fixture, with several first-team regulars rested.",
            hypothesis: vec!["West Ham United".into()],
            expect: json!({
                "article_relevant": true,
                "names_include": ["Dragojevic", "Clement", "Rangers"],
                "name_kind_is": {"Dragojevic": "person", "Rangers": "club"},
                "register_is": "neutral",
                "story_type_is": "injury"
            }),
        },
        Scenario {
            name: "transfer-report-accept-and-ground",
            note: "The recall half of the gate: an unambiguous transfer story about the hypothesis entity must be ACCEPTED and compressed with its facts intact. Without cases like this a model that rejects everything would score 100%.",
            source: "bbc",
            title: "Aston Villa agree £34m deal for Emiliano Buendia",
            description: "The midfielder is set for a medical on Friday.",
            text: "Aston Villa have agreed a £34m fee with Norwich City for Emiliano Buendia, with the Argentina midfielder due to undergo a medical on Friday ahead of a five-year contract. Villa head coach Unai Emery pushed for the signing after missing out on a similar target in January. Norwich will retain a 15% sell-on clause. Buendia scored 15 goals and registered 16 assists last season. The deal is expected to be confirmed early next week, subject to international clearance.",
            hypothesis: vec!["Aston Villa".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["Buendia"],
                "names_include": ["Buendia", "Norwich"],
                "name_kind_is": {"Buendia": "person"},
                "blurb_includes": ["Villa"],
                "blurb_excludes": ["not materially about"],
                "result_line_parses": false,
                "story_type_is": "transfer"
            }),
        },
        Scenario {
            name: "injury-report-accept-no-invention",
            note: "Accept + no-invention. The article gives a timeline and a cause; the return date is explicitly unconfirmed, so the card must not harden it into a fact. 'Do not invent context, implications, or sourcing.'",
            source: "skysports",
            title: "Saka ruled out for six weeks with hamstring injury",
            description: "Scans confirmed the tear on Monday.",
            text: "Arsenal winger Bukayo Saka has been ruled out for around six weeks after scans confirmed a hamstring tear sustained in Sunday's win over Everton. The club have not put a firm date on his return and said only that they expect him back at some point after the November international break. Manager Mikel Arteta said the squad has cover in the position. Saka had started every league game this season.",
            hypothesis: vec!["Arsenal".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["hamstring"],
                "key_facts_exclude": ["surgery", "season-ending"],
                "names_include": ["Saka", "Arteta"],
                "register_is": "neutral",
                "story_type_is": "injury"
            }),
        },
        Scenario {
            name: "non-english-accept-and-translate",
            note: "The multilingual half of the contract: a Spanish article materially about the entity must be ACCEPTED and the card written in English — dropping it, or echoing Spanish prose into key_facts, both fail.",
            source: "marca",
            title: "El Real Madrid confirma la renovación de Vinicius hasta 2030",
            description: "El extremo brasileño amplía su contrato.",
            text: "El Real Madrid ha confirmado este martes la renovación del extremo brasileño Vinicius Junior hasta el 30 de junio de 2030. El club comunicó que el acuerdo se cerró tras varias semanas de conversaciones y que el jugador pasará por el palco del Santiago Bernabéu esta semana. Vinicius, de 25 años, marcó 24 goles la temporada pasada. El presidente Florentino Pérez calificó la renovación como una prioridad absoluta del club para el proyecto deportivo.",
            hypothesis: vec!["Real Madrid".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["Vinicius"],
                // Accented Spanish-only forms: bare "confirma" is a substring of the English
                // "confirmation", which a correct English blurb may legitimately use (measured
                // false red, 2026-08-01 iter11).
                "blurb_excludes": ["confirmó", "renovación"],
                "story_type_is": "contract"
            }),
        },
        Scenario {
            name: "fan-protest-register-outrage",
            note: "C2's only NON-NEUTRAL case, and the set is a decoration without it: neutral-only fixtures can be passed by a model that answers `neutral` to everything — ar3's 99.1% failure wearing a different field name. The text carries an explicit quoted emotion so the register is not a judgment call. Grounding of the phrase is deliberately not pinned; the parser forces an empty phrase to neutral, so a passing register check proves a phrase materialized.",
            source: "Liverpool Echo",
            title: "Everton fans stage second Goodison sit-in after 4-0 humiliation",
            description: "Everton fans stage second Goodison sit-in after 4-0 humiliation Liverpool Echo",
            text: "Everton supporters staged a sit-in at Goodison Park for a second week running after Saturday's 4-0 defeat by Brentford, with several thousand fans remaining in the Gwladys Street end for twenty minutes after the final whistle. A banner reading \"SIXTEEN YEARS OF FAILURE\" was unfurled in the lower tier. Supporters' trust chair Denise Barrett-Baxendale said the mood had hardened: \"People are furious, and they have every right to be. This board has treated a great club with contempt.\" Manager David Moyes said he understood the anger and would not criticise the protest. Everton have taken four points from their opening seven fixtures.",
            hypothesis: vec!["Everton".into()],
            expect: json!({
                "article_relevant": true,
                "key_facts_include": ["sixteen years"],
                "names_include": ["Everton", "Moyes"],
                "names_exclude": ["Goodison", "Gwladys"],
                "register_is": "outrage"
            }),
        },
        Scenario {
            name: "coach-discovery-kyle-shanahan",
            note: "The discovery case the whole names[] design serves (kyle-shanahan shape): a coach is not in the players table, fuzzy-matches a real player, and mig 198 deliberately refused the alias. ep1 must emit him as person WITH a descriptor from the text — the 5.2 first-sight nomination trigger — and the resolver must land him unresolved (the Investigator's channel), never a link. A HIRING, so story_type pins `roster` (§1a: hirings are roster).",
            source: "nfl.com",
            title: "49ers appoint Kyle Shanahan as new head coach",
            description: "San Francisco name their next head coach.",
            text: "The San Francisco 49ers have appointed Kyle Shanahan as their new head coach on a six-year deal, the team announced on Wednesday. Shanahan arrives after two seasons as offensive coordinator in Atlanta and becomes the youngest head coach in the league. General manager John Lynch, hired earlier in the week, called the appointment the cornerstone of the rebuild. Shanahan said the roster needs work but the foundations are in place. Financial terms were not disclosed, though league sources described the deal as placing Shanahan among the better-paid coaches in the NFL.",
            hypothesis: vec!["San Francisco 49ers".into()],
            expect: json!({
                "article_relevant": true,
                "names_include": ["Kyle Shanahan"],
                "name_kind_is": {"Kyle Shanahan": "person"},
                "name_descriptor_nonempty": ["Kyle Shanahan"],
                "resolver_surfaces": [
                    {"name": "San Francisco 49ers", "entity_type": "team", "entity_id": 14}
                ],
                "resolver_unresolved_include": ["Kyle Shanahan"],
                "story_type_is": "roster"
            }),
        },
        Scenario {
            name: "result-line-verbatim-score",
            note: "The verbatim-or-empty shape: a completed result stated in the text must come back as the copyable line, and the PRODUCTION parse_result_line must consume it — Phase 4's box-score fork feeds on exactly this. The model never computes; it copies.",
            source: "bbc",
            title: "Real Madrid 2-1 Arsenal: late Bellingham goal settles semi-final first leg",
            description: "Bellingham strikes in the 89th minute.",
            text: "Real Madrid 2-1 Arsenal. Jude Bellingham struck in the 89th minute to give Real Madrid a 2-1 win over Arsenal in the first leg of their Champions League semi-final at the Bernabeu. Bukayo Saka had put the visitors ahead early in the second half, finishing a sweeping counter-attack, before Vinicius Junior levelled from close range on 71 minutes. Arsenal manager Mikel Arteta said his side remain confident for the return leg at the Emirates next Wednesday. Real Madrid have now won nine consecutive home games in the competition.",
            hypothesis: vec!["Real Madrid".into()],
            expect: json!({
                "article_relevant": true,
                "result_line_includes": ["2-1"],
                "result_line_parses": true,
                "names_include": ["Bellingham"],
                "story_type_is": "performance"
            }),
        },
        Scenario {
            name: "namesake-tie-vinicius",
            note: "The T9 tie: the article writes a bare 'Vinicius', and TWO same-sport players carry that exact surface. Exact match fires twice, and the resolver must REFUSE — never a coin flip; a refused tie always nominates (5.2). The fixture declares both surfaces so the production grouping is what gets scored.",
            source: "marca",
            title: "Vinicius rescues point for Real Madrid at Sevilla",
            description: "The forward equalises in stoppage time.",
            text: "Vinicius equalised deep into stoppage time as Real Madrid salvaged a 1-1 draw at Sevilla on Saturday night. The forward turned home a cross from the left after Sevilla had defended their lead for over an hour. Real Madrid remain two points clear at the top of the table. Coach Carlo Ancelotti said the performance was not good enough but praised the response after the break. Sevilla remain twelfth after a fifth draw of the season.",
            hypothesis: vec!["Real Madrid".into()],
            expect: json!({
                "article_relevant": true,
                "names_include": ["Vinicius"],
                "resolver_surfaces": [
                    {"name": "Vinicius", "entity_type": "player", "entity_id": 600687},
                    {"name": "Vinicius", "entity_type": "player", "entity_id": 831044}
                ],
                "resolver_refused_include": ["Vinicius"],
                "story_type_is": "performance"
            }),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let prompt =
                build_editor_prompt_parts(s.source, s.title, s.description, s.text, &s.hypothesis);
            // The hypothesis list travels WITH the fixture (as reader_vetted, the field the
            // evaluator already hands to the parser): only hypothesis entities' roles count
            // toward the derived verdict.
            let mut expect = s.expect.clone();
            expect["reader_vetted"] = json!(s.hypothesis);
            json!({
                "name": s.name,
                "task": "editor",
                "prompt_version": EDITOR_CONTRACT_VERSION,
                "note": s.note,
                "system": EDITOR_SYSTEM_PROMPT,
                "user_prompt": prompt,
                // 0.0, unlike the legacy set's inherited 0.2: greedy decoding makes the gate
                // REPRODUCIBLE (the Fixture doc's stated design). Production runs 0.2; the
                // gate pins the contract, not the sampler.
                "temperature": 0.0,
                "expect": expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
