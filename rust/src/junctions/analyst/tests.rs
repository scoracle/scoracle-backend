//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

#[test]
fn parses_momentum_reply() {
    // s11: a SCORE line is tolerated (every contract through s10 asked for one) but ignored.
    let parsed =
        parse_momentum_reply("SCORE: 3\nREAD: PEAK is rising while Vibe is calm.").unwrap();
    assert_eq!(parsed.blurb, "PEAK is rising while Vibe is calm.");
    // ...and the READ alone is now the whole contract.
    let bare = parse_momentum_reply("READ: PEAK is rising while Vibe is calm.").unwrap();
    assert_eq!(bare.blurb, "PEAK is rising while Vibe is calm.");
}

#[test]
fn parses_a_markdown_decorated_reply_the_2026_07_26_split_regression() {
    // Verbatim from pipeline_work.last_error. The topology split moved The Analyst onto
    // ministral-3:14b, which labels with Markdown; `**SCORE: -1**` does not start with
    // `SCORE:`, so every reply was rejected as "momentum: invalid response" and the item
    // failed and retried. A model swap must not be able to silently cost a whole junction.
    //
    // Kept across the s11 merge with its score assertion dropped, NOT weakened by accident:
    // s11 removed `MomentumReply.score` entirely (the ±5 conviction is computed from
    // momentum_score now), so there is no longer a field to assert. What this test still
    // guards is the part that broke production — that a Markdown-labeled reply PARSES AT ALL
    // rather than failing the item — plus the card-facing requirement below.
    let parsed = parse_momentum_reply(
        "**SCORE: -1**\n**READ:** Clark's **PEAK** remains flat—no change in his tackling.",
    )
    .expect("a Markdown-labeled reply must parse");
    // The emphasis inside the prose is stripped too: this text reaches a card, and a card
    // must never render literal asterisks.
    assert_eq!(
        parsed.blurb,
        "Clark's PEAK remains flat—no change in his tackling."
    );
}

#[test]
fn stray_momentum_line_is_tolerated_and_ignored() {
    // s4 dropped MOMENTUM from the contract; a model echoing the decided direction (in
    // any word, even the old Conflict failure) is skipped, never parsed as content.
    let parsed = parse_momentum_reply(
        "MOMENTUM: Conflict\nSCORE: -1.0\nREAD: Signals split between PEAK and Vibe.",
    )
    .unwrap();
    assert_eq!(parsed.blurb, "Signals split between PEAK and Vibe.");
    // An empty READ still fails closed — but a missing SCORE no longer does (s11).
    assert!(parse_momentum_reply("SCORE: 2").is_none());
    assert!(parse_momentum_reply("").is_none());
    assert!(parse_momentum_reply("READ: prose only, no score.").is_some());
}

#[test]
fn parses_the_defiant_fable_relabelings_the_2026_08_14_swap() {
    // Verbatim shapes from pipeline_work.last_error after the 08-14 swap to
    // defiant-fable:9b. Shape one: the whole reply on a `Momentum:` line, prose after
    // the direction/score echo — the echo goes, the prose is the READ.
    let one_line = parse_momentum_reply(
        "Momentum: Steady (-8.8/±10). The camp narrative remains fixture-focused with no new directional shift.",
    )
    .expect("a one-line Momentum: reply with prose must parse");
    assert_eq!(
        one_line.blurb,
        "The camp narrative remains fixture-focused with no new directional shift."
    );

    // Shape two: a `Momentum Read:` headline echo, the read in the paragraph below.
    let headline = parse_momentum_reply(
        "Momentum Read: Indianapolis Colts — Falling (-22.4)\n\nThe Colts are trending down as camp momentum shifts elsewhere.",
    )
    .expect("a Momentum Read: headline reply must parse");
    assert_eq!(
        headline.blurb,
        "The Colts are trending down as camp momentum shifts elsewhere."
    );

    // Prose on the relabeled line itself is kept too.
    let inline = parse_momentum_reply(
        "MOMENTUM READ: Falling (-22.4). The tape shows the drop across both windows.",
    )
    .unwrap();
    assert_eq!(inline.blurb, "The tape shows the drop across both windows.");

    // A bare echo with nothing under it still fails closed.
    assert!(parse_momentum_reply("Momentum: Steady (-8.8/±10).").is_none());
    assert!(parse_momentum_reply("MOMENTUM: sideways").is_none());
}

#[test]
fn foreign_script_leak_fails_closed_the_3b_delegation_glitch() {
    // Verbatim class from the 2026-08-15 delegation to ministral-3:3b: an Arabic run
    // mid-word in card-facing prose. The reply is well-formed by the label contract, so
    // only a content check catches it — reject and let the retry re-roll.
    assert!(parse_momentum_reply("READ: His playmaking has زمنed in Milwaukee.").is_none());
    // Latin diacritics are names, not leaks.
    let ok = parse_momentum_reply("READ: Éder Militão's form is falling, and the tape backs the drop.")
        .expect("diacritics must pass");
    assert!(ok.blurb.contains("Militão"));
}

