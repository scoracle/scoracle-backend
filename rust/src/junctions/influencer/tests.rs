//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

#[test]
fn parses_two_line_reply() {
    // v12-shape compat: a hook-less reply still parses; hook is None.
    let (score, hook, vibe) =
        parse_vibe_reply("SCORE: 73\nVIBE: Quietly surging into the playoff race.").unwrap();
    assert_eq!(score, 73);
    assert_eq!(hook, None);
    assert_eq!(vibe, "Quietly surging into the playoff race.");
}

#[test]
fn parses_three_line_reply_with_hook() {
    let (score, hook, vibe) = parse_vibe_reply(
        "SCORE: 73\nHOOK: The building believes again\nVIBE: Quietly surging into the playoff race.",
    )
    .unwrap();
    assert_eq!(score, 73);
    assert_eq!(hook.as_deref(), Some("The building believes again"));
    assert_eq!(vibe, "Quietly surging into the playoff race.");
}

#[test]
fn drifted_hook_after_vibe_is_not_swallowed() {
    // Format drift: a HOOK line AFTER the VIBE line still parses as the hook and never
    // joins into the felt-read prose.
    let (score, hook, vibe) =
        parse_vibe_reply("SCORE: 60\nVIBE: line one\nHOOK: The title\nline two").unwrap();
    assert_eq!(score, 60);
    assert_eq!(hook.as_deref(), Some("The title"));
    assert_eq!(vibe, "line one line two");
}

#[test]
fn compat_view_drops_hook() {
    let (score, vibe) =
        parse_sentiment_and_prompt("SCORE: 73\nHOOK: The title\nVIBE: The read.").unwrap();
    assert_eq!(score, 73);
    assert_eq!(vibe, "The read.");
}

#[test]
fn clamps_and_joins_trailing_vibe_lines() {
    let (score, vibe) =
        parse_sentiment_and_prompt("SCORE: 250\nVIBE: line one\nline two").unwrap();
    assert_eq!(score, 100);
    assert_eq!(vibe, "line one line two");
}

#[test]
fn falls_back_to_first_integer() {
    let (score, vibe) = parse_sentiment_and_prompt("the vibe is about 64 today").unwrap();
    assert_eq!(score, 64);
    assert_eq!(vibe, "");
}

#[test]
fn errors_without_digits() {
    assert!(parse_sentiment_and_prompt("no number here").is_err());
}

#[test]
fn builds_prompt_with_empty_sections() {
    // No previous, no memory ⇒ neither section renders (v11 byte-shape preserved).
    let p = build_sentiment_prompt("player", "Test Player", "NBA", &[], &[], None, None);
    assert_eq!(
        p,
        "Entity: Player Test Player (NBA)\n\nNarratives forming around them (ordered by relevance/topic heat; impact in brackets):\n- (none this cycle)\n\nCurrent transfer/trade activity (heat 0-100):\n- (none)\n\nRespond now (SCORE line, then HOOK line, then VIBE line)."
    );
}

#[test]
fn previous_vibe_renders_as_continuity_lead_in() {
    // A prior read renders a `=== PREVIOUS VIBE ===` block right after the Entity line,
    // BEFORE the fresh narratives — the model reads its prior before the new evidence
    // (the sigil Phase-5.2 placement).
    let previous = PrevVibe {
        sentiment: 68,
        vibe_prompt: "Quietly surging into the playoff race.".into(),
    };
    let p = build_sentiment_prompt(
        "player",
        "Test Player",
        "NBA",
        &[],
        &[],
        Some(&previous),
        None,
    );
    assert!(p.starts_with(
        "Entity: Player Test Player (NBA)\n\n=== PREVIOUS VIBE ===\nScore: 68/100\nQuietly surging into the playoff race.\n\nNarratives forming"
    ));
}

#[test]
fn previous_vibe_empty_read_renders_score_only() {
    let previous = PrevVibe {
        sentiment: 55,
        vibe_prompt: String::new(),
    };
    let p = build_sentiment_prompt("team", "Test Team", "NFL", &[], &[], Some(&previous), None);
    assert!(p.contains("=== PREVIOUS VIBE ===\nScore: 55/100\n\nNarratives forming"));
}

#[test]
fn memory_card_renders_between_heat_and_reply_cue() {
    let mem = "Prior story: Real Madrid — fizzled (Jun 2026, peak coverage 82/100).\nGround truth: completed a confirmed move to Arsenal on Jul 01 2026.";
    let p = build_sentiment_prompt(
        "player",
        "Test Player",
        "FOOTBALL",
        &[],
        &[],
        None,
        Some(mem),
    );
    assert!(p.contains("\nRelational memory (computed history"));
    assert!(p.contains("- Prior story: Real Madrid — fizzled"));
    assert!(p.contains("- Ground truth: completed"));
    let heat_pos = p.find("Current transfer/trade activity").unwrap();
    let mem_pos = p.find("Relational memory").unwrap();
    let cue_pos = p.find("Respond now").unwrap();
    assert!(heat_pos < mem_pos && mem_pos < cue_pos);
}

