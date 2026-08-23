//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

fn momentum(direction: &str) -> SynthMomentum {
    SynthMomentum {
        direction: Some(direction.into()),
        momentum_score: Some(2.0),
        ..SynthMomentum::default()
    }
}

#[test]
fn crown_parses_reading_and_score() {
    let r =
        parse_crown_reply(r#"{"reading": "The arc holds. Winter stirs.", "score": 73}"#).unwrap();
    assert_eq!(r.reading, "The arc holds. Winter stirs.");
    assert_eq!(r.score, 73);
}

#[test]
fn crown_salvages_prose_wrapped_json_and_collapses_whitespace() {
    let r = parse_crown_reply(
        "Here:\n{\"reading\": \"Line one.\\n  Line two.\", \"score\": 60}\nDone.",
    )
    .unwrap();
    assert_eq!(r.reading, "Line one. Line two.");
    assert_eq!(r.score, 60);
}

#[test]
fn crown_salvages_fenced_json_with_literal_newlines_in_string() {
    // The measured oMLX class (2026-08-10): unconstrained decoding fences the object AND writes
    // a paragraph break as a LITERAL newline inside the JSON string — illegal JSON that failed a
    // complete reply. The salvage folds control chars in the brace span; the reading normalizer
    // collapses the whitespace.
    let raw = "```json\n{\n  \"reading\": \"The arc climbs.\n\nThe rim is a fortress.\",\n  \"score\": 92\n}\n```";
    let r = parse_crown_reply(raw).unwrap();
    assert_eq!(r.reading, "The arc climbs. The rim is a fortress.");
    assert_eq!(r.score, 92);
}

#[test]
fn crown_score_coercions_and_clamp() {
    // Float, "N/100" string, and out-of-range all coerce + clamp to 1-100.
    assert_eq!(
        parse_crown_reply(r#"{"reading":"x.","score":91.6}"#)
            .unwrap()
            .score,
        92
    );
    assert_eq!(
        parse_crown_reply(r#"{"reading":"x.","score":"48/100"}"#)
            .unwrap()
            .score,
        48
    );
    assert_eq!(
        parse_crown_reply(r#"{"reading":"x.","score":250}"#)
            .unwrap()
            .score,
        100
    );
    assert_eq!(
        parse_crown_reply(r#"{"reading":"x.","score":0}"#)
            .unwrap()
            .score,
        1
    );
}

#[test]
fn crown_fail_closed_on_missing_reading_or_score() {
    assert!(parse_crown_reply(r#"{"reading":"   ","score":50}"#).is_none());
    assert!(parse_crown_reply(r#"{"score":50}"#).is_none());
    assert!(parse_crown_reply(r#"{"reading":"x."}"#).is_none());
    assert!(parse_crown_reply(r#"{"reading":"x.","score":"elite"}"#).is_none());
    assert!(parse_crown_reply("no json at all").is_none());
}

#[test]
fn crown_parser_is_fail_closed_err_not_none() {
    assert!(CrownParser.parse("not a reply").is_err());
    let ok = CrownParser
        .parse(r#"{"reading":"The spread is quiet.","score":55}"#)
        .unwrap()
        .expect("a valid reply is Some, never the fail-closed None");
    assert_eq!(ok.score, 55);
    assert_eq!(ok.reading, "The spread is quiet.");
}

#[test]
fn crown_parses_optional_headline() {
    // or11: present + non-empty → folded to one line; absent or empty → None (tolerance,
    // never a failed generation).
    let r = parse_crown_reply(
        r#"{"reading": "The arc holds.", "headline": "  Winter   stirs for Vale ", "score": 73}"#,
    )
    .unwrap();
    assert_eq!(r.headline.as_deref(), Some("Winter stirs for Vale"));
    let absent =
        parse_crown_reply(r#"{"reading": "The arc holds.", "score": 73}"#).unwrap();
    assert!(absent.headline.is_none());
    let empty = parse_crown_reply(
        r#"{"reading": "The arc holds.", "headline": "", "score": 73}"#,
    )
    .unwrap();
    assert!(empty.headline.is_none());
}

/// The card title FAILS OPEN, via the shared `guards::settle_title` (2026-08-23). This test
/// asserted the opposite until then — "a violation is Err, the item re-rolls" — which was never
/// true of any other seat and cost whole cards on three of them before it was noticed.
#[test]
fn crown_headline_fails_open_and_never_costs_the_reading() {
    for junk in [
        r#"{"reading":"The arc holds.","headline":"one two three four five six seven eight nine ten eleven twelve thirteen","score":50}"#,
        r#"{"reading":"The arc holds.","headline":"Vale: a crossroads","score":50}"#,
        r#"{"reading":"The arc holds.","headline":"Will winter stir?","score":50}"#,
    ] {
        let got = CrownParser
            .parse(junk)
            .expect("a junk title must not fail the reading")
            .expect("a reply");
        assert!(got.reading.contains("The arc holds"), "the reading survives: {got:?}");
        assert!(
            got.headline.as_deref().is_none_or(|h| crate::guards::hook_violation(h).is_none()),
            "a shipped title always satisfies the contract: {:?}", got.headline
        );
    }
    let clean = CrownParser
        .parse(r#"{"reading":"x.","headline":"Winter stirs at the Emirates","score":50}"#)
        .unwrap()
        .unwrap();
    assert_eq!(clean.headline.as_deref(), Some("Winter stirs at the Emirates"));
}

/// The Oracle had NO markdown protection until 2026-08-23 — not a strip, not a ban — which is
/// why its own fixture gate reported `reading_plain_text — found '*'`.
#[test]
fn the_crown_reading_is_scrubbed_of_emphasis() {
    let got = CrownParser
        .parse(r#"{"reading":"The **arc** holds, and the wire is __quiet__.","score":50}"#)
        .unwrap()
        .unwrap();
    assert!(!got.reading.contains("**") && !got.reading.contains("__"), "scrubbed: {:?}", got.reading);
    assert!(got.reading.contains("The arc holds"), "prose intact: {:?}", got.reading);
}

#[test]
fn counts_sentences_ignoring_decimals() {
    assert_eq!(count_sentences("One. Two! Three?"), 3);
    assert_eq!(
        count_sentences("He averages 2.5 assists. The arc holds."),
        2
    );
    assert_eq!(count_sentences("no terminator"), 0);
}

#[test]
fn pillar_convergence_is_agree_ratio_floored_to_db_contract() {
    let agree = PillarComparison {
        label: "a".into(),
        agree: true,
    };
    let disagree = PillarComparison {
        label: "b".into(),
        agree: false,
    };
    assert_eq!(pillar_convergence(&[]), None);
    assert_eq!(
        pillar_convergence(&[agree.clone(), agree.clone()]),
        Some(100)
    );
    assert_eq!(
        pillar_convergence(&[agree.clone(), disagree.clone()]),
        Some(50)
    );
    // All-disagree rounds to 0, which sigil_synthesis_convergence_check rejects (NULL or
    // 1-100) — the floor keeps the persist valid; ≤ 50 is a crossroads either way.
    assert_eq!(
        pillar_convergence(&[disagree.clone(), disagree.clone()]),
        Some(1)
    );
    assert_eq!(
        pillar_convergence(&[agree.clone(), agree.clone(), disagree.clone()]),
        Some(67)
    );
}

#[test]
fn omen_crossroads_when_half_or_more_disagree() {
    // convergence ≤ 50 ⇒ crossroads regardless of direction (half-or-more disagree).
    assert_eq!(compute_omen(Some(50), &momentum("rising")).0, "crossroads");
    assert_eq!(compute_omen(Some(40), &momentum("rising")).0, "crossroads");
}

#[test]
fn omen_momentum_decides_alone() {
    // or10: Momentum's decided sign is the sole direction input.
    assert_eq!(compute_omen(Some(80), &momentum("rising")).0, "ascendant");
    // Momentum falling → waning.
    assert_eq!(compute_omen(None, &momentum("falling")).0, "waning");
    // Nothing directional → steady.
    assert_eq!(
        compute_omen(Some(90), &SynthMomentum::default()).0,
        "steady"
    );
}

#[test]
fn trend_dir_buckets() {
    assert_eq!(trend_dir(2.0), "trending up strongly");
    assert_eq!(trend_dir(0.5), "trending up");
    assert_eq!(trend_dir(0.0), "steady");
    assert_eq!(trend_dir(-0.5), "trending down");
    assert_eq!(trend_dir(-2.0), "trending down strongly");
    // Boundary: exactly 0.3 is NOT > 0.3 → steady (mirrors Go's strict `>`).
    assert_eq!(trend_dir(0.3), "steady");
}

#[test]
fn momentum_score_tracks_direction_not_quality() {
    let surging = SynthMomentum {
        vibe_slope: Some(2.0),
        vibe_samples: 4,
        rating_slope: Some(2.0),
        rating_samples: 5,
        momentum_score: Some(4.0),
        ..SynthMomentum::default()
    };
    assert_eq!(momentum_score(&surging), Some(4));
    assert_eq!(momentum_score_label(4), "surging");

    let sliding = SynthMomentum {
        vibe_slope: Some(-2.0),
        vibe_samples: 3,
        rating_slope: Some(-1.0),
        rating_samples: 3,
        momentum_score: Some(-1.5),
        ..SynthMomentum::default()
    };
    assert_eq!(momentum_score(&sliding), Some(-2));
    assert_eq!(momentum_score_label(-2), "sliding");

    assert_eq!(momentum_score(&SynthMomentum::default()), None);
}

#[test]
fn linear_slope_of_a_rising_series() {
    // A perfectly linear +5/step series ⇒ slope 5.
    assert!((linear_slope(&[10.0, 15.0, 20.0, 25.0]) - 5.0).abs() < 1e-9);
    // Fewer than two points ⇒ 0.
    assert_eq!(linear_slope(&[42.0]), 0.0);
    assert_eq!(linear_slope(&[]), 0.0);
}

#[test]
fn round1_matches_go() {
    assert_eq!(round1(73.04), 73.0);
    assert_eq!(round1(73.05), 73.1); // half away from zero
    assert_eq!(round1(73.0), 73.0);
}

#[test]
fn go_json_encoders_single_homed_in_util() {
    // The leaf encoders + hash now live in `crate::util` (single-homed post-L12); the
    // shadow-table parity gate is the regression check, and util.rs carries its own
    // pinning tests for them. Sigil re-pins the COMPOSITION here (see the next test).
}

#[test]
fn input_components_use_stable_go_json_shape() {
    // Validates sorted keys + HTML escaping + Go float form + int form together — the
    // canonical JSON whose SHA-256 is the input_hash. Compare against the exact bytes Go's
    // json.Marshal would emit for the same map.
    let narratives = vec![
        SynthNarrative {
            title: "B & C".into(),
            body: "x".into(),
            impact: 5.0,
            trajectory: "heating_up".into(),
            source_count: 0,
            source_age_days: None,
        },
        SynthNarrative {
            title: "Alpha".into(),
            body: "y".into(),
            impact: 3.0,
            trajectory: DEFAULT_TRAJECTORY.into(),
            source_count: 0,
            source_age_days: None,
        },
    ];
    let rating = SynthRating {
        body: "z".into(),
        notability: 88,
        rating_trajectory: "falling".into(),
        rating_trajectory_label: "Composite and PEAK z-scores trending down over recent games"
            .into(),
    };
    let mom = SynthMomentum {
        vibe_slope: Some(1.0),
        vibe_samples: 4,
        rating_slope: Some(0.0),
        rating_samples: 5,
        momentum_score: Some(2.5),
        blurb: Some("PEAK is sliding while Vibe holds.".into()),
        input_hash: Some("a1b2c3d4e5f60718293a4b5c6d7e8f90".into()),
        ..SynthMomentum::default()
    };
    let vibe = SynthVibe {
        sentiment: 60,
        prompt: "Quietly surging".into(),
    };
    let got = build_synthesis_input_components(&narratives, Some(&rating), Some(&vibe), &mom, &[]);
    // "B & C"'s ampersand is HTML-escaped (the backslash-u form), exactly as Go's
    // json.Marshal emits it. Built via format! with a runtime backslash (bs) so the
    // source carries no literal backslash-u token (the editor would decode it).
    // The vibe prompt and momentum blurb are non-empty on purpose: the golden proves the
    // upstream model prose is NOT in the hash pre-image (F1 material-only debounce) —
    // vibe contributes only vibe_sentiment, momentum its material-only summary hash.
    let bs = '\\';
    let want = format!(
        r#"{{"momentum_rating_samples":5,"momentum_rating_slope":0,"momentum_score":2.5,"momentum_summary_hash":"a1b2c3d4e5f60718293a4b5c6d7e8f90","momentum_vibe_samples":4,"momentum_vibe_slope":1,"narrative_titles":["Alpha","B {bs}u0026 C"],"narrative_trajectories":["Alpha:developing_story","B {bs}u0026 C:heating_up"],"notability":88,"vibe_sentiment":60}}"#
    );
    assert_eq!(got, want);
}

#[test]
fn input_components_narrative_titles_always_present() {
    // Rating-only entity: narrative_titles is still present as [] by contract.
    let rating = SynthRating {
        body: "b".into(),
        notability: 40,
        rating_trajectory: "steady".into(),
        rating_trajectory_label: String::new(),
    };
    let got =
        build_synthesis_input_components(&[], Some(&rating), None, &SynthMomentum::default(), &[]);
    assert_eq!(
        got,
        r#"{"narrative_titles":[],"narrative_trajectories":[],"notability":40}"#
    );
}

#[test]
fn transfer_heat_enters_components_only_when_present() {
    // No transfers → NO transfer_heat key at all (so a pre-Phase-5.1 entity keeps its hash).
    let without = build_synthesis_input_components(&[], None, None, &SynthMomentum::default(), &[]);
    assert_eq!(
        without,
        r#"{"narrative_titles":[],"narrative_trajectories":[]}"#
    );

    // Served heat → one sorted "counterparty:heat:direction:stage" line per rumor. The two
    // rumors are given OUT of sorted order to prove the pre-image sorts (stable hash).
    let transfers = vec![
        HeatItem {
            counterparty: "Real Madrid".into(),
            heat: 71,
            stage: "advanced_talks".into(),
            direction: "outgoing".into(),
            summary: String::new(),
            confidence: None,
        },
        HeatItem {
            counterparty: "Arsenal".into(),
            heat: 40,
            stage: "speculation".into(),
            direction: "incoming".into(),
            summary: String::new(),
            confidence: None,
        },
    ];
    let with =
        build_synthesis_input_components(&[], None, None, &SynthMomentum::default(), &transfers);
    assert_eq!(
        with,
        r#"{"narrative_titles":[],"narrative_trajectories":[],"transfer_heat":["Arsenal:40:incoming:speculation","Real Madrid:71:outgoing:advanced_talks"]}"#
    );
}

#[test]
fn crown_prompt_renders_cards_and_omen() {
    // entity_type is raw ("player", not "Player"); sport uses the passed (raw) case. The rich
    // pillar cards render (the crown scores from them); the OMEN closes; no PRIOR READ block.
    let narratives = vec![SynthNarrative {
        title: "Trade buzz".into(),
        body: "details".into(),
        impact: 7.0,
        trajectory: "heating_up".into(),
        source_count: 3,
        source_age_days: Some(1),
    }];
    let mom = SynthMomentum {
        vibe_slope: Some(0.5),
        vibe_samples: 4,
        momentum_score: Some(1.0),
        ..SynthMomentum::default()
    };
    let vibe = SynthVibe {
        sentiment: 62,
        prompt: "On the rise".into(),
    };
    let p = build_crown_prompt(
        "player",
        "Test Player",
        "NBA",
        &narratives,
        None,
        Some(&vibe),
        &mom,
        &[],
        "steady",
        "the arc holds its line",
        None,
    );
    assert!(p.starts_with("Entity: Test Player (NBA player)\n"));
    assert!(!p.contains("YOUR PRIOR READ"));
    assert!(!p.contains("RELATIONAL MEMORY"));
    assert!(p.contains("=== THE JOURNALIST'S CARD (news storylines) ===\n[impact 7, Heating up, 3 sources, latest 1d ago] Trade buzz\ndetails"));
    assert!(p.contains("=== THE SCOUT'S CARD (scouting brief) ===\n(no stat commentary available)"));
    assert!(p.contains("=== THE INFLUENCER'S CARD (the felt read) ===\nMood: 62/100\nOn the rise"));
    assert!(p.contains("=== THE ANALYST'S CARD (momentum) ===\nMomentum score: 1 (rising)\nMood trend: 0.5 over 4 samples (trending up)"));
    assert!(p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n(no active transfer rumors)"));
    assert!(p.contains("=== THE OMEN (computed) ===\nOmen: steady — the arc holds its line\n"));
    assert!(p.ends_with(
        "\nYour peers have spoken; the table is yours. Read their cards, then render the score."
    ));
}

/// 7.8, the 4096 envelope: on the packet rail every pillar body is capped and the Journalist's
/// card is capped as ONE card — at most three storylines, sharing the budget, with the remainder
/// NAMED (A5). On the legacy rail nothing truncates, which is why the two prompts below differ
/// only where the cap bites.
#[test]
fn packet_rail_caps_every_pillar_body_and_names_what_it_dropped() {
    let long = "word ".repeat(2_000);
    let narratives: Vec<SynthNarrative> = (0..5)
        .map(|i| SynthNarrative {
            title: format!("Storyline {i}"),
            body: long.clone(),
            impact: 7.0,
            trajectory: "heating_up".into(),
            source_count: 3,
            source_age_days: Some(1),
        })
        .collect();
    let rating = SynthRating {
        body: long.clone(),
        notability: 71,
        rating_trajectory: "rising".into(),
        rating_trajectory_label: "Composite trending up over recent games".into(),
    };
    let vibe = SynthVibe {
        sentiment: 62,
        prompt: long.clone(),
    };
    let mom = SynthMomentum {
        blurb: Some(long.clone()),
        direction: Some("rising".into()),
        momentum_score: Some(30.0),
        ..SynthMomentum::default()
    };

    let uncapped = build_crown_prompt(
        "player",
        "Test Player",
        "NBA",
        &narratives,
        Some(&rating),
        Some(&vibe),
        &mom,
        &[],
        "ascendant",
        "the arc climbs",
        None,
    );
    let capped = build_crown_prompt(
        "player",
        "Test Player",
        "NBA",
        &narratives,
        Some(&rating),
        Some(&vibe),
        &mom,
        &[],
        "ascendant",
        "the arc climbs",
        Some(CROWN_CARD_BODY_CAP),
    );

    // Legacy truncates nothing — that is the behaviour a 16,384-token window allowed, and it is
    // what the legacy rail keeps sending.
    assert!(uncapped.len() > 5 * long.len());
    assert!(!uncapped.contains("not shown — budget"));
    // The packet rail's whole crown prompt is now smaller than ONE uncapped card.
    assert!(
        capped.len() < long.len(),
        "capped crown prompt is {} bytes",
        capped.len()
    );
    assert!(capped.contains("(+2 more storyline(s) not shown — budget)"));
    assert!(capped.contains("Storyline 0"));
    assert!(
        !capped.contains("Storyline 3"),
        "the card is capped as ONE card"
    );
    // Every card still SPEAKS — a cap that silences a pillar would change the verdict, not the
    // window. All five headers stand, and the omen still closes.
    for header in [
        "THE JOURNALIST'S CARD",
        "THE SCOUT'S CARD",
        "THE INFLUENCER'S CARD",
        "THE ANALYST'S CARD",
        "THE INSIDER'S CARD",
        "THE OMEN (computed)",
    ] {
        assert!(capped.contains(header), "{header} lost to the cap");
    }
}

#[test]
fn pillar_divergence_names_the_rail_conflict() {
    // The fixture-measured sigil failure: strong-but-falling PEAK, positive vibe, falling
    // momentum. The card must hand the model both DISAGREE pairs deterministically.
    let rating = SynthRating {
        body: "b".into(),
        notability: 88,
        rating_trajectory: "falling".into(),
        rating_trajectory_label: String::new(),
    };
    let vibe = SynthVibe {
        sentiment: 75,
        prompt: "warm".into(),
    };
    let mom = SynthMomentum {
        direction: Some("falling".into()),
        momentum_score: Some(-2.0),
        ..SynthMomentum::default()
    };
    let c = build_pillar_divergence(&[], Some(&rating), Some(&vibe), &mom);
    let rendered: Vec<(String, bool)> = c.into_iter().map(|x| (x.label, x.agree)).collect();
    // or10: the raw trajectory marker left the crown's math — only Vibe/Momentum and the two
    // Profile-strength LEVEL pairs remain.
    assert_eq!(
        rendered,
        vec![
            ("Vibe (positive) vs Momentum (negative)".to_string(), false),
            (
                "Profile strength (strong) vs Momentum (negative)".to_string(),
                false
            ),
            (
                "Profile strength (strong) vs Vibe (positive)".to_string(),
                true
            ),
        ]
    );
}

#[test]
fn pillar_divergence_skips_neutral_and_absent_signals() {
    // Steady momentum, mid-band vibe, no rating: nothing directional → empty card → None
    // convergence (a quiet spread has nothing to converge on).
    let vibe = SynthVibe {
        sentiment: 50,
        prompt: String::new(),
    };
    let mom = SynthMomentum {
        direction: Some("steady".into()),
        ..SynthMomentum::default()
    };
    let c = build_pillar_divergence(&[], None, Some(&vibe), &mom);
    assert!(c.is_empty());
    assert_eq!(pillar_convergence(&c), None);
}

#[test]
fn crown_prompt_no_momentum_data_line() {
    let p = build_crown_prompt(
        "team",
        "Test Team",
        "NFL",
        &[],
        None,
        None,
        &SynthMomentum::default(),
        &[],
        "steady",
        "r",
        None,
    );
    assert!(p.contains("=== THE JOURNALIST'S CARD (news storylines) ===\n(no recent narratives)"));
    assert!(p.contains("=== THE ANALYST'S CARD (momentum) ===\n(no momentum data)"));
    assert!(p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n(no active transfer rumors)"));
}

#[test]
fn crown_prompt_transfer_heat_renders() {
    // A team with one served rumor: the P5 section renders through the shared write_heat_lines
    // format (`- <counterparty> — heat <n>, <direction>, <stage>`).
    let transfers = vec![HeatItem {
        counterparty: "Liverpool".into(),
        heat: 66,
        stage: "advanced_talks".into(),
        direction: "incoming".into(),
        summary: String::new(),
        confidence: None,
    }];
    let p = build_crown_prompt(
        "team",
        "Test Team",
        "FOOTBALL",
        &[],
        None,
        None,
        &SynthMomentum::default(),
        &transfers,
        "steady",
        "r",
        None,
    );
    assert!(p.contains("=== THE INSIDER'S CARD (transfer wire) ===\n- Liverpool — heat 66, incoming, advanced_talks\n"));
}

#[test]
fn crown_prompt_is_blind_to_memories() {
    // or9 (Scott, 2026-08-10): the crown reads the five cards + the omen, whole — no prior-read
    // block, no relational-memory card. The cards follow the Entity line directly.
    let p = build_crown_prompt(
        "player",
        "Test Player",
        "NBA",
        &[],
        None,
        None,
        &SynthMomentum::default(),
        &[],
        "steady",
        "r",
        None,
    );
    assert!(p.starts_with(
        "Entity: Test Player (NBA player)\n\n=== THE JOURNALIST'S CARD (news storylines) ==="
    ));
    assert!(!p.contains("YOUR PRIOR READ"));
    assert!(!p.contains("RELATIONAL MEMORY"));
}

#[test]
fn synthesis_hash_preimage_is_pillar_inputs_only() {
    // The debounce hash pre-image is built from pillar inputs alone and is deterministic. (This
    // test's old name asserted the continuity reads stayed OUT of the hash; or9 deleted those
    // reads entirely, and what remains worth pinning is the pre-image's shape + determinism.)
    let mom = SynthMomentum::default();
    let a = build_synthesis_input_components(&[], None, None, &mom, &[]);
    let b = build_synthesis_input_components(&[], None, None, &mom, &[]);
    assert_eq!(a, b);
    assert_eq!(a, r#"{"narrative_titles":[],"narrative_trajectories":[]}"#);
}

/// The Scout's z-notation must not reach the crown. Measured 2026-08-23, a live failure:
/// `crown: reading carries banned vocabulary "z-score"` — the word was in the card it was
/// handed, not in the Oracle's head. Same fix this file already applied to "notability" and
/// "Sentiment"; z survived both because it hides in free prose rather than a labelled line.
#[test]
fn the_scouts_z_notation_never_reaches_the_crown() {
    let brief = "Blocked shots are elite at 172 (98th percentile, z +1.8). \
                 Shots on target allowed sit at the 4th percentile, z -1.9, the clear liability. \
                 Interceptions are ordinary at the 54th percentile (z 0.1).";
    let got = super::prompt::descrub_z(brief);

    assert!(!got.contains(" z "), "z tokens survive: {got}");
    assert!(!got.contains("z +") && !got.contains("z -"), "signed z survives: {got}");

    // The evidence the Oracle IS allowed to speak stays intact.
    assert!(got.contains("98th percentile"), "percentiles survive: {got}");
    assert!(got.contains("4th percentile"), "percentiles survive: {got}");
    assert!(got.contains("172"), "raw values survive: {got}");
    assert!(got.contains("elite"), "tiers survive: {got}");

    // And the prose is not left littered with stranded punctuation.
    assert!(!got.contains(" )") && !got.contains("()") && !got.contains(" ,"), "litter: {got}");

    // A word merely starting with z is not a z-token.
    assert_eq!(super::prompt::descrub_z("zonal marking at 60th"), "zonal marking at 60th");
}