#[test]
fn parses_the_s17_headline_line() {
    // s17 (mig 226): HEADLINE after the READ is the contracted position.
    let parsed = parse_momentum_reply(
        "READ: The form is rising and the mood confirms it.\nHEADLINE: Form and mood rise together for Vale",
    )
    .expect("a contracted headline must parse");
    assert_eq!(
        parsed.headline.as_deref(),
        Some("Form and mood rise together for Vale")
    );
    assert_eq!(parsed.blurb, "The form is rising and the mood confirms it.");

    // Absent line → None (tolerance, never a failed generation).
    let bare = parse_momentum_reply("READ: The tape is steady.").unwrap();
    assert!(bare.headline.is_none());

    // Order drift (title first) still captures both — a shape quirk, not a failure.
    let drifted = parse_momentum_reply(
        "HEADLINE: Kerr holds the line\nREAD: The form is holding while the mood wobbles.",
    )
    .unwrap();
    assert_eq!(drifted.headline.as_deref(), Some("Kerr holds the line"));
    assert_eq!(drifted.blurb, "The form is holding while the mood wobbles.");

    // A title ends the READ: prose after it belongs to the title, not the read.
    let trailing = parse_momentum_reply(
        "READ: First sentence.\nHEADLINE: The title\nSome trailing note.",
    )
    .unwrap();
    assert_eq!(trailing.blurb, "First sentence.");
    assert_eq!(trailing.headline.as_deref(), Some("The title"));
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
fn conviction_is_computed_not_asked_of_the_model() {
    // s11: the ±5 magnitude is derived from the SAME ±100 momentum_score that decides
    // direction, so the pair can never disagree — the failure class the old clamp existed
    // to paper over is now unrepresentable.
    assert_eq!(momentum_conviction_from_score(None), 0);
    assert_eq!(momentum_conviction_from_score(Some(0.0)), 0);
    assert_eq!(momentum_conviction_from_score(Some(4.9)), 0); // flat: under half the band

    // Inside the steady band a real lean still reads as ±1 (the old contract's steady range).
    assert_eq!(momentum_conviction_from_score(Some(5.0)), 1);
    assert_eq!(momentum_conviction_from_score(Some(-9.9)), -1);

    // At and beyond the band the ladder opens up — the range the model never used.
    assert_eq!(momentum_conviction_from_score(Some(10.0)), 1);
    assert_eq!(momentum_conviction_from_score(Some(20.0)), 2);
    assert_eq!(momentum_conviction_from_score(Some(35.0)), 3);
    assert_eq!(momentum_conviction_from_score(Some(55.0)), 4);
    assert_eq!(momentum_conviction_from_score(Some(80.0)), 5);
    assert_eq!(momentum_conviction_from_score(Some(100.0)), 5);
    assert_eq!(momentum_conviction_from_score(Some(-100.0)), -5);
}

#[test]
fn conviction_sign_always_agrees_with_the_decided_direction() {
    // The invariant the whole change buys: one source number, so one story. Sweep the scale.
    for tenths in -1000..=1000 {
        let s = f64::from(tenths) / 10.0;
        let dir = momentum_direction_from_score(Some(s));
        let conv = momentum_conviction_from_score(Some(s));
        match dir {
            "rising" => assert!((1..=5).contains(&conv), "rising got {conv} at score {s}"),
            "falling" => assert!((-5..=-1).contains(&conv), "falling got {conv} at score {s}"),
            _ => assert!((-1..=1).contains(&conv), "steady got {conv} at score {s}"),
        }
    }
}
#[test]
fn prompt_carries_the_decided_direction_line() {
    let mom = SynthMomentum {
        rating_slope: Some(50.7),
        rating_samples: 4,
        momentum_score: Some(50.7),
        ..SynthMomentum::default()
    };
    let prompt = build_momentum_prompt(
        "player",
        "Test Player",
        "FOOTBALL",
        None,
        None,
        &mom,
    );
    // s18: BOTH decided facts arrive as words — the direction line hands the model no
    // figure and no "steady band" to echo (the digit-starvation pass; 50.7 ⇒ conviction
    // 3 ⇒ "clean and well supported" via momentum_conviction_from_score).
    assert!(prompt.contains(
        "Direction (decided upstream, final): rising — strength of the move, also decided upstream: clean and well supported"
    ));
    let direction_line = prompt
        .lines()
        .find(|l| l.starts_with("Direction (decided upstream, final):"))
        .expect("direction line present");
    assert!(!direction_line.contains("steady band"));
    assert!(!crate::guards::has_ascii_digit(direction_line));
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
    );
    assert!(empty
        .contains("Direction (decided upstream, final): steady (no durable momentum snapshot)"));
}

