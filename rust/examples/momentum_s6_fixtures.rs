//! Analyst fixtures cover direction, divergence, and thin evidence.
//! Each scenario renders through the REAL production builder (`build_momentum_prompt` +
//! `MOMENTUM_SYSTEM_PROMPT`), so the frozen `system`/`user_prompt` are byte-exact — a
//! prompt bump means "re-run this example", not "hand-patch the JSON". Writes files directly:
//!     cargo run --example momentum_s6_fixtures
//!     cargo run --bin eval -- --task momentum --fixtures   (needs Ollama)

use std::path::Path;

use scoracle_cognition::junctions::analyst::{
    build_momentum_prompt, MOMENTUM_PROMPT_VERSION, MOMENTUM_SYSTEM_PROMPT,
};
use scoracle_cognition::junctions::oracle::{SynthMomentum, SynthRating, SynthVibe};
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

fn rating(notability: i32, label: &str, body: &str) -> Option<SynthRating> {
    Some(SynthRating {
        body: body.to_string(),
        notability,
        rating_trajectory: String::new(), // the prompt renders the label; the enum is unused here
        rating_trajectory_label: label.to_string(),
    })
}

fn vibe(sentiment: i32, prompt: &str) -> Option<SynthVibe> {
    Some(SynthVibe {
        sentiment,
        prompt: prompt.to_string(),
    })
}

