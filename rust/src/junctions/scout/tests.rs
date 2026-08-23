//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

fn dp(label: &str, value: f64, z: f64, pct: f64, sign: i32) -> RatingDatapoint {
    RatingDatapoint {
        label: label.to_string(),
        value,
        z,
        pct,
        sign,
        ..Default::default()
    }
}

fn req(sport: &str, entity_type: &str, name: &str) -> RatingReq {
    RatingReq {
        entity_type: entity_type.to_string(),
        entity_id: 1,
        entity_name: name.to_string(),
        sport: sport.to_string(),
        season: None,
        trigger_type: "manual".to_string(),
    }
}

fn profile_player() -> RatingProfile {
    let mut scoring = dp("Scoring", 24.0, 3.1, 95.0, 1);
    scoring.scoped_pct.insert("position".to_string(), 88.0);
    RatingProfile {
        entity_type: "player".to_string(),
        season: 2025,
        position: "Guard".to_string(),
        composite_score: Some(67.0),
        breakdown: vec![scoring, dp("Defense", 2.5, -0.5, 40.0, 1)],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    }
}

fn faceted(label: &str, pct: f64, facet: &str) -> RatingDatapoint {
    let mut d = dp(label, 1.0, 0.0, pct, 1);
    d.facet = facet.to_string();
    d
}

fn nfl_profile(position: &str, breakdown: Vec<RatingDatapoint>) -> RatingProfile {
    RatingProfile {
        entity_type: "player".to_string(),
        season: 2025,
        position: position.to_string(),
        composite_score: Some(60.0),
        breakdown,
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    }
}

// --- off-facet filter: an offensive player's defensive stat sheet (and vice versa) must
// never reach the scouting decision, the prompt, or the input_hash. The Stafford/London
// bug: 0th-pct Tackling surfacing as a QB's "primary weakness to exploit".
// -----------------------------------------------------------------------------------------------

#[test]
fn off_facet_filter_drops_other_side_for_qb() {
    let mut p = nfl_profile(
        "QB",
        vec![
            faceted("Passing Yards", 90.0, "offense"),
            faceted("Tackling", 0.0, "defense"),
            faceted("Tackles For Loss", 0.0, "defense"),
            faceted("Giveaways", 20.0, "offense"),
        ],
    );
    p.rate_modes.insert(
        "per_game".to_string(),
        vec![
            faceted("Passing Yards", 91.0, "offense"),
            faceted("Tackling", 0.0, "defense"),
        ],
    );
    let dropped = drop_off_facet_datapoints(&mut p);
    assert_eq!(dropped, vec!["Tackling", "Tackles For Loss"]);
    let labels: Vec<&str> = p.breakdown.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, vec!["Passing Yards", "Giveaways"]);
    assert_eq!(p.rate_modes["per_game"].len(), 1);
}

#[test]
fn off_facet_filter_drops_offense_for_cornerback_spelled_out() {
    let mut p = nfl_profile(
        "Cornerback",
        vec![
            faceted("Interceptions", 85.0, "defense"),
            faceted("Receiving Yards", 2.0, "offense"),
        ],
    );
    let dropped = drop_off_facet_datapoints(&mut p);
    assert_eq!(dropped, vec!["Receiving Yards"]);
}

#[test]
fn off_facet_filter_fails_open_for_special_teams_and_unknown() {
    for pos in ["Punter", "KR", "Unknown", ""] {
        let mut p = nfl_profile(
            pos,
            vec![
                faceted("Field Goals", 70.0, "offense"),
                faceted("Tackling", 10.0, "defense"),
            ],
        );
        assert!(drop_off_facet_datapoints(&mut p).is_empty(), "pos={pos:?}");
        assert_eq!(p.breakdown.len(), 2, "pos={pos:?}");
    }
}

#[test]
fn off_facet_filter_noops_without_nfl_facets() {
    // NBA "Guard" collides with the NFL position name, but NBA breakdowns carry
    // facet="all" (or ""), which the filter never touches — the facet VALUE gates,
    // not the position alone.
    let mut p = nfl_profile(
        "Guard",
        vec![faceted("Scoring", 95.0, "all"), faceted("Steals", 20.0, "")],
    );
    assert!(drop_off_facet_datapoints(&mut p).is_empty());
    assert_eq!(p.breakdown.len(), 2);
}

// --- display-tier (retired) filter: metrics the engine retired from its equation
// (in_comp=false AND in_spec=false — the mig 060/062 display tier) must never reach the
// scouting decision, the prompt, the PEAK crown, or the input_hash pre-image. The Dan
// Burn bug: "PEAK: Clearances" — a metric mig 062 demoted for rewarding reactive volume.
// -----------------------------------------------------------------------------------------------