/// s19's load-bearing test: NOTHING but the two rails reaches this prompt.
///
/// It replaces two tests that asserted the opposite — that the compiled storylines and the
/// relational memory card rendered as context. They did, and that was the defect: measured
/// across eight well-covered teams, the Analyst named a trajectory in 57% of reads while
/// touching the stat profile in 42%, the mood in 42%, the news in 42% and transfers in 28%.
/// She was narrating her inputs. So the inputs went, and this test keeps them gone.
#[test]
fn only_the_two_rails_reach_the_prompt() {
    let mom = SynthMomentum {
        rating_slope: Some(-33.7),
        rating_samples: 4,
        vibe_slope: Some(14.0),
        vibe_samples: 11,
        momentum_score: Some(-22.4),
        ..SynthMomentum::default()
    };
    let p = build_momentum_prompt(
        "team",
        "Test Team",
        "FOOTBALL",
        Some(&a_rating()),
        Some(&a_vibe()),
        &mom,
    );

    // Both rails, both levels, both directions — and every one of them in WORDS.
    assert!(p.contains("Form is: moving hard down, on a modest sample"));
    // ONE statement per rail: her own slope wins, so the Scout's label — measured on HIS window
    // and saying the opposite here — must not also appear.
    assert!(
        !p.contains("overall scores holding steady over recent games"),
        "her slope and the Scout's label must never both describe the form rail: {p}"
    );
    assert!(p.contains("Mood stands: warm"));
    assert!(p.contains("Mood is: drifting up, on a healthy sample"));

    // NOT ONE DIGIT in the whole prompt body above the entity line. s18 took the figure out of
    // the direction line and digits_in_read fell from 65% of generations; s19 removed the prose
    // around the remaining slopes, which promoted them to the most prominent thing left, and the
    // first probe came back with "a 14-point climb over 11 samples" — four digits, instant
    // rejection. The input must not shout what the output may not say.
    assert!(
        !crate::guards::has_ascii_digit(&p),
        "no figure may reach the Analyst's prompt: {p}"
    );

    // No peer PROSE, whatever the cards carry. These two strings are the bodies of
    // a_rating() and a_vibe(); if either reaches the prompt the seat can narrate it.
    assert!(
        !p.contains("Chances created have held their line"),
        "the Scout's brief must not reach the Analyst: {p}"
    );
    assert!(
        !p.contains("The room is warm after the cup run"),
        "the Influencer's felt read must not reach the Analyst: {p}"
    );
    assert!(!p.contains("Scouting read:"));
    assert!(!p.contains("Felt read:"));
    assert!(!p.contains("Profile distinctiveness"));

    // And no story rails at all — she is off the packet rail and reads no memory card.
    assert!(!p.contains("THE STORIES BEHIND THE MOVE"));
    assert!(!p.contains("RELATIONAL MEMORY"));

    // The decided fact stays last and adjacent to the reply cue.
    let dir = p.find("Direction (decided upstream, final)").unwrap();
    let cue = p.find("Write the Momentum read now").unwrap();
    assert!(dir < cue, "the decided fact stays final");
}


#[test]
fn input_components_are_stable_and_sorted() {
    let rating = SynthRating {
        body: "body".to_string(),
        notability: 88,
        rating_trajectory: "rising".to_string(),
        rating_trajectory_label: "Composite rising".to_string(),
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
            r#"{{"momentum_rating_samples":6,"momentum_rating_slope":1.2,"momentum_score":1.2,"momentum_vibe_samples":4,"momentum_vibe_slope":-0,"notability":88,"prompt_version":"{MOMENTUM_PROMPT_VERSION}","rating_trajectory":"rising","rating_trajectory_label":"Composite rising","vibe_sentiment":62}}"#
        )
    );
}

// ── PARTIAL SPREADS ─────────────────────────────────────────────────────────────────────
// The doctrine (Scott, 2026-08-15): "If the Analyst receives no info, it won't have an
// output, but gracefully skip. If it only has vibe instead of rating, then it will build an
// output on that. Work with what we have, don't fabricate, not having something to say is an
// acceptable answer."
//
// The seat ALREADY does exactly this, and that is the problem these tests fix: the behaviour
// rested entirely on `MomentumContext::empty()` being `&&` rather than `||`, and nothing
// asserted it. Every one of the ten momentum fixtures carries BOTH rails, so flipping that
// operator would have deleted the vibe-only read — the whole "build an output on that" half
// of the brief — while the gate stayed green. A rule measured by nothing is advice (or8).
//
// This is the NORMAL path, not an edge case: the DB grows by fetch-and-upsert with no
// bootstrap, so entities arrive with nothing and fill in over weeks.