fn snapshot(score: f64, r_slope: f64, r_n: i32, v_slope: f64, v_n: i32) -> SynthMomentum {
    // Production's momentum_score IS the average of the two ±100-scale slopes; incoherent
    // fixture data produces a prompt that argues with its own direction line.
    assert!(
        (score - (r_slope + v_slope) / 2.0).abs() < 0.35,
        "incoherent fixture data: momentum_score {score} is not the slope average of \
         ({r_slope}, {v_slope}) — production derives it, so fixtures must too"
    );
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
            rating: rating(83, "overall scores and the top skill trending up over recent games",
                "High-usage creator whose efficiency and rim pressure are climbing; the shot profile keeps improving."),
            vibe: vibe(39, "Efficiency is climbing, but coverage has turned sour after public frustration with the rotation."),
            momentum: snapshot(3.4, 12.5, 6, -5.7, 5),
            expect: json!({}),
        },
        Scenario {
            name: "noisy-flat-signals-steady",
            note: "choppy, low-amplitude signals on both rails. Steady, and the read must not manufacture a trend.",
            entity: "Ethan Cross", entity_type: "player", sport: "NFL",
            rating: rating(79, "recent overall marks are choppy with no clear sustained move",
                "Reliable separator on money downs; recent games alternate strong and quiet without a direction."),
            vibe: vibe(55, "Beat coverage is balanced: one strong practice week, one quiet game, and no larger storyline."),
            momentum: snapshot(0.6, 0.7, 5, 0.5, 4),
            expect: json!({}),
        },
        Scenario {
            name: "rating-surge-vibe-flat",
            note: "genuine statistical surge with calm coverage: rising on the strength of the PEAK rail alone.",
            entity: "Harbor City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating(88, "overall scores and the top skill trending up over recent games",
                "The press is winning the ball higher and more often; underlying numbers back the run of wins."),
            vibe: vibe(63, "Coverage is mostly calm; the tactical press is getting more praise after a run of wins."),
            momentum: snapshot(8.5, 16.1, 7, 0.9, 4),
            expect: json!({}),
        },
        Scenario {
            name: "sparse-samples-stay-steady",
            note: "a big slope on TWO samples is noise, not momentum. Steady, and the read should name the thin sample.",
            entity: "Malik Stone", entity_type: "player", sport: "NBA",
            rating: rating(74, "two recent games show a better overall mark, but the sample is thin",
                "Explosive open-floor finisher; the recent uptick is real but rests on two games."),
            vibe: vibe(52, "Coverage is quiet and mostly waiting for a larger role before drawing conclusions."),
            momentum: snapshot(0.8, 1.6, 2, 0.1, 2),
            expect: json!({}),
        },
        Scenario {
            name: "stats-down-vibe-up-near-zero",
            note: "the inverse split: defense declining while sentiment warms. Near zero, both rails named.",
            entity: "Northbank Rovers", entity_type: "team", sport: "FOOTBALL",
            rating: rating(81, "overall defensive scores and the top skill trending down over recent matches",
                "Season-long elite at limiting chances, but the last stretch shows real defensive slippage."),
            vibe: vibe(68, "Supporter and local coverage is warming after a young forward's breakout week."),
            momentum: snapshot(-0.3, -8.3, 6, 7.7, 5),
            expect: json!({}),
        },
        Scenario {
            name: "vibe-slide-steady-peak",
            note: "sentiment sliding under steady production: modestly negative, PEAK label not clung to.",
            entity: "Nia Torres", entity_type: "player", sport: "NBA",
            rating: rating(86, "overall scores and the top skill steady over recent games",
                "Anchor defender; the production has not moved even as the noise around her has."),
            vibe: vibe(35, "Local coverage has turned negative after late-game benchings and visible frustration."),
            momentum: snapshot(-4.1, 0.4, 6, -8.6, 5),
            expect: json!({}),
        },
        Scenario {
            name: "transfer-noise-sentiment-spike",
            note: "NEW (bakeoff-notes ask): a short, rumor-driven sentiment spike over flat production. Momentum must NOT chase the spike — steady, and the read should attribute the vibe to rumor chatter.",
            entity: "Deni Kovac", entity_type: "player", sport: "FOOTBALL",
            rating: rating(77, "overall scores and the top skill flat over recent matches",
                "Press-resistant carrier whose underlying numbers have not moved in a month."),
            vibe: vibe(75, "A burst of transfer rumor chatter has coverage buzzing, though nothing on the pitch has changed."),
            momentum: snapshot(3.2, 0.1, 6, 6.2, 3),
            expect: json!({}),
        },
        Scenario {
            name: "clean-decline-falling",
            note: "NEW: both rails clearly negative on healthy samples — the set's first unambiguous falling case.",
            entity: "Coastal City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating(72, "overall scores and the top skill trending down over recent matches",
                "The attack has dried up: fewer chances created in each of the last five matches."),
            vibe: vibe(30, "Coverage is grim — a winless month, fan protests, and pressure on the manager."),
            momentum: snapshot(-22.4, -20.6, 9, -24.2, 9),
            expect: json!({}),
        },
        Scenario {
            name: "rising-confirmed",
            note: "s14's first decided-RISING case, adopted into the generator at s15 (it was hand-authored on disk and a regen would have silently dropped it). Both rails up on healthy samples; the READ must voice rising and name both signals in the sport's words.",
            entity: "Deshawn Carter", entity_type: "player", sport: "NBA",
            rating: rating(68, "overall scores and the top skill trending up over recent games",
                "The jumper is falling and the rim pressure is real: efficiency up in each of the last six games with the usage holding."),
            vibe: vibe(76, "The building believes again — a signature road win, the crowd chanting his name, and the beat writers running out of superlatives."),
            momentum: snapshot(28.4, 26.3, 9, 30.5, 10),
            expect: json!({}),
        },
        Scenario {
            name: "falling-confirmed",
            note: "s14's first decided-FALLING case, adopted into the generator at s15 (see rising-confirmed). Both rails down on healthy samples; the READ must voice falling and name both signals in the sport's words.",
            entity: "Riverton Athletic", entity_type: "team", sport: "FOOTBALL",
            rating: rating(61, "overall scores and the top skill trending down over recent matches",
                "The press has collapsed: distances covered and chances created are down in each of the last five matches, and opponents are playing through the midfield at will."),
            vibe: vibe(24, "The mood has curdled — three straight defeats, banners calling for the board, and the away end leaving early."),
            momentum: snapshot(-31.6, -35.0, 9, -28.2, 10),
            expect: json!({}),
        },
    ]
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/momentum");
    std::fs::create_dir_all(&dir)?;
    let scenarios = scenarios();
    let n = scenarios.len();
    for s in scenarios {
        // The Analyst reads only the two rails: no packets, no memory card.
        let prompt = build_momentum_prompt(
            s.entity_type,
            s.entity,
            s.sport,
            s.rating.as_ref(),
            s.vibe.as_ref(),
            &s.momentum,
            None,
        );
        let v = json!({
            "name": s.name,
            "task": "momentum",
            "prompt_version": MOMENTUM_PROMPT_VERSION,
            "note": s.note,
            "system": &*MOMENTUM_SYSTEM_PROMPT,
            "user_prompt": prompt,
            "temperature": 0.0,
            "expect": s.expect});
        let path = dir.join(format!("{}.json", s.name));
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
        println!("wrote {} ({} chars prompt)", path.display(), prompt.len());
    }
    println!("done — {n} fixtures at {MOMENTUM_PROMPT_VERSION}");
    Ok(())
}
