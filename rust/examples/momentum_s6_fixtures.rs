//! momentum_s6_fixtures — regenerate the hand-authored Analyst eval fixtures.
//!
//! s15 is the product-name inversion (Scott, 2026-08-10): the READ names signals in the
//! sport's own words — the form/the tape, the mood/emotion around the club — and the desk's
//! labels ("PEAK", "Vibe") are banned from served prose by the case-sensitive
//! `no_product_names` invariant. The old `prose_includes: ["PEAK","Vibe"]` expects flip to
//! `prose_includes_any` synonym groups (FORM_WORDS/MOOD_WORDS below), the baked
//! `peak_trajectory_label` inputs speak the descrubbed z_trajectory_label shape, and the two
//! s14 direction fixtures (rising/falling-confirmed) — which existed only as hand-authored
//! JSON — are adopted as scenarios so a regen can never silently drop them again. This pass
//! also reconciles the D-T51/52 gate growth (prose_no_digits, total_sentences_max, "**")
//! that lived only in the on-disk JSON.
//!
//! s6 was the Characters Phase B voice pass: the system prompt speaks The Analyst's telling
//! (persona-first — detached, directional, comparative, results-only). The s5 CONTRACT —
//! READ shape, decided-direction-is-final, sign agreement, memory discipline — carries: the
//! cases (steady discipline, split rails, thin samples, sentiment spikes, clean decline)
//! are the regression floor and hold regardless of voice.
//!
//! Each scenario holds pillar structs and renders through the REAL production builder
//! (`build_momentum_prompt` + `MOMENTUM_SYSTEM_PROMPT`), so the frozen `system`/`user_prompt`
//! are byte-exact — a prompt bump means "re-run this example", not "hand-patch the JSON".
//! Since s6 this writes the fixture files directly (the transfers/vibe generator pattern):
//!
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