fn ctx(rating: Option<SynthRating>, vibe: Option<SynthVibe>, snap: SynthMomentum) -> MomentumContext {
    MomentumContext {
        season: 2025,
        rating,
        vibe,
        snapshot: snap,
        input_components_json: String::new(),
        input_hash: String::new(),
    }
}

fn a_rating() -> SynthRating {
    SynthRating {
        body: "Chances created have held their line.".to_string(),
        notability: 71,
        rating_trajectory: "steady".to_string(),
        rating_trajectory_label: "overall scores holding steady over recent games".to_string(),
    }
}

fn a_vibe() -> SynthVibe {
    SynthVibe {
        sentiment: 64,
        prompt: "The room is warm after the cup run.".to_string(),
    }
}

#[test]
fn only_a_totally_empty_context_is_empty_the_load_bearing_and() {
    // All three absent — the graceful-skip case. The handler returns Ok(()) and writes no
    // row: silence is a valid output, not a failure.
    assert!(ctx(None, None, SynthMomentum::default()).empty());

    // ...and every PARTIAL spread is NOT empty, so the seat proceeds and reads on whatever
    // survived. These four are the assertions that pin `&&`: under `||` all of them flip to
    // `empty()` == true and the seat would fall silent on entities it can genuinely read.
    assert!(
        !ctx(None, Some(a_vibe()), SynthMomentum::default()).empty(),
        "vibe with no rating must still produce a read — the brief's explicit case"
    );
    assert!(
        !ctx(Some(a_rating()), None, SynthMomentum::default()).empty(),
        "rating with no vibe must still produce a read"
    );
    let snap_only = SynthMomentum {
        momentum_score: Some(1.4),
        ..SynthMomentum::default()
    };
    assert!(
        !ctx(None, None, snap_only).empty(),
        "a trajectory snapshot alone is material enough to read"
    );
    assert!(!ctx(Some(a_rating()), Some(a_vibe()), SynthMomentum::default()).empty());
}

#[test]
fn a_vibe_only_context_builds_a_prompt_that_claims_no_form() {
    // The second half of the brief: vibe-without-rating still builds a prompt, and it must NOT
    // hand the model a form/trajectory line it could narrate a direction from — the Ipswich
    // failure mode, one seat over.
    //
    // s19 INVERTS this test's first assertion. It used to require the felt read to reach the
    // prompt; the felt read is the Influencer's prose and is exactly what made the Analyst
    // narrate the mood instead of its direction. What survives from her card is the LEVEL.
    let p = build_momentum_prompt(
        "team",
        "Ipswich Town",
        "FOOTBALL",
        None,
        Some(&a_vibe()),
        &SynthMomentum::default(),
    );
    assert!(
        p.contains("Mood stands: warm"),
        "the surviving vibe card's LEVEL must reach the prompt, in words: {p}"
    );
    assert!(
        !p.contains("cup run"),
        "but never its prose — that is the Influencer's card: {p}"
    );
    assert!(
        !p.contains("holding steady over recent games"),
        "no rating card was supplied, so no trajectory label may appear: {p}"
    );
}

/// The blanket digit ban is retired; the precise bookkeeping check replaces it (2026-08-24).
///
/// The old rule rejected any ASCII digit anywhere in the READ — 1,221 drops in three days, and
/// the thing that permanently dead-lettered momentum player 367 at five attempts. It had no test
/// asserting the rejection, so nothing caught its removal. This is that test, for the rule that
/// replaced it: digits in open prose are ordinary sporting evidence, a parenthetical carrying a
/// digit is the analyst's desk notes.
#[test]
fn momentum_allows_digits_in_prose_but_never_a_bookkeeping_citation() {
    let read = |blurb: &str| MomentumParser.parse(&format!("READ: {blurb}"));

    // Ships now. Under the old rule every one of these burned a finished READ.
    for ok in [
        "Three wins in a row and the room believes again.",
        "3 wins in a row and the room believes again.",
        "A 14-point climb over 11 samples, and the shape is holding.",
    ] {
        let got = read(ok).expect("digits in prose never fail the card").expect("a reply");
        assert_eq!(got.blurb, ok);
    }

    // Still rejected: the desk notes pasted into a card.
    for bad in [
        "The slide is real (Mood: 30/100) and nobody is arguing.",
        "He is climbing (4th percentile) against a soft run.",
    ] {
        assert!(read(bad).is_err(), "bookkeeping citation must still fail: {bad}");
    }

    // A parenthetical WITHOUT a digit is ordinary prose, not a citation.
    let aside = "The slide is real (and nobody is arguing) this week.";
    assert_eq!(read(aside).unwrap().unwrap().blurb, aside);
}
