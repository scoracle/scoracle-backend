//! oracle_or4_fixtures — regenerate the hand-authored Oracle eval fixtures.
//!
//! or4 is the Characters Phase B voice pass (the LAST of the six): the system prompt now
//! speaks the Oracle as the sixth character at the table — five peers have published their
//! stories, and the Oracle's turn comes last (wiki/Characters.md craft appendix: measured,
//! knowing, quietly mystic; the reader at the table, never a narrator above the story; the
//! mysticism lives in the telling, every fact from the cards). The or3 CONTRACT —
//! `{reading, score}` JSON, 2-4 sentence reading, omen-is-final, no internal field words,
//! prior-read score discipline — is unchanged.
//!
//! The six scenario CONCEPTS carry over from the or2-era frozen set (ascendant-aligned,
//! ascendant-thin-hype, crossroads-conflict, steady-quiet, transfer-crossroads,
//! waning-freefall), re-authored as pillar structs and rendered through the REAL production
//! path — `build_pillar_divergence` → `pillar_convergence` → `compute_omen` →
//! `build_crown_prompt` — so the frozen prompts are byte-exact AND the omen each scenario
//! is named for is asserted against the computed one (a drifted omen rule fails the
//! generator, not silently the gate). prior_read/memory pin None: reproducible fixtures
//! measure the fresh-card contract, not the enrichment riders (the eval discipline).
//!
//!     cargo run --example oracle_or4_fixtures
//!     cargo run --bin eval -- --task oracle --fixtures   (needs Ollama)

use std::path::Path;

use scoracle_cognition::corpus::HeatItem;
use scoracle_cognition::junctions::oracle::{
    build_crown_prompt, build_pillar_divergence, compute_omen, pillar_convergence, SynthMomentum,
    SynthNarrative, SynthRating, SynthVibe, ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT,
};
use serde_json::json;

struct Scenario {
    name: &'static str,
    note: &'static str,
    entity: &'static str,
    entity_type: &'static str,
    sport: &'static str,
    expect_omen: &'static str, // asserted against compute_omen — self-checking fixtures
    narratives: Vec<SynthNarrative>,
    rating: Option<SynthRating>,
    vibe: Option<SynthVibe>,
    momentum: SynthMomentum,
    transfers: Vec<HeatItem>,
    expect: serde_json::Value,
}

fn narrative(
    title: &str,
    body: &str,
    impact: f64,
    trajectory: &str,
    sources: i32,
) -> SynthNarrative {
    SynthNarrative {
        title: title.to_string(),
        body: body.to_string(),
        impact,
        trajectory: trajectory.to_string(),
        source_count: sources,
        source_age_days: Some(1),
    }
}

fn rating(
    notability: i32,
    trajectory: &str,
    label: &str,
    body: &str,
) -> Option<SynthRating> {
    Some(SynthRating {
        body: body.to_string(),
        notability,
        rating_trajectory: trajectory.to_string(),
        rating_trajectory_label: label.to_string(),
    })
}