fn tiered(label: &str, pct: f64, in_comp: bool, in_spec: bool) -> RatingDatapoint {
    let mut d = dp(label, 10.0, 1.0, pct, 1);
    d.in_comp = in_comp;
    d.in_spec = in_spec;
    d
}

#[test]
fn display_tier_filter_drops_retired_metrics_from_breakdown_and_modes() {
    let mut p = nfl_profile(
        "",
        vec![
            tiered("Goalscoring", 70.0, true, true), // composite + in-spec: stays
            tiered("On-Court Impact", 60.0, true, false), // composite-only: stays
            tiered("Penalties Won", 88.0, false, true), // in-spec only: stays
            tiered("Clearances", 96.0, false, false), // display tier: dropped
            tiered("Duels", 90.0, false, false),     // display tier: dropped
        ],
    );
    p.rate_modes.insert(
        "per_90".to_string(),
        vec![
            tiered("Goalscoring", 71.0, true, true),
            tiered("Clearances", 95.0, false, false),
        ],
    );
    let dropped = drop_display_tier_datapoints(&mut p);
    assert_eq!(dropped, vec!["Clearances", "Duels"]);
    let labels: Vec<&str> = p.breakdown.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["Goalscoring", "On-Court Impact", "Penalties Won"]
    );
    assert_eq!(p.rate_modes["per_90"].len(), 1);
    assert_eq!(p.rate_modes["per_90"][0].label, "Goalscoring");
}

#[test]
fn retired_metric_never_reaches_prompt_preimage_or_crown() {
    // The Session D golden: Dan Burn-shaped profile — a display-tier metric holds the top
    // percentile. After the filter, the crown, the built prompt, and the input_components
    // hash pre-image must all be free of it, and the crown falls to the best REAL signal.
    let mut p = nfl_profile(
        "",
        vec![
            tiered("Clearances", 96.0, false, false),
            tiered("Duels", 90.0, false, false),
            tiered("Interceptions", 80.0, true, true),
            tiered("Tackling", 62.0, true, true),
        ],
    );
    p.rate_modes.insert(
        "per_game".to_string(),
        vec![
            tiered("Clearances", 94.0, false, false),
            tiered("Interceptions", 82.0, true, true),
        ],
    );
    drop_display_tier_datapoints(&mut p);

    let decision = build_scouting_decision(&p);
    assert_eq!(
        decision
            .primary_strength_to_stop
            .as_ref()
            .map(|f| f.label.as_str()),
        Some("Interceptions")
    );

    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Test Defender"),
        &p,
        60,
        None,
        None,
        None,
        None,
    );
    for retired in ["Clearances", "Duels"] {
        assert!(
            !prompt.contains(retired),
            "retired metric {retired:?} leaked into the built prompt"
        );
    }
    assert!(prompt.contains("Interceptions"));

    let ic = input_components(&p);
    for retired in ["Clearances", "Duels"] {
        assert!(
            !ic.contains(retired),
            "retired metric {retired:?} leaked into the input_components pre-image"
        );
    }
}

#[test]
fn degenerate_zero_datapoints_drop_but_real_absences_stay() {
    // London-shaped: 0 ground yards at z -0.28 is a usage artifact -> dropped. A zero with
    // a strongly negative z (a starter with no touchdowns) is a real finding -> kept.
    let mut artifact = faceted("Ground Yards Responsible", 1.0, "offense");
    artifact.value = 0.0;
    artifact.z = -0.28;
    let mut real_absence = faceted("Touchdowns", 2.0, "offense");
    real_absence.value = 0.0;
    real_absence.z = -2.1;
    let mut p = nfl_profile("WR", vec![artifact, real_absence]);
    let dropped = drop_degenerate_zero_datapoints(&mut p);
    assert_eq!(dropped, vec!["Ground Yards Responsible"]);
    assert_eq!(p.breakdown.len(), 1);
    assert_eq!(p.breakdown[0].label, "Touchdowns");
}

