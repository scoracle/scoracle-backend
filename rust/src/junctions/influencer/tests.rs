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
    let p = build_sentiment_prompt("player", "Test Player", "NBA", &[], &[], &[], None, None);
    assert_eq!(
        p,
        "Entity: Player Test Player (NBA)\n\nNarratives forming around them (ordered by relevance/topic heat; impact in brackets):\n- (none this cycle)\n\nTransfer/trade chatter — the TEMPERATURE only; the wire itself is another desk's card:\n- nothing live — the wire is quiet this cycle\n\nRespond now (SCORE line, then HOOK line, then VIBE line)."
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
    let p = build_sentiment_prompt("team", "Test Team", "NFL", &[], &[], &[], Some(&previous), None);
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
        &[],
        None,
        Some(mem),
    );
    assert!(p.contains("\nRelational memory (computed history"));
    assert!(p.contains("- Prior story: Real Madrid — fizzled"));
    assert!(p.contains("- Ground truth: completed"));
    let heat_pos = p.find("Transfer/trade chatter").unwrap();
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
        build_vibe_input_components(&narratives, &heat, &[]),
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
        build_vibe_input_components(&[], &[], &[]),
        format!(
            r#"{{"prompt_version":"{VIBE_PROMPT_VERSION}","narrative_impacts":[],"narrative_titles":[],"narrative_trajectories":[]}}"#
        )
    );
}

// --- 7.6 / E3: the packet is her material, and she may file first ------------------------------

fn packet_block(id: i64) -> PacketBlock {
    PacketBlock {
        packet_id: id,
        text: "STORY: Arsenal close on Vinicius Junior\nMOOD: anticipation — \"the whole of north London is holding its breath\"\nREPORTED (newest first):\n- Football365: Arsenal have reached an agreement in principle\n".into(),
    }
}

/// The legacy pre-image is byte-identical to what shipped before Phase 7: no packets exist on
/// that rail, so the `packets` key is absent exactly the way `transfer_heat` is when the wire is
/// quiet. Every legacy `input_hash` therefore keeps serving its existing row.
#[test]
fn legacy_pre_image_carries_no_packets_key() {
    let out = build_vibe_input_components(&[], &[], &[]);
    assert!(!out.contains("packets"));
    assert_eq!(
        out,
        format!(
            r#"{{"prompt_version":"{VIBE_PROMPT_VERSION}","narrative_impacts":[],"narrative_titles":[],"narrative_trajectories":[]}}"#
        )
    );
}

