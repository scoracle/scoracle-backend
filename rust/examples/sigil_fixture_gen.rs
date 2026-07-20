//! sigil_fixture_gen — regenerate the sigil eval fixtures through the REAL production prompt
//! builder, so the frozen fixture prompts are byte-true to the current `SIGIL_PROMPT_VERSION`.
//!
//! The original three fixtures were hand-authored against s11 and drifted from the production
//! renderer (invented stage names, non-production trajectory labels, and — after s13 — no
//! PILLAR AGREEMENT card at all). This example holds each scenario as pillar STRUCTS and calls
//! `build_synthesis_prompt` + `SIGIL_SYSTEM_PROMPT`, so a future prompt bump regenerates the
//! set mechanically:
//!
//!     cargo run --example sigil_fixture_gen > /tmp/sigil_fixtures.json
//!
//! Output: a JSON array of fixture objects; split into `fixtures/sigil/<name>.json` by hand or
//! script. Offline — no DB, no model, no queue.

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::sigil::{
    build_synthesis_prompt, PrevSigil, SynthMomentum, SynthNarrative, SynthRating, SynthVibe,
    SIGIL_PROMPT_VERSION, SIGIL_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    entity: &'static str,
    narratives: Vec<SynthNarrative>,
    rating: Option<SynthRating>,
    vibe: Option<SynthVibe>,
    momentum: SynthMomentum,
    transfers: Vec<HeatItem>,
    previous: Option<PrevSigil>,
    expect: serde_json::Value,
}

fn narrative(
    title: &str,
    body: &str,
    impact: f64,
    trajectory: &str,
    source_count: i32,
    source_age_days: Option<i32>,
) -> SynthNarrative {
    SynthNarrative {
        title: title.to_string(),
        body: body.to_string(),
        impact,
        trajectory: trajectory.to_string(),
        source_count,
        source_age_days,
    }
}

fn rating(
    divined_peak: &str,
    notability: i32,
    trajectory: &str,
    trajectory_label: &str,
    body: &str,
) -> SynthRating {
    SynthRating {
        divined_peak: divined_peak.to_string(),
        body: body.to_string(),
        notability,
        peak_trajectory: trajectory.to_string(),
        peak_trajectory_label: trajectory_label.to_string(),
    }
}