fn vibe(sentiment: i32, prompt: &str) -> Option<SynthVibe> {
    Some(SynthVibe {
        sentiment,
        prompt: prompt.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn momentum(
    direction: &str,
    score: f64,
    r_slope: Option<f64>,
    r_n: i32,
    v_slope: Option<f64>,
    v_n: i32,
    blurb: &str,
) -> SynthMomentum {
    SynthMomentum {
        direction: Some(direction.to_string()),
        blurb: Some(blurb.to_string()),
        momentum_score: Some(score),
        rating_slope: r_slope,
        rating_samples: r_n,
        vibe_slope: v_slope,
        vibe_samples: v_n,
        ..SynthMomentum::default()
    }
}

/// Spread-CONTEXTUAL exclusions only: the wrong omen words for this spread (the undrawn-omen
/// rule — `omen` is the drawn one; the OTHER directional words must not appear). The global
/// vocabulary bans (internal metric names, "(", "**", "the omen is") left the per-fixture
/// expects 08-19 — they are one `no_banned_phrases` invariant per reading now
/// (`guards::ORACLE_READING_BANS`), the same list `CrownParser` enforces in production.
fn base_excludes(omen: &str) -> Vec<&'static str> {
    let mut ex = Vec::new();
    for o in ["ascendant", "waning", "crossroads"] {
        if o != omen {
            ex.push(o);
        }
    }
    ex
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "ascendant-aligned",
            note: "every rail rising together — the cleanest ascendant spread. The reading should carry the rise with a steady hand: grounded in the cards, one image, no hype and no manufactured caveats.",
            entity: "Kiana Wells", entity_type: "player", sport: "NBA",
            expect_omen: "ascendant",
            narratives: vec![narrative(
                "Defensive anchor keeps rolling",
                "A month of dominant rim protection has the rotation built around her; the coverage is admiring and consistent.",
                6.0, "heating_up", 3,
            )],
            rating: rating(85, "rising", "Composite and PEAK z-scores trending up over recent games",
                "Anchor defender erasing the paint; blocks and altered shots keep climbing."),
            vibe: vibe(70, "The room is warm and getting warmer — a run of dominant defensive games has the coverage glowing."),
            momentum: momentum("rising", 3.4, Some(1.2), 6, Some(0.8), 5,
                "Form and feeling are moving together; the rise is backed on both rails."),
            transfers: vec![],
            expect: json!({"reading_includes": ["Wells"],
                           "reading_excludes": base_excludes("ascendant"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        Scenario {
            name: "ascendant-thin-hype",
            note: "rising on feeling more than form: no PEAK card at all, warm vibe, modest momentum. Still ascendant by the computed omen — but the reading should keep the steady hand and not inflate thin evidence into a coronation.",
            entity: "Deni Kovac", entity_type: "player", sport: "FOOTBALL",
            expect_omen: "ascendant",
            narratives: vec![narrative(
                "Breakout buzz builds",
                "Two strong substitute appearances have the coverage buzzing about a bigger role.",
                4.0, "heating_up", 2,
            )],
            rating: None,
            vibe: vibe(80, "The buzz is loud and affectionate after two eye-catching cameos; most of it is projection."),
            momentum: momentum("rising", 1.4, None, 0, Some(1.8), 3,
                "Feeling is climbing fast on a thin sample; form has no card to show."),
            transfers: vec![],
            expect: json!({"reading_includes": ["Kovac"],
                           "reading_excludes": base_excludes("ascendant"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        Scenario {
            name: "crossroads-conflict",
            note: "the classic split: elite steady production against a souring room and sliding momentum. The reading must name THIS tension — which forces pull against each other — not generic fate talk.",
            entity: "Marcus Vale", entity_type: "player", sport: "NBA",
            expect_omen: "crossroads",
            narratives: vec![narrative(
                "Trade demand reported",
                "A trade-demand story broke this week and hangs over everything; the front office has not responded publicly.",
                8.0, "heating_up", 4,
            )],
            rating: rating(88, "steady", "Composite and PEAK z-scores steady over recent games",
                "Elite two-way production; the numbers have not blinked through the noise."),
            vibe: vibe(30, "The room has soured — the standoff dominates every thread and the patience is thinning."),
            momentum: momentum("falling", -1.8, Some(-0.2), 6, Some(-1.3), 5,
                "Feeling is dragging the arc down while the production holds its line."),
            transfers: vec![],
            expect: json!({"reading_includes": ["Vale"],
                           "reading_excludes": base_excludes("crossroads"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        Scenario {
            name: "steady-quiet",
            note: "a quiet spread: mid notability, mid vibe, steady momentum — nothing directional at all. A calm reading for a calm table; no manufactured drama, and the arc may be called steady (the omen IS steady).",
            entity: "Harbor City FC", entity_type: "team", sport: "FOOTBALL",
            expect_omen: "steady",
            narratives: vec![narrative(
                "Midtable rhythm holds",
                "Two draws and a narrow win; the coverage is routine match reports without a larger storyline.",
                3.0, "developing_story", 2,
            )],
            rating: rating(50, "steady", "Defensive numbers steady over recent matches",
                "Organized and hard to create against; little else stands out either way."),
            vibe: vibe(50, "Coverage is quiet and balanced — no surge of feeling in either direction."),
            momentum: momentum("steady", 0.3, Some(0.1), 5, Some(-0.1), 4,
                "Neither form nor feeling is moving; the line holds."),
            transfers: vec![],
            expect: json!({"reading_includes": ["Harbor City"],
                           "reading_excludes": base_excludes("steady"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        Scenario {
            name: "transfer-crossroads",
            note: "the Insider's wire carries the turn: advanced talks with a named counterparty while form dips and the room buzzes. Crossroads by the computed omen; the reading must name the counterparty exactly as the card does (Real Madrid) — a reading that could belong to another entity is no reading.",
            entity: "Rui Almeida", entity_type: "player", sport: "FOOTBALL",
            expect_omen: "crossroads",
            narratives: vec![narrative(
                "Madrid talks advance",
                "The move has progressed to personal terms according to multiple outlets; his club has begun scouting replacements.",
                7.0, "heating_up", 5,
            )],
            rating: rating(82, "steady", "Pressing numbers steady over recent matches",
                "Elite presser and the engine of the midfield; the profile has not moved."),
            vibe: vibe(68, "The room is buzzing at the link — half farewell tribute, half plea for him to stay."),
            momentum: momentum("falling", -1.2, Some(-0.9), 5, Some(0.4), 4,
                "Form has dipped since the story broke even as the feeling stays warm."),
            transfers: vec![HeatItem {
                counterparty: "Real Madrid".to_string(),
                heat: 78,
                stage: "advanced_talks".to_string(),
                direction: "outgoing".to_string(),
                summary: "Talks over a summer move have advanced to personal terms.".to_string(),
                confidence: Some(0.8),
            }],
            expect: json!({"reading_includes": ["Almeida", "Madrid"],
                           "reading_excludes": base_excludes("crossroads"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        Scenario {
            name: "waning-freefall",
            note: "every rail falling on healthy samples — the unambiguous waning spread. The reading should carry the decline with the same steady hand as a rise: no melodrama, grounded in the cards, and the score should sit low.",
            entity: "Coastal City FC", entity_type: "team", sport: "FOOTBALL",
            expect_omen: "waning",
            narratives: vec![narrative(
                "Winless month deepens",
                "Four matches without a win, fan protests outside the ground, and open pressure on the manager.",
                7.0, "heating_up", 4,
            )],
            rating: rating(30, "falling", "Attacking numbers trending down over recent matches",
                "The attack has dried up; chances created have fallen in each of the last five matches."),
            vibe: vibe(25, "The coverage is grim — protests, pressure, and a dressing room described as flat."),
            momentum: momentum("falling", -3.6, Some(-1.5), 6, Some(-1.1), 5,
                "Both rails point down and neither shows a floor yet."),
            transfers: vec![],
            expect: json!({"reading_includes": ["Coastal"],
                           "reading_excludes": base_excludes("waning"),
                           "reading_min_sentences": 2, "reading_max_sentences": 9}),
        },
        // ── PARTIAL SPREADS ──────────────────────────────────────────────────────────────
        // The doctrine (Scott, 2026-08-15): "if it only has 3 cards instead of 5, it will
        // build its output using those 3. Work with what we have, don't fabricate, not having
        // something to say is an acceptable answer."
        //
        // This is the NORMAL path, not an edge case: the DB grows by fetch-and-upsert with no
        // bootstrap, so every entity arrives empty and fills in. Ipswich Town had no stats at
        // all and its live or5 crown invented "the tension between the Vibe's intensity and
        // the PEAK's decline" — a direction read off a card that does not exist. or7's
        // allowance rule is what forbids that ("the cards that hold nothing no sentence at
        // all"), and until these two scenarios there was no fixture below four cards to hold
        // it to account.
        //
        // THE ASSERTION IS THE SENTENCE CEILING, and that is deliberate. A substring ban is
        // the wrong instrument here: the honest reading of a thin spread NAMES the absence,
        // so banning "form" would fail correct prose (ascendant-thin-hype's own momentum
        // blurb reads "form has no card to show"). What fabrication actually looks like is
        // PADDING — reaching eight sentences on a spread that holds three. So the ceiling
        // scales with the cards: fewer cards, fewer sentences available. That is the or7 rule
        // stated mechanically, and it is the same reasoning that gave reading_max_peers its
        // own check instead of an exclusion.
        Scenario {
            name: "three-card-newcomer",
            note: "THE PARTIAL SPREAD, three cards: a just-promoted side with news, a transfer wire and a felt read, but NO scouting brief and NO momentum — the shape every newly-arrived entity presents before its stats accumulate. The reading must build on the three cards it holds and stay silent on form and trajectory rather than infer them. Short is correct here; the ceiling is the assertion.",
            entity: "Ipswich Town", entity_type: "team", sport: "FOOTBALL",
            expect_omen: "steady",
            narratives: vec![narrative(
                "Promotion side reshapes its spine",
                "Two confirmed signings have arrived to rebuild the defensive line before the opener.",
                5.0, "developing_story", 3,
            )],
            rating: None,
            vibe: vibe(62, "Cautious optimism around the rebuild; the room likes the business but wants to see it on grass."),
            momentum: SynthMomentum::default(),
            transfers: vec![HeatItem {
                counterparty: "Fulham".to_string(),
                heat: 62,
                stage: "confirmed".to_string(),
                direction: "incoming".to_string(),
                summary: "The move for Issa Diop is done and the paperwork is filed.".to_string(),
                confidence: Some(0.9),
            }],
            expect: json!({"reading_includes": ["Ipswich"],
                           "reading_excludes": base_excludes("steady"),
                           "reading_min_sentences": 2, "reading_max_sentences": 5}),
        },
        Scenario {
            name: "near-empty-quiet-wire",
            note: "THE PARTIAL SPREAD, floor case: one card. No brief, no felt read, no momentum, no wire — a single quiet storyline is the whole spread. 'Not having something to say is an acceptable answer': two or three sentences is the complete reading, and anything longer is manufactured. This is the fixture that would have caught Ipswich's or5 fabrication.",
            entity: "Venezia", entity_type: "team", sport: "FOOTBALL",
            expect_omen: "steady",
            narratives: vec![narrative(
                "Squad reports back for preseason",
                "The squad returned for preseason testing this week; no further detail has surfaced.",
                2.0, "developing_story", 1,
            )],
            rating: None,
            vibe: None,
            momentum: SynthMomentum::default(),
            transfers: vec![],
            expect: json!({"reading_includes": ["Venezia"],
                           "reading_excludes": base_excludes("steady"),
                           "reading_min_sentences": 2, "reading_max_sentences": 3}),
        },
    ]
}

fn main() -> anyhow::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/oracle");
    std::fs::create_dir_all(&dir)?;
    let scenarios = scenarios();
    let n = scenarios.len();
    for s in scenarios {
        // The REAL deterministic pipeline: divergence → convergence → omen. The scenario's
        // named omen is asserted, so a drifted omen rule fails HERE, not silently at the gate.
        let comparisons = build_pillar_divergence(
            &s.narratives,
            s.rating.as_ref(),
            s.vibe.as_ref(),
            &s.momentum,
        );
        let convergence = pillar_convergence(&comparisons);
        let (omen, omen_reason) = compute_omen(convergence, &s.momentum);
        assert_eq!(
            omen, s.expect_omen,
            "scenario {}: authored pillars drew omen {omen}, expected {}",
            s.name, s.expect_omen
        );
        let prompt = build_crown_prompt(
            s.entity_type,
            s.entity,
            s.sport,
            &s.narratives,
            s.rating.as_ref(),
            s.vibe.as_ref(),
            &s.momentum,
            &s.transfers,
            omen,
            &omen_reason,
            // or9: the crown is blind to memories by CONTRACT now — the memory-free shape the
            // fixtures always pinned is simply the shape.
            None,
        );
        let v = json!({
            "name": s.name,
            "task": "oracle",
            "prompt_version": ORACLE_PROMPT_VERSION,
            "note": s.note,
            "omen": omen,
            "system": ORACLE_SYSTEM_PROMPT,
            "user_prompt": prompt,
            "temperature": 0.0,
            "expect": s.expect,
        });
        let path = dir.join(format!("{}.json", s.name));
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&v)?))?;
        println!(
            "wrote {} (omen={omen}, {} chars prompt)",
            path.display(),
            prompt.len()
        );
    }
    println!("done — {n} fixtures at {ORACLE_PROMPT_VERSION}");
    Ok(())
}