#[allow(clippy::too_many_arguments)]
/// s15 signal-naming groups: "name the signal" now means the sport's own words, which
/// legitimately vary — each is one pipe-delimited any-of check (`prose_includes_any`).
/// PROBE NOTE: these lists were authored against the s15 worked examples ("the form", "the
/// tape", "the mood around", "the feed"); if the gate shows honest READs failing the group,
/// grow the list rather than trusting the red (the D-T50 probe rule — a check can be wrong).
const FORM_WORDS: &str = "form|tape|performance|production";
const MOOD_WORDS: &str = "mood|emotion|feeling|the room|the feed";

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
            momentum: snapshot(0.2, 0.9, 6, -0.8, 5),
            expect: json!({"prose_includes_any": [FORM_WORDS, MOOD_WORDS], "prose_excludes": ["**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "noisy-flat-signals-steady",
            note: "choppy, low-amplitude signals on both rails. Steady, and the read must not manufacture a trend.",
            entity: "Ethan Cross", entity_type: "player", sport: "NFL",
            rating: rating(79, "recent overall marks are choppy with no clear sustained move",
                "Reliable separator on money downs; recent games alternate strong and quiet without a direction."),
            vibe: vibe(55, "Beat coverage is balanced: one strong practice week, one quiet game, and no larger storyline."),
            momentum: snapshot(0.6, 0.2, 5, -0.1, 4),
            expect: json!({"prose_includes": ["steady"], "prose_excludes": ["falling", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "rating-surge-vibe-flat",
            note: "genuine statistical surge with calm coverage: rising on the strength of the PEAK rail alone.",
            entity: "Harbor City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating(88, "overall scores and the top skill trending up over recent games",
                "The press is winning the ball higher and more often; underlying numbers back the run of wins."),
            vibe: vibe(63, "Coverage is mostly calm; the tactical press is getting more praise after a run of wins."),
            momentum: snapshot(3.1, 1.4, 7, 0.2, 4),
            expect: json!({"prose_includes_any": [FORM_WORDS], "prose_excludes": ["falling", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "sparse-samples-stay-steady",
            note: "a big slope on TWO samples is noise, not momentum. Steady, and the read should name the thin sample.",
            entity: "Malik Stone", entity_type: "player", sport: "NBA",
            rating: rating(74, "two recent games show a better overall mark, but the sample is thin",
                "Explosive open-floor finisher; the recent uptick is real but rests on two games."),
            vibe: vibe(52, "Coverage is quiet and mostly waiting for a larger role before drawing conclusions."),
            momentum: snapshot(0.8, 1.6, 2, 0.1, 2),
            expect: json!({"prose_includes": ["sample"], "prose_excludes": ["surging", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "stats-down-vibe-up-near-zero",
            note: "the inverse split: defense declining while sentiment warms. Near zero, both rails named.",
            entity: "Northbank Rovers", entity_type: "team", sport: "FOOTBALL",
            rating: rating(81, "overall defensive scores and the top skill trending down over recent matches",
                "Season-long elite at limiting chances, but the last stretch shows real defensive slippage."),
            vibe: vibe(68, "Supporter and local coverage is warming after a young forward's breakout week."),
            momentum: snapshot(-0.3, -1.1, 6, 0.9, 5),
            expect: json!({"prose_includes_any": [FORM_WORDS, MOOD_WORDS], "prose_excludes": ["**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "vibe-slide-steady-peak",
            note: "sentiment sliding under steady production: modestly negative, PEAK label not clung to.",
            entity: "Nia Torres", entity_type: "player", sport: "NBA",
            rating: rating(86, "overall scores and the top skill steady over recent games",
                "Anchor defender; the production has not moved even as the noise around her has."),
            vibe: vibe(35, "Local coverage has turned negative after late-game benchings and visible frustration."),
            momentum: snapshot(-1.4, 0.0, 6, -1.2, 5),
            expect: json!({"prose_includes_any": [MOOD_WORDS], "prose_excludes": ["**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "transfer-noise-sentiment-spike",
            note: "NEW (bakeoff-notes ask): a short, rumor-driven sentiment spike over flat production. Momentum must NOT chase the spike — steady, and the read should attribute the vibe to rumor chatter.",
            entity: "Deni Kovac", entity_type: "player", sport: "FOOTBALL",
            rating: rating(77, "overall scores and the top skill flat over recent matches",
                "Press-resistant carrier whose underlying numbers have not moved in a month."),
            vibe: vibe(75, "A burst of transfer rumor chatter has coverage buzzing, though nothing on the pitch has changed."),
            momentum: snapshot(0.9, 0.1, 6, 1.8, 3),
            expect: json!({"prose_excludes": ["surging", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "clean-decline-falling",
            note: "NEW: both rails clearly negative on healthy samples — the set's first unambiguous falling case.",
            entity: "Coastal City FC", entity_type: "team", sport: "FOOTBALL",
            rating: rating(72, "overall scores and the top skill trending down over recent matches",
                "The attack has dried up: fewer chances created in each of the last five matches."),
            vibe: vibe(30, "Coverage is grim — a winless month, fan protests, and pressure on the manager."),
            momentum: snapshot(-3.2, -1.5, 6, -1.1, 5),
            expect: json!({"prose_excludes": ["rising", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "rising-confirmed",
            note: "s14's first decided-RISING case, adopted into the generator at s15 (it was hand-authored on disk and a regen would have silently dropped it). Both rails up on healthy samples; the READ must voice rising and name both signals in the sport's words.",
            entity: "Deshawn Carter", entity_type: "player", sport: "NBA",
            rating: rating(68, "overall scores and the top skill trending up over recent games",
                "The jumper is falling and the rim pressure is real: efficiency up in each of the last six games with the usage holding."),
            vibe: vibe(76, "The building believes again — a signature road win, the crowd chanting his name, and the beat writers running out of superlatives."),
            momentum: snapshot(28.4, 2.1, 7, 1.4, 6),
            expect: json!({"prose_includes": ["rising"], "prose_includes_any": [FORM_WORDS, MOOD_WORDS],
                           "prose_excludes": ["falling", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
        Scenario {
            name: "falling-confirmed",
            note: "s14's first decided-FALLING case, adopted into the generator at s15 (see rising-confirmed). Both rails down on healthy samples; the READ must voice falling and name both signals in the sport's words.",
            entity: "Riverton Athletic", entity_type: "team", sport: "FOOTBALL",
            rating: rating(61, "overall scores and the top skill trending down over recent matches",
                "The press has collapsed: distances covered and chances created are down in each of the last five matches, and opponents are playing through the midfield at will."),
            vibe: vibe(24, "The mood has curdled — three straight defeats, banners calling for the board, and the away end leaving early."),
            momentum: snapshot(-31.6, -2.4, 6, -1.7, 5),
            expect: json!({"prose_includes": ["falling"], "prose_includes_any": [FORM_WORDS, MOOD_WORDS],
                           "prose_excludes": ["rising", "**"],
                           "prose_no_digits": true, "total_sentences_max": 10,
                           "prose_min_words": 25, "prose_max_words": 260}),
        },
    ]
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/momentum");
    std::fs::create_dir_all(&dir)?;
    let scenarios = scenarios();
    let n = scenarios.len();
    for s in scenarios {
        let prompt = build_momentum_prompt(
            s.entity_type,
            s.entity,
            s.sport,
            s.rating.as_ref(),
            s.vibe.as_ref(),
            &s.momentum,
            // Fixtures pin the memory-free shape (the s5 eval discipline).
            None,
            &[],
        );
        let v = json!({
            "name": s.name,
            "task": "momentum",
            "prompt_version": MOMENTUM_PROMPT_VERSION,
            "note": s.note,
            "system": MOMENTUM_SYSTEM_PROMPT,
            "user_prompt": prompt,
            "temperature": 0.0,
            "expect": s.expect,
        });
        let path = dir.join(format!("{}.json", s.name));
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
        println!("wrote {} ({} chars prompt)", path.display(), prompt.len());
    }
    println!("done — {n} fixtures at {MOMENTUM_PROMPT_VERSION}");
    Ok(())
}