fn momentum(direction: &str, score: f64, vibe_slope: f64, vibe_samples: i32) -> SynthMomentum {
    SynthMomentum {
        direction: Some(direction.to_string()),
        momentum_score: Some(score),
        vibe_slope: Some(vibe_slope),
        vibe_samples,
        ..SynthMomentum::default()
    }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "aligned-convergent",
            note: "aligned: every lens agrees, the PILLAR AGREEMENT card is all-agree. FLOOR: convergence_min 70. disagreement_absent passes only via the N/A normalization.",
            entity: "Aria Cole",
            narratives: vec![narrative(
                "Breakout stretch drawing All-Star buzz",
                "Coverage is uniformly positive: efficient scoring, improved playmaking, team winning.",
                6.0, "heating_up", 3, Some(1),
            )],
            rating: Some(rating(
                "Playmaking", 72, "rising",
                "Composite and PEAK z-scores trending up over recent games",
                "Ascending lead guard. Career-high efficiency and assist rate; strong impact metrics.",
            )),
            vibe: Some(SynthVibe { sentiment: 78, prompt: "Clearly on the rise.".to_string() }),
            momentum: momentum("rising", 2.0, 0.6, 5),
            transfers: vec![],
            previous: None,
            expect: json!({ "convergence_min": 70, "disagreement_nonempty": false }),
        },
        Scenario {
            name: "hot-but-weakly-sourced-transfer",
            note: "single-source rumor vs steady fundamentals; every trajectory is neutral so NO agreement card renders — the model must read the weak evidence itself. TARGET: convergence_max 60 + no parroted 'sliding momentum'.",
            entity: "Diego Ferran",
            narratives: vec![narrative(
                "Trade-deadline speculation swirls",
                "A single aggregator floated a blockbuster; established insiders have not corroborated it.",
                5.0, "developing_story", 1, Some(0),
            )],
            rating: Some(rating(
                "Spot-up shooting", 45, "steady", "",
                "Solid rotation forward. Average-to-good efficiency; role player value.",
            )),
            vibe: Some(SynthVibe { sentiment: 55, prompt: "Mixed, speculation-driven chatter.".to_string() }),
            momentum: momentum("steady", 0.0, 0.1, 4),
            transfers: vec![HeatItem {
                counterparty: "Warriors".to_string(),
                heat: 71,
                stage: "speculation".to_string(),
                direction: "incoming".to_string(),
                summary: "An aggregator floats a Warriors blockbuster; no established insider has corroborated it".to_string(),
                confidence: Some(0.3),
            }],
            previous: None,
            expect: json!({
                "convergence_max": 60,
                "disagreement_excludes": ["sliding momentum"],
                "why_now_nonempty": true
            }),
        },
        Scenario {
            name: "hot-stats-cold-narrative",
            note: "the classic level-vs-direction conflict: elite PEAK strength, falling momentum, negative vibe. The card hands the model both PEAK-strength DISAGREE pairs. TARGET: convergence_max 55.",
            entity: "Marcus Vale",
            narratives: vec![narrative(
                "Locker-room friction and trade-demand reports",
                "Multiple beat writers describe a frustrated star at odds with the coaching staff; sources say he has privately asked about his future.",
                8.0, "heating_up", 4, Some(1),
            )],
            rating: Some(rating(
                "Two-way wing play", 90, "steady", "",
                "Elite two-way wing. Top-5 percentile scoring efficiency and on-ball defense this season; carrying a heavy usage load with the team's best on/off splits.",
            )),
            vibe: Some(SynthVibe { sentiment: 34, prompt: "Sour mood around the player despite the production.".to_string() }),
            momentum: momentum("falling", -2.0, -0.7, 5),
            transfers: vec![],
            previous: None,
            expect: json!({
                "convergence_max": 55,
                "disagreement_nonempty": true,
                "disagreement_includes": ["PEAK"],
                "why_now_nonempty": true
            }),
        },
        Scenario {
            name: "freefall-all-negative",
            note: "convergence measures AGREEMENT, not positivity: every rail is negative and AGREES. The score must be low but convergence HIGH, with no disagreement. Guards the 'low score = low convergence' confusion.",
            entity: "Ty Marsh",
            narratives: vec![narrative(
                "Benched after another quiet outing",
                "Coverage is consistently negative: shrinking role, poor finishing, and a coach unwilling to commit minutes.",
                6.0, "heating_up", 3, Some(1),
            )],
            rating: Some(rating(
                "No standout skill", 20, "falling",
                "Composite and PEAK z-scores trending down over recent games",
                "Fringe rotation forward. Below-average efficiency and declining usage; no strong or elite skill this season.",
            )),
            vibe: Some(SynthVibe { sentiment: 25, prompt: "Gloomy — fans and beat writers see a shrinking role.".to_string() }),
            momentum: momentum("falling", -3.0, -0.9, 6),
            transfers: vec![],
            previous: None,
            expect: json!({
                "score_max": 40,
                "convergence_min": 70,
                "disagreement_nonempty": false
            }),
        },
        Scenario {
            name: "hype-without-production",
            note: "the inverse conflict: weak PEAK strength (notability 25) under positive vibe and rising momentum — hype the stats do not confirm. Both PEAK-strength pairs DISAGREE; convergence must come down and the disagreement should name the stats-vs-hype gap.",
            entity: "Rio Vance",
            narratives: vec![narrative(
                "Viral highlight run has the fanbase dreaming",
                "Two spectacular plays drove a wave of highlight coverage and breakout talk, though beat writers note the underlying role has not changed.",
                7.0, "heating_up", 3, Some(0),
            )],
            rating: Some(rating(
                "No standout skill", 25, "steady", "",
                "Deep-bench wing. Limited minutes with below-average efficiency; no strong or elite skill in the profile.",
            )),
            vibe: Some(SynthVibe { sentiment: 74, prompt: "Buzzing — highlight-reel plays have fans excited.".to_string() }),
            momentum: momentum("rising", 2.0, 0.8, 5),
            transfers: vec![],
            previous: None,
            expect: json!({
                "convergence_max": 55,
                "disagreement_nonempty": true,
                "disagreement_includes": ["PEAK"]
            }),
        },
        Scenario {
            name: "previous-sigil-continuity",
            note: "the memory anchor: a strong prior (82) with mildly positive fresh signals must move DELIBERATELY, not reset — score stays in the prior's neighborhood. Guards prior-blindness.",
            entity: "Aria Cole",
            narratives: vec![narrative(
                "Steady stretch keeps the good vibes rolling",
                "Quietly excellent play continues; coverage is positive but unremarkable.",
                4.0, "developing_story", 2, Some(2),
            )],
            rating: Some(rating(
                "Playmaking", 74, "steady", "",
                "Established lead guard. Efficiency and assist rate holding at career-best levels.",
            )),
            vibe: Some(SynthVibe { sentiment: 68, prompt: "Warm and settled.".to_string() }),
            momentum: momentum("steady", 0.5, 0.2, 5),
            transfers: vec![],
            previous: Some(PrevSigil { score: 82, blurb: "A rising lead guard putting together an All-Star-calibre season, with the numbers and the narrative pulling the same direction.".to_string() }),
            expect: json!({ "score_min": 68, "score_max": 92 }),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let prompt = build_synthesis_prompt(
                "player",
                s.entity,
                "NBA",
                &s.narratives,
                s.rating.as_ref(),
                s.vibe.as_ref(),
                &s.momentum,
                &s.transfers,
                s.previous.as_ref(),
                // Fixtures pin the memory-free shape (the s15 eval/parity discipline).
                None,
            );
            json!({
                "name": s.name,
                "task": "sigil",
                "prompt_version": SIGIL_PROMPT_VERSION,
                "note": s.note,
                "system": SIGIL_SYSTEM_PROMPT,
                "user_prompt": prompt,
                "temperature": 0.0,
                "expect": s.expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