/// On the packet rail the ids ARE the material key: packets are append-only snapshots, so a
/// recompiled storyline is a new id and an unmoved one keeps its own. Sorted, like every other
/// line in this pre-image, so load order cannot perturb the hash.
#[test]
fn packet_ids_enter_the_pre_image_sorted() {
    let out = build_vibe_input_components(&[], &[], &[packet_block(22), packet_block(9)]);
    assert!(out.ends_with(r#","packets":["22","9"]}"#), "{out}");
    let reversed = build_vibe_input_components(&[], &[], &[packet_block(9), packet_block(22)]);
    assert_eq!(hash_components(&out), hash_components(&reversed));
}

/// E3, the first-voice fix. Before 7.6 a packet-woken entity with no narratives and no heat
/// looked EMPTY, and `enqueue_vibe_if_needed` returned `Ok(false)` — the Influencer could not
/// speak until The Journalist had spoken for her. A packet now counts as material.
#[test]
fn a_packet_alone_is_material_enough_to_wake_her() {
    let with_packet = VibeContext {
        narratives: Vec::new(),
        heat: Vec::new(),
        news_ids: Vec::new(),
        packets: vec![packet_block(1)],
        input_components_json: String::new(),
        input_hash: String::new(),
    };
    assert!(
        !with_packet.empty(),
        "a charged packet is her material — she files first"
    );
    let nothing = VibeContext {
        packets: Vec::new(),
        ..with_packet
    };
    assert!(nothing.empty(), "no narratives, no heat, no packet ⇒ marker path");
}

/// Her prompt carries the packet ABOVE the narratives (on this rail his card may not exist yet),
/// and the legacy prompt is untouched by the arm entirely.
#[test]
fn packet_block_renders_above_the_narratives_and_never_on_legacy() {
    let legacy = build_sentiment_prompt("team", "Arsenal", "FOOTBALL", &[], &[], &[], None, None);
    assert!(!legacy.contains("The stories running around them"));

    let packet = build_sentiment_prompt(
        "team",
        "Arsenal",
        "FOOTBALL",
        &[],
        &[],
        &[packet_block(1)],
        None,
        None,
    );
    let story = packet.find("The stories running around them").expect("packet section");
    let narr = packet.find("Narratives forming around them").unwrap();
    assert!(story < narr, "the story she reads comes before his write-up of it");
    // The register and its phrase are hers, and they arrive by the renderer's voice rule.
    assert!(packet.contains("MOOD: anticipation"));
    assert!(packet.contains("holding its breath"));
}

#[test]
fn input_hash_ignores_narrative_ordering() {
    // order_narratives reorders for the prompt (topic-heat/impact/title); the
    // debounce key must not care.
    let a = test_narrative("Alpha story", 3, "developing_story");
    let b = test_narrative("Trade buzz", 7, "heating_up");
    let forward = build_vibe_input_components(&[a.clone(), b.clone()], &[], &[]);
    let reversed = build_vibe_input_components(&[b, a], &[], &[]);
    assert_eq!(hash_components(&forward), hash_components(&reversed));
}

// --- v19: the per-block char budget -----------------------------------------------------------

#[test]
fn packet_block_depth_is_bounded_in_the_prompt() {
    // A mega-storyline's single block ran ~2k tokens of claims (measured 2026-08-15) — the
    // prompt spends at most PACKET_BLOCK_TRUNCATE chars per block, keeping the top (newest
    // claims render first). A normal block is untouched.
    let mut big = packet_block(1);
    big.text = format!(
        "STORY: The saga\nMOOD: weary — \"here we go again\"\nREPORTED (newest first):\n{}",
        "- Outlet: a long claim line repeated far past any reasonable allowance.\n".repeat(400)
    );
    let p = build_sentiment_prompt("team", "Test FC", "FOOTBALL", &[], &[], &[big], None, None);
    let story_at = p.find("STORY: The saga").expect("block renders");
    let narratives_at = p.find("Narratives forming").expect("next section renders");
    assert!(
        narratives_at - story_at <= PACKET_BLOCK_TRUNCATE + 8,
        "block spent {} chars, allowance is {}",
        narratives_at - story_at,
        PACKET_BLOCK_TRUNCATE
    );

    let small = packet_block(2);
    let q = build_sentiment_prompt("team", "Test FC", "FOOTBALL", &[], &[], &[small.clone()], None, None);
    assert!(q.contains(small.text.trim_end()), "a normal block renders whole");
}

/// v22: the Insider's ledger must not reach the Influencer — only its temperature.
#[test]
fn the_wire_reaches_her_as_temperature_never_as_a_ledger() {
    let heat = vec![
        HeatItem {
            counterparty: "Tottenham Hotspur".to_string(),
            heat: 78,
            direction: "outgoing".to_string(),
            stage: "advanced".to_string(),
            confidence: Some(0.9),
            summary: "Direct pursuit with club-level backing.".to_string(),
        },
        HeatItem {
            counterparty: "Celtic".to_string(),
            heat: 30,
            direction: "incoming".to_string(),
            stage: "speculation".to_string(),
            confidence: Some(0.3),
            summary: "Low-confidence tracking only.".to_string(),
        },
    ];
    let p = build_sentiment_prompt("team", "West Ham United", "FOOTBALL", &[], &heat, &[], None, None);

    // The temperature, in words, with the departure signal her SCORE anchors rely on.
    assert!(
        p.contains("- loud — 2 live threads, movement both ways"),
        "the wire's temperature must reach her: {p}"
    );

    // ...and not one line of the Insider's card. Before v22 this prompt rendered
    // `write_heat_lines`, the same function that builds HIS wrap, and 75% of her felt reads
    // came back talking transfer mechanics.
    for leaked in [
        "Tottenham Hotspur",
        "Celtic",
        "advanced",
        "speculation",
        "confidence",
        "Direct pursuit with club-level backing",
        "78",
    ] {
        assert!(
            !p.contains(leaked),
            "the Insider's ledger leaked {leaked:?} into the Influencer's prompt: {p}"
        );
    }
}

/// An unsalvageable hook costs the title, never the card. Measured 2026-08-23: 39 vibe items
/// dead-lettered at max attempts on hooks that were GOOD, merely one to five words long and
/// single-clause, so salvage_hook had no beat separator to cut at. Each retry discarded the
/// SCORE too — and the score feeds momentum, so a lost vibe corrupts a downstream trajectory.
#[test]
fn an_unsalvageable_hook_never_costs_the_card() {
    let reply = "SCORE: 62\n\
                 HOOK: Brandt walks into the market like a man who's already out of options.\n\
                 VIBE: The room around Brandt has gone quiet in the way rooms do when everyone \
                 already knows. Nobody is arguing about his future any more.";
    let got = VibeParser
        .parse(reply)
        .expect("a long single-clause hook must not fail the card")
        .expect("a reply");

    assert_eq!(got.sentiment, 62, "the score survives, and momentum reads it");
    assert!(got.vibe_prompt.contains("gone quiet"), "the body survives: {:?}", got.vibe_prompt);
    assert!(
        got.hook.as_deref().is_none_or(|h| crate::guards::hook_violation(h).is_none()),
        "a shipped hook always satisfies the contract: {:?}", got.hook
    );

    // Since 2026-08-24 a two-beat hook under 140 chars SHIPS WHOLE — the twist is the
    // Influencer's voice, and the guard's only business is whether it fits a leaderboard row.
    let two_beat = VibeParser
        .parse("SCORE: 40\nHOOK: The room has turned on him — and the window is closing fast\nVIBE: Flat.")
        .expect("clean")
        .expect("a reply");
    assert_eq!(
        two_beat.hook.as_deref(),
        Some("The room has turned on him — and the window is closing fast")
    );

    // Punctuation is voice too: a colon and a question mark both survive to the card.
    for hook in ["Breaking: the room has turned", "Is the window already shut?"] {
        let got = VibeParser
            .parse(&format!("SCORE: 40\nHOOK: {hook}\nVIBE: Flat."))
            .expect("clean")
            .expect("a reply");
        assert_eq!(got.hook.as_deref(), Some(hook), "punctuation cost the hook");
    }
}

/// Markdown in the body costs the decoration, never the card. Measured 2026-08-23: 89 vibe
/// items dead-lettered on `body carries banned "**"` — the largest failure bucket on the rail,
/// discarding finished felt reads and the SCORE momentum depends on, over typography.
#[test]
fn markdown_in_the_body_is_stripped_not_fatal() {
    let reply = "SCORE: 71\n\
                 HOOK: The room has turned\n\
                 VIBE: **Brandt** is the story now, and the feed knows it. The mood around \
                 him is __loud__ in a way it has not been all season.";
    let got = VibeParser
        .parse(reply)
        .expect("emphasis must not fail the card")
        .expect("a reply");

    assert_eq!(got.sentiment, 71, "the score survives for momentum");
    assert!(!got.vibe_prompt.contains("**"), "emphasis stripped: {:?}", got.vibe_prompt);
    assert!(!got.vibe_prompt.contains("__"), "emphasis stripped: {:?}", got.vibe_prompt);
    assert!(got.vibe_prompt.contains("Brandt is the story now"), "prose intact: {:?}", got.vibe_prompt);
    assert!(got.vibe_prompt.contains("loud in a way"), "prose intact: {:?}", got.vibe_prompt);
}