#[test]
fn blank_memory_renders_no_section() {
    let p = build_sentiment_prompt(
        "player",
        "Test Player",
        "NBA",
        &[],
        &[],
        None,
        Some("  \n "),
    );
    assert!(!p.contains("Relational memory"));
}

#[test]
fn dedupes_preserving_order() {
    assert_eq!(dedupe_i64(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
}

#[test]
fn vibe_parser_wraps_valid_reply_as_some() {
    let reply = VibeParser
        .parse("SCORE: 73\nHOOK: The building believes again\nVIBE: Quietly surging into the playoff race.")
        .unwrap()
        .expect("a valid reply is Some, never the fail-closed None");
    assert_eq!(reply.sentiment, 73);
    assert_eq!(reply.hook.as_deref(), Some("The building believes again"));
    assert_eq!(reply.vibe_prompt, "Quietly surging into the playoff race.");
}

#[test]
fn vibe_parser_accepts_hookless_v12_shape() {
    let reply = VibeParser
        .parse("SCORE: 73\nVIBE: Quietly surging into the playoff race.")
        .unwrap()
        .expect("a hook-less reply still parses");
    assert_eq!(reply.sentiment, 73);
    assert_eq!(reply.hook, None);
}

#[test]
fn vibe_parser_errors_without_digits() {
    // No digit anywhere ⇒ Err (retry/back-off), NOT Ok(None): vibe's only fail-closed
    // path is the pre-model no-corpus marker, never an unparseable reply.
    assert!(VibeParser.parse("no number here").is_err());
}

fn test_narrative(title: &str, impact: i32, trajectory: &str) -> Narrative {
    Narrative {
        title: title.into(),
        // Non-empty prose + derived/freshness signals on purpose: the goldens below
        // prove NONE of them enter the hash pre-image (F2 material-only debounce).
        body: "long derived prose that must never enter the hash".into(),
        impact,
        trajectory: trajectory.into(),
        topic_heat: 42,
        source_count: 5,
        source_age_days: Some(1),
    }
}

#[test]
fn input_components_are_material_only_and_sorted() {
    // Two narratives OUT of sorted order (the pre-image sorts, so weighting can't
    // perturb the hash), plus one heat line in the sigil convention.
    let narratives = vec![
        test_narrative("Trade buzz", 7, "heating_up"),
        test_narrative("Alpha story", 3, "developing_story"),
    ];
    let heat = vec![HeatItem {
        counterparty: "Arsenal".into(),
        heat: 40,
        stage: "speculation".into(),
        direction: "incoming".into(),
        // Derived commentary — excluded (the narratives precedent).
        summary: "derived commentary".into(),
        confidence: Some(0.8),
    }];
    // prompt_version leads the pre-image (single-sourced from the const, so a bump can't
    // silently rot this pin) — a v-bump changes every entity's hash once, forcing the regen.
    assert_eq!(
        build_vibe_input_components(&narratives, &heat),
        format!(
            r#"{{"prompt_version":"{VIBE_PROMPT_VERSION}","narrative_impacts":["Alpha story:3","Trade buzz:7"],"narrative_titles":["Alpha story","Trade buzz"],"narrative_trajectories":["Alpha story:developing_story","Trade buzz:heating_up"],"transfer_heat":["Arsenal:40:incoming:speculation"]}}"#
        )
    );
}

#[test]
fn input_components_empty_material_is_stable() {
    // The no-corpus marker pre-image: narrative keys always present as [], NO
    // transfer_heat key (sigil convention) — so quiet entities debounce on a stable hash.
    assert_eq!(
        build_vibe_input_components(&[], &[]),
        format!(
            r#"{{"prompt_version":"{VIBE_PROMPT_VERSION}","narrative_impacts":[],"narrative_titles":[],"narrative_trajectories":[]}}"#
        )
    );
}

#[test]
fn input_hash_ignores_narrative_ordering() {
    // order_narratives reorders for the prompt (topic-heat/impact/title); the
    // debounce key must not care.
    let a = test_narrative("Alpha story", 3, "developing_story");
    let b = test_narrative("Trade buzz", 7, "heating_up");
    let forward = build_vibe_input_components(&[a.clone(), b.clone()], &[]);
    let reversed = build_vibe_input_components(&[b, a], &[]);
    assert_eq!(hash_components(&forward), hash_components(&reversed));
}
