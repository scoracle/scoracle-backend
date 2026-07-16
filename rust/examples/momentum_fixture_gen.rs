//! momentum_fixture_gen — regenerate the momentum eval fixtures through the REAL production
//! prompt builder (`momentum::build_momentum_prompt`, prompt version `momentum-s3`).
//!
//! The original six fixtures froze prompts from the eval-only "momentum-eval-v3" fork (no
//! Scouting read line, pre-unification builder). This example holds each scenario as pillar
//! structs so the set regenerates mechanically on any prompt bump:
//!
//!     cargo run --example momentum_fixture_gen > /tmp/momentum_fixtures.json
//!
//! Adds two scenarios the 2026-07-10 bakeoff notes asked for: a transfer-noise sentiment
//! spike (momentum must not chase spiky rumor-driven vibe) and a clean two-rail decline
//! (the set previously had no unambiguous `falling` case).

use scoracle_cognition::momentum::{
    build_momentum_prompt, MOMENTUM_PROMPT_VERSION, MOMENTUM_SYSTEM_PROMPT,
};
use scoracle_cognition::sigil::{SynthMomentum, SynthRating, SynthVibe};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    entity: &'static str,
    entity_type: &'static str,
    sport: &'static str,
    rating: Option<SynthRating>,
    vibe: Option<SynthVibe>,
    momentum: SynthMomentum,
    expect: serde_json::Value,
}

#[allow(clippy::too_many_arguments)]
fn rating(peak: &str, notability: i32, label: &str, body: &str) -> Option<SynthRating> {
    Some(SynthRating {
        divined_peak: peak.to_string(),
        body: body.to_string(),
        notability,
        peak_trajectory: String::new(), // the prompt renders the label; the enum is unused here
        peak_trajectory_label: label.to_string(),
    })
}

fn vibe(sentiment: i32, prompt: &str) -> Option<SynthVibe> {
    Some(SynthVibe {
        sentiment,
        prompt: prompt.to_string(),
    })
}