#[test]
fn weakness_requires_material_z_not_just_percentile() {
    // London's giveaways: 5th pct but sign-adjusted z only -0.2 -> NOT a weakness.
    let mut clumped = faceted("Giveaways", 5.3, "offense");
    clumped.value = 1.0;
    clumped.z = 0.199;
    clumped.sign = -1;
    assert!(!is_weakness(&clumped));
    // Stafford's giveaways: 0th pct at sign-adjusted z -4.9 -> emphatically a weakness.
    let mut real = faceted("Giveaways", 0.0, "offense");
    real.value = 12.0;
    real.z = 4.9023;
    real.sign = -1;
    assert!(is_weakness(&real));
    // With only artifact-grade negatives, the decision names NO weakness.
    let mut strong = faceted("Air Yards Responsible", 92.9, "offense");
    strong.value = 919.0;
    strong.z = 1.0;
    let p = nfl_profile("WR", vec![strong, clumped]);
    let d = build_scouting_decision(&p);
    assert_eq!(d.primary_weakness_to_exploit, None);
}

#[test]
fn scouting_decision_weakness_is_positional_after_filter() {
    let mut giveaways = faceted("Giveaways", 20.0, "offense");
    giveaways.value = 12.0;
    giveaways.z = 2.0; // sign-adjusted -2.0: materially bad, a real weakness
    giveaways.sign = -1;
    let mut p = nfl_profile(
        "QB",
        vec![
            faceted("Passing Yards", 90.0, "offense"),
            faceted("Tackling", 0.0, "defense"),
            giveaways,
        ],
    );
    drop_off_facet_datapoints(&mut p);
    let d = build_scouting_decision(&p);
    // Without the filter the 0th-pct Tackling wins min-by-pct; with it, the weakness is
    // the worst stat the player actually plays.
    assert_eq!(
        d.primary_weakness_to_exploit
            .as_ref()
            .map(|f| f.label.as_str()),
        Some("Giveaways")
    );
}

// --- build_stat_prompt byte-fixtures: the deterministic parity axis. The expected strings are
// computed by hand from the Rust assembly, so prompt drift fails here (offline, no model).
// -----------------------------------------------------------------------------------------------

#[test]
fn prompt_player_composite_datapoints_and_scoped_position() {
    let p = profile_player();
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        prompt,
        "Entity: Test Player (NBA player, Guard)\n\
\nProfile distinctiveness: 70/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nOverall score (how WELL overall — T-score, 50 = average): 67\n\
\nDECISION CARD\n\
Headline strength: Scoring: 24 · 95th pct (elite) · rating +3.1 [position: 88th, strong]\n\
Secondary strengths: None supplied.\n\
Headline limitation: Defense: 2.5 · 40th pct (below average) · rating -0.5\n\
\nDatapoints — value · percentile + TIER (the tier is the truth) · rating (how far above or below the average; a higher rating is a rarer edge); [position] percentile when present:\n\
- Scoring: 24 · 95th pct (elite) · rating +3.1 [position: 88th, strong]\n\
- Defense: 2.5 · 40th pct (below average) · rating -0.5\n\
\nWrite the report now: four labelled lines (Strengths / Limitations / Summary / HEADLINE), each on its own line, plain text, no Markdown. Begin directly with the word \"Strengths:\" — no preamble, nothing before it."
    );
}

#[test]
fn prompt_team_no_composite_no_position() {
    // Team: position "" (no ", Guard" in the header), no composite line, one datapoint.
    let p = RatingProfile {
        entity_type: "team".to_string(),
        season: 2025,
        position: String::new(),
        composite_score: None,
        breakdown: vec![dp("Defense", 0.38, 1.2, 78.0, 1)],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    };
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "team", "Test FC"),
        &p,
        55,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        prompt,
        "Entity: Test FC (FOOTBALL team)\n\
\nProfile distinctiveness: 55/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nDECISION CARD\n\
Headline strength: Defense: 0.38 · 78th pct (strong) · rating +1.2\n\
Secondary strengths: None supplied.\n\
Headline limitation: None — this profile offers no clean exploit.\n\
\nDatapoints — value · percentile + TIER (the tier is the truth) · rating (how far above or below the average; a higher rating is a rarer edge); [position] percentile when present:\n\
- Defense: 0.38 · 78th pct (strong) · rating +1.2\n\
\nWrite the report now: four labelled lines (Strengths / Limitations / Summary / HEADLINE), each on its own line, plain text, no Markdown. Begin directly with the word \"Strengths:\" — no preamble, nothing before it."
    );
}

