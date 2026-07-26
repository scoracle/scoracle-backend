//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

#[test]
fn parses_momentum_reply() {
    let parsed =
        parse_momentum_reply("SCORE: 3\nREAD: PEAK is rising while Vibe is calm.").unwrap();
    assert_eq!(parsed.score, 3);
    assert_eq!(parsed.blurb, "PEAK is rising while Vibe is calm.");
}

#[test]
fn parse_first_i32_scans_sign_and_digits() {
    assert_eq!(parse_first_i32("SCORE: 3"), Some(3));
    assert_eq!(parse_first_i32("SCORE: -1.0"), Some(-1));
    assert_eq!(parse_first_i32("no digits here"), None);
}

#[test]
fn stray_momentum_line_is_tolerated_and_ignored() {
    // s4 dropped MOMENTUM from the contract; a model echoing the decided direction (in
    // any word, even the old Conflict failure) is skipped, never parsed as content.
    let parsed = parse_momentum_reply(
        "MOMENTUM: Conflict\nSCORE: -1.0\nREAD: Signals split between PEAK and Vibe.",
    )
    .unwrap();
    assert_eq!(parsed.score, -1);
    assert_eq!(parsed.blurb, "Signals split between PEAK and Vibe.");
    // Missing SCORE or empty READ still fail closed.
    assert!(parse_momentum_reply("READ: prose only, no score.").is_none());
    assert!(parse_momentum_reply("SCORE: 2").is_none());
}

#[test]
fn direction_is_decided_by_band_not_model() {
    // ±MOMENTUM_STEADY_BAND on the ±100-scale momentum_score; None (no durable
    // snapshot) is honestly steady.
    assert_eq!(momentum_direction_from_score(Some(10.0)), "rising");
    assert_eq!(momentum_direction_from_score(Some(9.9)), "steady");
    assert_eq!(momentum_direction_from_score(Some(-9.9)), "steady");
    assert_eq!(momentum_direction_from_score(Some(-10.0)), "falling");
    assert_eq!(momentum_direction_from_score(Some(0.0)), "steady");
    assert_eq!(momentum_direction_from_score(None), "steady");
}

#[test]
fn score_clamps_into_the_decided_directions_range() {
    assert_eq!(clamp_score_to_direction("rising", -3), 1);
    assert_eq!(clamp_score_to_direction("rising", 4), 4);
    assert_eq!(clamp_score_to_direction("falling", 2), -1);
    assert_eq!(clamp_score_to_direction("falling", -5), -5);
    assert_eq!(clamp_score_to_direction("steady", 4), 1);
    assert_eq!(clamp_score_to_direction("steady", -4), -1);
    assert_eq!(clamp_score_to_direction("steady", 0), 0);
}

#[test]
fn prompt_carries_the_decided_direction_line() {
    let mom = SynthMomentum {
        rating_slope: Some(50.7),
        rating_samples: 4,
        momentum_score: Some(50.7),
        ..SynthMomentum::default()
    };
    let prompt =
        build_momentum_prompt("player", "Test Player", "FOOTBALL", None, None, &mom, None);
    assert!(prompt.contains(
        "Direction (decided upstream, final): rising (momentum score +50.7, steady band ±10)"
    ));
    // No memory ⇒ no section (s4 byte-shape preserved).
    assert!(!prompt.contains("RELATIONAL MEMORY"));
    // No snapshot → the decided line still exists and is honestly steady.
    let empty = build_momentum_prompt(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &SynthMomentum::default(),
        None,
    );
    assert!(empty.contains(
        "Direction (decided upstream, final): steady (no durable momentum snapshot)"
    ));
}

#[test]
fn relational_memory_renders_before_the_decided_direction() {
    // The s5 memory card sits between the snapshot and the decided-direction line, so
    // the decided fact stays adjacent to the reply cue. Instruction line + bullets.
    let mom = SynthMomentum {
        momentum_score: Some(50.7),
        ..SynthMomentum::default()
    };
    let mem = "Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nCurrent story: Chelsea — tracked since Jun 09, computed likelihood 85/100.";
    let p = build_momentum_prompt(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &mom,
        Some(mem),
    );
    assert!(
        p.contains("=== RELATIONAL MEMORY (computed history) ===\nArc context for the READ")
    );
    assert!(p.contains("- Prior story: Real Madrid — fizzled"));
    let mem_pos = p.find("RELATIONAL MEMORY").unwrap();
    let dir_pos = p.find("Direction (decided upstream, final)").unwrap();
    let cue_pos = p.find("Write the Momentum read now").unwrap();
    assert!(mem_pos < dir_pos && dir_pos < cue_pos);
    // Blank memory ⇒ no section.
    let blank = build_momentum_prompt(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &mom,
        Some(" \n  "),
    );
    assert!(!blank.contains("RELATIONAL MEMORY"));
}

#[test]
fn input_components_are_stable_and_sorted() {
    let rating = SynthRating {
        divined_peak: "Rim protection".to_string(),
        body: "body".to_string(),
        notability: 88,
        peak_trajectory: "rising".to_string(),
        peak_trajectory_label: "Composite rising".to_string(),
    };
    let vibe = SynthVibe {
        sentiment: 62,
        prompt: "Coverage is warmer".to_string(),
    };
    let mom = SynthMomentum {
        rating_slope: Some(1.24),
        rating_samples: 6,
        vibe_slope: Some(-0.04),
        vibe_samples: 4,
        momentum_score: Some(1.19),
        ..SynthMomentum::default()
    };
    // The vibe prompt is non-empty on purpose: the golden proves the felt-read prose is
    // NOT in the hash pre-image (F1 material-only debounce) — only vibe_sentiment is.
    // prompt_version joined at s6 (single-sourced from the const, so a bump can't
    // silently rot this pin); keys stay sorted, so it lands alphabetically.
    assert_eq!(
        build_momentum_input_components(Some(&rating), Some(&vibe), &mom),
        format!(
            r#"{{"divined_peak":"Rim protection","momentum_rating_samples":6,"momentum_rating_slope":1.2,"momentum_score":1.2,"momentum_vibe_samples":4,"momentum_vibe_slope":-0,"notability":88,"peak_trajectory":"rising","peak_trajectory_label":"Composite rising","prompt_version":"{MOMENTUM_PROMPT_VERSION}","vibe_sentiment":62}}"#
        )
    );
}