fn snapshot(score: f64, r_slope: f64, r_n: i32, v_slope: f64, v_n: i32) -> SynthMomentum {
    SynthMomentum {
        momentum_score: Some(score),
        rating_slope: Some(r_slope),
        rating_samples: r_n,
        vibe_slope: Some(v_slope),
        vibe_samples: v_n,
        ..SynthMomentum::default()
    }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "mixed-peak-up-vibe-down",
            note: "split rails: PEAK improving, sentiment souring. Must hold steady near zero and name the conflict in the READ (never on the MOMENTUM line).",
            entity: "Jalen Rowe", entity_type: "player", sport: "NBA",
            rating: rating("Shot creation", 83, "Composite and PEAK z-scores trending up over recent games",
                "High-usage creator whose efficiency and rim pressure are climbing; the shot profile keeps improving."),
            vibe: vibe(39, "Efficiency is climbing, but coverage has turned sour after public frustration with the rotation."),
            momentum: snapshot(0.2, 0.9, 6, -0.8, 5),
            expect: json!({"momentum_direction": "steady", "momentum_score_min": -1, "momentum_score_max": 1,
                           "prose_includes": ["PEAK", "Vibe"], "prose_min_words": 18, "prose_max_words": 80}),
        },
        Scenario {
            name: "noisy-flat-signals-steady",
            note: "choppy, low-amplitude signals on both rails. Steady, and the read must not manufacture a trend.",
            entity: "Ethan Cross", entity_type: "player", sport: "NFL",
            rating: rating("Third-down separation", 79, "Recent rating marks are choppy with no clear sustained move.",
                "Reliable separator on money downs; recent games alternate strong and quiet without a direction."),
            vibe: vibe(55, "Beat coverage is balanced: one strong practice week, one quiet game, and no larger storyline."),
            momentum: snapshot(0.6, 0.2, 5, -0.1, 4),
            expect: json!({"momentum_direction": "steady", "momentum_score_min": -1, "momentum_score_max": 1,
                           "prose_includes": ["steady"], "prose_excludes": ["falling"], "prose_min_words": 15, "prose_max_words": 75}),
        },
        Scenario {
            name: "rating-surge-vibe-flat",
            note: "genuine statistical surge with calm coverage: rising on the strength of the PEAK rail alone.",
            entity: "Harbor City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating("High press recoveries", 88, "Composite and PEAK z-scores trending up over recent games",
                "The press is winning the ball higher and more often; underlying numbers back the run of wins."),
            vibe: vibe(63, "Coverage is mostly calm; the tactical press is getting more praise after a run of wins."),
            momentum: snapshot(3.1, 1.4, 7, 0.2, 4),
            expect: json!({"momentum_direction": "rising", "momentum_score_min": 2,
                           "prose_includes": ["PEAK"], "prose_excludes": ["falling"], "prose_min_words": 15, "prose_max_words": 75}),
        },
        Scenario {
            name: "sparse-samples-stay-steady",
            note: "a big slope on TWO samples is noise, not momentum. Steady, and the read should name the thin sample.",
            entity: "Malik Stone", entity_type: "player", sport: "NBA",
            rating: rating("Transition finishing", 74, "Two recent games show a better composite mark, but the sample is thin.",
                "Explosive open-floor finisher; the recent uptick is real but rests on two games."),
            vibe: vibe(52, "Coverage is quiet and mostly waiting for a larger role before drawing conclusions."),
            momentum: snapshot(0.8, 1.6, 2, 0.1, 2),
            expect: json!({"momentum_direction": "steady", "momentum_score_min": -1, "momentum_score_max": 1,
                           "prose_includes": ["sample"], "prose_excludes": ["surging"], "prose_min_words": 15, "prose_max_words": 80}),
        },
        Scenario {
            name: "stats-down-vibe-up-near-zero",
            note: "the inverse split: defense declining while sentiment warms. Near zero, both rails named.",
            entity: "Northbank Rovers", entity_type: "team", sport: "FOOTBALL",
            rating: rating("Chance suppression", 81, "Defensive composite and PEAK z-scores are trending down over recent matches.",
                "Season-long elite at limiting chances, but the last stretch shows real defensive slippage."),
            vibe: vibe(68, "Supporter and local coverage is warming after a young forward's breakout week."),
            momentum: snapshot(-0.3, -1.1, 6, 0.9, 5),
            expect: json!({"momentum_direction": "steady", "momentum_score_min": -1, "momentum_score_max": 1,
                           "prose_includes": ["PEAK", "Vibe"], "prose_min_words": 18, "prose_max_words": 85}),
        },
        Scenario {
            name: "vibe-slide-steady-peak",
            note: "sentiment sliding under steady production: modestly negative, PEAK label not clung to.",
            entity: "Nia Torres", entity_type: "player", sport: "NBA",
            rating: rating("Rim protection", 86, "Composite and PEAK z-scores steady over recent games.",
                "Anchor defender; the production has not moved even as the noise around her has."),
            vibe: vibe(35, "Local coverage has turned negative after late-game benchings and visible frustration."),
            momentum: snapshot(-1.4, 0.0, 6, -1.2, 5),
            expect: json!({"momentum_direction": "falling", "momentum_score_max": 0,
                           "prose_includes": ["Vibe"], "prose_min_words": 15, "prose_max_words": 80}),
        },
        Scenario {
            name: "transfer-noise-sentiment-spike",
            note: "NEW (bakeoff-notes ask): a short, rumor-driven sentiment spike over flat production. Momentum must NOT chase the spike — steady, and the read should attribute the vibe to rumor chatter.",
            entity: "Deni Kovac", entity_type: "player", sport: "FOOTBALL",
            rating: rating("Progressive carries", 77, "Composite and PEAK z-scores flat over recent matches.",
                "Press-resistant carrier whose underlying numbers have not moved in a month."),
            vibe: vibe(75, "A burst of transfer rumor chatter has coverage buzzing, though nothing on the pitch has changed."),
            momentum: snapshot(0.9, 0.1, 6, 1.8, 3),
            expect: json!({"momentum_direction": "steady", "momentum_score_min": -1, "momentum_score_max": 1,
                           "prose_excludes": ["surging"], "prose_min_words": 15, "prose_max_words": 80}),
        },
        Scenario {
            name: "clean-decline-falling",
            note: "NEW: both rails clearly negative on healthy samples — the set's first unambiguous falling case.",
            entity: "Coastal City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating("Chance creation", 72, "Composite and PEAK z-scores trending down over recent matches.",
                "The attack has dried up: fewer chances created in each of the last five matches."),
            vibe: vibe(30, "Coverage is grim — a winless month, fan protests, and pressure on the manager."),
            momentum: snapshot(-3.2, -1.5, 6, -1.1, 5),
            expect: json!({"momentum_direction": "falling", "momentum_score_max": -2,
                           "prose_excludes": ["rising"], "prose_min_words": 15, "prose_max_words": 80}),
        },
    ]
}

fn main() {
    let out: Vec<serde_json::Value> = scenarios()
        .into_iter()
        .map(|s| {
            let prompt = build_momentum_prompt(
                s.entity_type,
                s.entity,
                s.sport,
                s.rating.as_ref(),
                s.vibe.as_ref(),
                &s.momentum,
            );
            json!({
                "name": s.name,
                "task": "momentum",
                "prompt_version": MOMENTUM_PROMPT_VERSION,
                "note": s.note,
                "system": MOMENTUM_SYSTEM_PROMPT,
                "user_prompt": prompt,
                "temperature": 0.0,
                "expect": s.expect,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