#[test]
fn cross_season_memory_renders_before_the_write_cue() {
    // The s12 memory card renders as the last content section, bulleted, with the
    // tier-truth guard in the header; None pins the s11 byte shape (the byte-fixtures
    // above). Blank memory ⇒ no section.
    let p = profile_player();
    // The mem string is what mig 221's stat_context_for_entity renders — the divined label
    // retired with PEAK, so the prior read carries distinctiveness and the trajectory.
    let mem = "Our prior read: season 2025 scored this profile 98/100 for distinctiveness.\nMatchup memory: pts vs Test Rivals — 22.8/game vs a 16.0 baseline (adjusted +4.7), n=13 games, reliability 44/100.";
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        Some(mem),
        None,
        None,
        None,
    );
    assert!(prompt.contains("\nCross-season memory (computed history — arc context only"));
    assert!(prompt.contains("- Our prior read: season 2025 scored this profile 98/100"));
    assert!(prompt.contains("- Matchup memory: pts vs Test Rivals"));
    let mem_pos = prompt.find("Cross-season memory").unwrap();
    let cue_pos = prompt.find("Write the report now").unwrap();
    let dp_pos = prompt.find("Datapoints — value").unwrap();
    assert!(dp_pos < mem_pos && mem_pos < cue_pos);
    let blank = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        Some(" \n "),
        None,
        None,
        None,
    );
    assert!(!blank.contains("Cross-season memory"));
}

#[test]
fn scouting_decision_requires_no_standout_when_top_is_only_above_average() {
    let p = RatingProfile {
        entity_type: "player".to_string(),
        season: 2025,
        position: "SG".to_string(),
        composite_score: Some(49.0),
        breakdown: vec![
            dp("Spot-up shooting", 38.1, 0.5, 64.0, 1),
            dp("Turnovers", 3.7, -1.4, 23.0, 1),
        ],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    };
    let d = build_scouting_decision(&p);
    // s19: no divined label — no-standout is asserted structurally below.
    assert!(d.primary_strength_to_stop.is_none());
    assert_eq!(
        d.primary_weakness_to_exploit
            .as_ref()
            .map(|f| f.label.as_str()),
        Some("Turnovers")
    );
    assert!(d
        .no_standout_reason
        .as_deref()
        .is_some_and(|r| r.contains("Spot-up shooting") && r.contains("above average")));
}

// --- input_components canonical JSON: the input_hash pre-image (must match Go json.Marshal). -----

#[test]
fn input_components_matches_go_marshal_bytes() {
    // Datapoints walk the breakdown in STORED order (NOT pct-sorted): Scoring then Defense.
    // Top keys sorted: composite_score, datapoints, position, prompt_version, season
    // (s19: the specialist entries peak_label/peak_score left the pre-image with the concept). Datapoint keys sorted: label, pct. composite round1(67.0)=67 → "67"; pct round1
    // → "95"/"40". prompt_version is interpolated from the const (single-sourced: an s-bump
    // changes the pre-image by design and must not need a hand-edit here).
    let ic = input_components(&profile_player());
    assert_eq!(
        ic,
        format!(
            r#"{{"composite_score":67,"datapoints":[{{"label":"Scoring","pct":95}},{{"label":"Defense","pct":40}}],"position":"Guard","prompt_version":"{RATING_PROMPT_VERSION}","season":2025}}"#
        )
    );
    // The hash is a deterministic function of those exact bytes.
    assert_eq!(hash_components(&ic), hash_components(&ic));
}

#[test]
fn input_components_omits_absent_optional_keys() {
    // No composite, no position, no rate modes → only season/datapoints (+ version).
    let p = RatingProfile {
        entity_type: "team".to_string(),
        season: 2024,
        position: String::new(),
        composite_score: None,
        breakdown: vec![],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    };
    // Empty breakdown → "datapoints":[] (a non-nil empty slice marshals as []).
    assert_eq!(
        input_components(&p),
        format!(r#"{{"datapoints":[],"prompt_version":"{RATING_PROMPT_VERSION}","season":2024}}"#)
    );
}

#[test]
fn rating_datapoint_tolerates_null_like_go() {
    // A sparse datapoint with an explicit null value + a null scoped entry — Go's json.Unmarshal
    // keeps zero values; serde must too (the L12 gate caught this: player:268's "Penalties Won"
    // carries "value": null, which plain #[serde(default)] would reject).
    let d: RatingDatapoint = serde_json::from_str(
        r#"{"label":"Penalties Won","value":null,"z":0.0,"pct":12.4,"scoped_pct":{"position":11.6,"x":null}}"#,
    )
    .expect("null tolerated like Go");
    assert_eq!(d.value, 0.0);
    assert_eq!(d.pct, 12.4);
    assert_eq!(d.scoped_pct.get("position"), Some(&11.6));
    assert_eq!(d.scoped_pct.get("x"), Some(&0.0)); // null map value → 0.0 (Go parity)
}

// --- deterministic helpers ----------------------------------------------------------------------

#[test]
fn pct_band_boundaries() {
    assert_eq!(pct_band(90.0), "elite");
    assert_eq!(pct_band(89.9), "strong");
    assert_eq!(pct_band(75.0), "strong");
    assert_eq!(pct_band(60.0), "above average");
    assert_eq!(pct_band(50.0), "average");
    assert_eq!(pct_band(49.9), "below average");
    assert_eq!(pct_band(35.0), "below average");
    assert_eq!(pct_band(34.9), "poor");
    assert_eq!(pct_band(0.0), "poor");
}

#[test]
fn trim_float_formats_like_go() {
    assert_eq!(trim_float(3.0), "3"); // integral → %.0f
    assert_eq!(trim_float(24.0), "24");
    assert_eq!(trim_float(0.38), "0.38"); // abs < 1 → %.2f
    assert_eq!(trim_float(0.4), "0.40");
    assert_eq!(trim_float(10.7), "10.7"); // else → %.1f
    assert_eq!(trim_float(2.5), "2.5");
    assert_eq!(trim_float(-3.0), "-3");
}

#[test]
fn rating_trajectory_buckets_and_labels_recent_form() {
    assert_eq!(trajectory_key(linear_slope(&[0.1, 0.5, 1.0])), "rising");
    assert_eq!(trajectory_key(linear_slope(&[2.2, 1.7, 1.1])), "falling");
    assert_eq!(trajectory_key(linear_slope(&[0.7, 0.8, 0.75])), "steady");
    assert_eq!(
        z_trajectory_label("falling"),
        "overall scores trending down over recent games"
    );
}

#[test]
fn compute_notability_known_case() {
    // top_pct 95, elite_count 1 (only Scoring ≥ 85), comp 67.
    // score = 0.6*95 + min(30, 10) + clamp(-10,10,(67-50)*0.4=6.8) = 57 + 10 + 6.8 = 73.8 → 74.
    let (n, comps) = compute_notability(&profile_player());
    assert_eq!(n, 74);
    assert_eq!(comps["top_pct"], 95.0);
    assert_eq!(comps["elite_count"], 1);
    assert_eq!(comps["composite"], 67.0);
}

#[test]
fn ordered_facts_sorts_desc_and_truncates() {
    let p = RatingProfile {
        entity_type: "player".to_string(),
        season: 2025,
        position: String::new(),
        composite_score: None,
        breakdown: vec![
            dp("low", 1.0, 0.0, 20.0, 1),
            dp("high", 1.0, 0.0, 90.0, 1),
            dp("mid", 1.0, 0.0, 55.0, 1),
        ],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    };
    let ordered = ordered_facts(&p.breakdown);
    assert_eq!(
        ordered.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(),
        vec!["high", "mid", "low"]
    );
}

// --- output parsing (marker strip is transition tolerance since s18/s19) ------------------------

#[test]
fn parse_rating_commentary_splits_marker_and_body() {
    let (peak, body) = parse_rating_commentary("PEAK: Elite scoring\nA lethal scorer who...");
    assert_eq!(peak, "Elite scoring");
    assert_eq!(body, "A lethal scorer who...");
}

#[test]
fn parse_rating_commentary_accepts_legacy_sigil_prefix() {
    let (peak, body) = parse_rating_commentary("SIGIL: Rim protection\nDominant inside.");
    assert_eq!(peak, "Rim protection");
    assert_eq!(body, "Dominant inside.");
}

#[test]
fn parse_rating_commentary_no_marker_is_all_body() {
    let (peak, body) = parse_rating_commentary("Just prose, no marker line\nsecond line");
    assert_eq!(peak, "");
    assert_eq!(body, "Just prose, no marker line\nsecond line");
}

#[test]
fn parse_rating_commentary_salvages_inline_peak_paragraph_as_body() {
    let raw = "PEAK: Nia Torres showcases elite rim protection at the 96th percentile and anchors the paint.";
    let (peak, body) = parse_rating_commentary(raw);
    assert_eq!(peak, "");
    assert_eq!(
        body,
        "Nia Torres showcases elite rim protection at the 96th percentile and anchors the paint."
    );
}

#[test]
fn clean_commentary_strips_fences_and_labels() {
    assert_eq!(
        clean_commentary("`Analysis: A solid two-way wing.`"),
        "A solid two-way wing."
    );
    assert_eq!(clean_commentary("  Identity: A poacher.  "), "A poacher.");
    assert_eq!(clean_commentary("Plain prose."), "Plain prose.");
}

#[test]
fn rating_work_token_carries_contract_and_still_parses_season() {
    // The s11 token format, renamed by mig 221: rating:s<season>:<prompt_version>:<hash|no-stats>.
    // The prompt-version leg is what reopens a done queue row on a persona change; the
    // season parse must survive the extra segment.
    let v = rating_work_input_version(2025, Some("abc123"));
    assert_eq!(v, format!("rating:s2025:{RATING_PROMPT_VERSION}:abc123"));
    assert_eq!(rating_work_season(Some(&v)), Some(2025));
    assert_eq!(
        rating_work_input_version(2025, None),
        format!("rating:s2025:{RATING_PROMPT_VERSION}:no-stats")
    );
    // Short-form tokens (the migration rewrote every stored prefix) parse their season too.
    assert_eq!(rating_work_season(Some("rating:s2024:deadbeef")), Some(2024));
}

#[test]
fn a_transfer_triggered_rating_token_is_distinguishable_and_still_parses_season() {
    // Scott's brief, 2026-08-15: the Scout has to know when a transfer crossed the threshold
    // and became concrete. The application id rides in the hash slot, which does two jobs at
    // once — it makes each applied move its OWN input_version (so work::enqueue reopens a done
    // row instead of collapsing into it), and it marks the item so RatingHandler turns the
    // skip_unchanged debounce off. The stats have not moved, so without that second half the
    // reopened row would short-circuit before the model call and the brief would still describe
    // a squad that no longer exists.
    let v = rating_work_input_version_for_transfer(2025, 4211);
    assert_eq!(v, format!("rating:s2025:{RATING_PROMPT_VERSION}:xfer4211"));

    // The season parse must survive the marker — the handler reads the season from this token.
    assert_eq!(rating_work_season(Some(&v)), Some(2025));
    assert!(rating_work_is_transfer_triggered(Some(&v)));

    // ...and an ordinary stats-driven token must NOT be mistaken for one, or every periodic
    // rating in the fleet would bypass the debounce and regenerate on every enumeration.
    let stats = rating_work_input_version(2025, Some("abc123"));
    assert!(!rating_work_is_transfer_triggered(Some(&stats)));
    assert!(!rating_work_is_transfer_triggered(Some(
        &rating_work_input_version(2025, None)
    )));
    assert!(!rating_work_is_transfer_triggered(Some("rating:s2024:deadbeef")));
    assert!(!rating_work_is_transfer_triggered(None));
}

#[test]
fn rating_parser_never_fails_closed() {
    // Even garbage parses to Some (rating's only marker is pre-model); an empty body is the
    // caller's hard error, not a served UNKNOWN.
    let reply = RatingParser
        .parse("PEAK: X\nbody")
        .unwrap()
        .expect("always Some");
    // s19: a legacy marker line is stripped and DISCARDED; only the body survives.
    assert_eq!(reply.body, "body");
    assert!(RatingParser.parse("").unwrap().is_some());
}

#[test]
fn rating_splits_the_s20_headline_line() {
    // s20 (mig 226): the contracted closing title line — lifted out of the body, folded.
    let reply = RatingParser
        .parse("Strengths: Rim protection at the 96th percentile.\nSummary: Take away the rim first.\nHEADLINE:   Take away the rim against Vale ")
        .unwrap()
        .expect("always Some");
    assert_eq!(
        reply.headline.as_deref(),
        Some("Take away the rim against Vale")
    );
    assert!(!reply.body.contains("HEADLINE"));
    assert!(reply.body.contains("Rim protection"));

    // Absent line → None; body untouched.
    let bare = RatingParser.parse("Summary: The verdict stands.").unwrap().unwrap();
    assert!(bare.headline.is_none());
    assert_eq!(bare.body, "Summary: The verdict stands.");

    // Empty title folds to None, never an error.
    let empty = RatingParser.parse("Summary: x.\nHEADLINE: ").unwrap().unwrap();
    assert!(empty.headline.is_none());

    // A hook-contract violation FAILS OPEN (2026-08-22): an over-long title with no beat
    // separator cannot be salvaged, so it degrades to no title and the report still ships.
    // It used to return Err and discard a complete graded profile over its own title.
    let long = RatingParser
        .parse("Summary: x.\nHEADLINE: one two three four five six seven eight nine ten eleven twelve thirteen")
        .expect("a junk title never fails the report")
        .expect("a reply");
    assert!(long.headline.is_none(), "unsalvageable title drops: {:?}", long.headline);
    assert_eq!(long.body, "Summary: x.");
}

// --- 7.7 the personnel block: the Scout's second confirmed-fact road ------------------
// T4 holds by construction here — every field the renderer reads is a date, a resolved name, or
// the adjudicated `event_type` enum. There is no path by which news prose reaches this seat.

fn change(
    kind: &str,
    date: &str,
    player: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> PersonnelChange {
    PersonnelChange {
        kind: kind.to_string(),
        date_label: date.to_string(),
        event_type: Some("transfer".to_string()),
        player_name: player.to_string(),
        old_team: old.map(|s| s.to_string()),
        new_team: new.map(|s| s.to_string()),
        old_team_id: old.map(|_| 277),
        new_team_id: new.map(|_| 3468),
    }
}

#[test]
fn a_player_move_names_both_clubs_and_the_date() {
    let out = render_personnel_block(
        "player",
        37922937,
        &[change(
            "applied",
            "Jul 29",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .expect("a move renders");
    assert_eq!(out, "- Jul 29: joined New FC from Old FC (transfer).\n");
}

/// The club a player came FROM is exactly what `transfer_ground_truth` drops (it selects
/// `new_team_id` only), so a missing old club must still render a clean fact, never "from None".
#[test]
fn a_player_move_without_a_known_old_club_still_renders() {
    let out = render_personnel_block(
        "player",
        1,
        &[change(
            "applied",
            "Jul 16",
            "Test Player",
            None,
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    assert_eq!(out, "- Jul 16: joined New FC (transfer).\n");
}

/// A team read must see BOTH directions. The ground-truth view matches `new_team_id` only, so a
/// club losing a player sees nothing there — this is the half 7.7 exists to add. The side is
/// decided by id, never by comparing club names.
#[test]
fn a_team_sees_arrivals_and_departures_decided_by_id() {
    let arrival = render_personnel_block(
        "team",
        3468,
        &[change(
            "applied",
            "Jul 29",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        arrival,
        "- Jul 29: signed Test Player from Old FC (transfer).\n"
    );

    // Same row, read by the OTHER club: a departure.
    let departure = render_personnel_block(
        "team",
        277,
        &[change(
            "applied",
            "Jul 29",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        departure,
        "- Jul 29: lost Test Player to New FC (transfer).\n"
    );
}

/// A revert is the fact the ground-truth view can never carry (it filters `reverted_at IS NULL`),
/// and it is the one the Scout most needs: the last brief may have been written around a move
/// that has since been undone. It must never render as a move.
#[test]
fn a_revert_renders_as_a_correction_not_a_move() {
    let player = render_personnel_block(
        "player",
        1,
        &[change(
            "reverted",
            "Aug 02",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        player,
        "- Aug 02: earlier move to New FC REVERTED — that move is not in force (transfer).\n"
    );
    assert!(!player.contains("joined"));

    let team = render_personnel_block(
        "team",
        3468,
        &[change(
            "reverted",
            "Aug 02",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    assert!(team.contains("Test Player's move REVERTED"));
    assert!(!team.contains("signed") && !team.contains("lost"));
}

/// The A5 rule: what the cap drops is NAMED. A deadline-day squad churn must not crowd out the
/// datapoints inside a 4,096 window, and it must not silently pretend six changes were all of them.
#[test]
fn the_cap_names_what_it_dropped() {
    let rows: Vec<PersonnelChange> = (0..MAX_PERSONNEL_LINES)
        .map(|i| {
            change(
                "applied",
                "Jul 29",
                &format!("Player {i}"),
                Some("Old FC"),
                Some("New FC"),
            )
        })
        .collect();
    let out = render_personnel_block("team", 3468, &rows, MAX_PERSONNEL_LINES + 4).unwrap();
    assert_eq!(out.lines().count(), MAX_PERSONNEL_LINES + 1);
    assert!(out.ends_with("- (+4 older personnel changes in this window, not shown)\n"));

    // Nothing dropped ⇒ no drop line at all.
    let exact = render_personnel_block("team", 3468, &rows, MAX_PERSONNEL_LINES).unwrap();
    assert_eq!(exact.lines().count(), MAX_PERSONNEL_LINES);
    assert!(!exact.contains("not shown"));
}

/// No changes ⇒ no section. A heading with nothing under it asserts "nothing moved", which is a
/// claim the adjudication chain has not made — it may only mean nothing has been adjudicated yet.
#[test]
fn nothing_moved_renders_no_section_at_all() {
    assert!(render_personnel_block("player", 1, &[], 0).is_none());
    let p = profile_player();
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        None,
        None,
        None,
        None,
    );
    assert!(!prompt.contains("Personnel changes"));
}

/// Placement: below the datapoints (a tier is still the truth about the player who holds it),
/// above the cross-season memory card (this is the squad now, not the arc), above the write cue.
#[test]
fn the_personnel_block_sits_between_the_datapoints_and_the_memory_card() {
    let p = profile_player();
    let personnel = render_personnel_block(
        "player",
        1,
        &[change(
            "applied",
            "Jul 29",
            "Test Player",
            Some("Old FC"),
            Some("New FC"),
        )],
        1,
    )
    .unwrap();
    let mem = "Our prior read: season 2025 PEAK was \"Shooting\" (notability 98/100).";
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        Some(mem),
        Some(&personnel),
        None,
        None,
    );
    let dp = prompt.find("Datapoints — value").unwrap();
    let pers = prompt
        .find("Personnel changes since our last read")
        .unwrap();
    let memp = prompt.find("Cross-season memory").unwrap();
    let cue = prompt.find("Write the report now").unwrap();
    assert!(dp < pers && pers < memp && memp < cue);
    assert!(prompt.contains("- Jul 29: joined New FC from Old FC (transfer).\n"));
    // The tier-truth invariant travels with the block.
    assert!(prompt.contains("these do NOT alter any tier or number above"));

    // Blank personnel ⇒ no section, same as blank memory.
    let blank = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        70,
        None,
        Some(" \n "),
        None,
        None,
    );
    assert!(!blank.contains("Personnel changes"));
}

/// s21: the datapoint block must SPAN the entity's range, not crowd its top.
#[test]
fn datapoints_span_the_range_rather_than_taking_the_top() {
    // Forty facets at descending percentiles, 97.5 down to 0.0.
    let breakdown: Vec<RatingDatapoint> = (0..40)
        .map(|i| RatingDatapoint {
            label: format!("Stat {i}"),
            pct: 97.5 - (i as f64) * 2.5,
            ..Default::default()
        })
        .collect();
    let got = ordered_facts(&breakdown);

    assert_eq!(got.len(), MAX_STAT_FACTS, "the 4,096-window budget still binds");
    // Presented high to low.
    for w in got.windows(2) {
        assert!(w[0].pct >= w[1].pct, "datapoints stay in pct DESC order");
    }
    // Both ends are present. Before s21 this list was facts[0..14] — everything at or below
    // the 62nd percentile was invisible, so the bottom assertion is the whole point.
    assert_eq!(got.first().unwrap().pct, 97.5, "the best skill is shown");
    assert_eq!(got.last().unwrap().pct, 0.0, "and so is the worst");
    // ...and the middle survives, which top-plus-bottom would also have missed.
    let middle = got
        .iter()
        .filter(|d| d.pct > 20.0 && d.pct < 75.0)
        .count();
    assert!(
        middle >= 3,
        "the interior of the distribution must be represented, got {middle}: {:?}",
        got.iter().map(|d| d.pct).collect::<Vec<_>>()
    );
}

/// A junk title never costs the report. Measured 2026-08-22: a live generation died on
/// `hook_colon (headline="Hornets: Elite shooter...")`, discarding a complete graded profile
/// over punctuation in its title — the only seat still failing closed on a title.
#[test]
fn a_bad_headline_never_throws_the_report_away() {
    let body = "Strengths: Blocked shots are elite at the 98th percentile.\nLimitations: Shots on target allowed sit at the 4th percentile.\nSummary: A spiky defensive profile.";

    // A colon title salvages to its first beat rather than killing the read.
    let salvaged = RatingParser
        .parse(&format!("{body}\nHEADLINE: Hornets — elite shooting, poor containment"))
        .expect("a two-beat title must not fail the report")
        .expect("a reply");
    assert!(salvaged.body.contains("Strengths:"), "the report survives");

    // An unsalvageable title degrades to no title, and the report still ships.
    let dropped = RatingParser
        .parse(&format!("{body}\nHEADLINE: Hornets: Elite shooter, poor containment inside"))
        .expect("an unsalvageable title must not fail the report")
        .expect("a reply");
    assert!(dropped.body.contains("Limitations:"), "the report survives: {:?}", dropped.body);
    assert!(
        dropped.headline.as_deref().is_none_or(|h| crate::guards::hook_violation(h).is_none()),
        "a shipped title always satisfies the contract: {:?}", dropped.headline
    );
}
