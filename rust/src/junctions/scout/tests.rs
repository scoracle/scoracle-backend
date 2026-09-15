//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use crate::evidence::personnel::{MAX_AVAILABILITY_LINES, MAX_PERSONNEL_LINES};

use super::*;

#[test]
fn unranked_one_appearance_is_not_an_elite_or_declining_profile() {
    let mut current = nfl_profile("Midfielder", vec![]);
    current.season = 2026;
    current.composite_score = None;
    current.observed_at = Some("2026-09-07".into());
    current.sample.insert("Appearances".into(), 1.0);
    current.breakdown = serde_json::from_value(serde_json::json!([
        {"label":"Chance Creation","measure":"expected assists","value":1.01,
         "z":4.5385,"pct":null,"in_comp":true,"in_spec":true,"sign":1},
        {"label":"Shooting","measure":"expected goals","value":0.14,
         "z":-0.3137,"pct":null,"in_comp":true,"in_spec":true,"sign":1},
        {"label":"Goalscoring","measure":"goals","value":0,
         "z":0,"pct":null,"in_comp":true,"in_spec":true,"sign":1}
    ]))
    .unwrap();
    // A real old rank is not comparable to an unranked current observation,
    // even if the skill and measurement have not changed.
    let mut prior = current.clone();
    prior.season = 2025;
    prior.sample.insert("Appearances".into(), 37.0);
    prior.breakdown[0].pct = Some(95.4);
    let changes = build_skill_changes(&current, &prior);
    assert!(changes.is_empty());
    assert!(drop_degenerate_zero_datapoints(&mut current).is_empty());
    current
        .rate_modes
        .insert("per_90".into(), current.breakdown.clone());
    assert!(collect_rate_standouts(&current).is_empty());
    let decision = build_scouting_decision(&current);
    assert!(decision.primary_strength_to_stop.is_none());
    assert!(decision.primary_weakness_to_exploit.is_none());
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Morgan Rogers"),
        &current,
        None,
        Some(&changes),
        None,
        None,
        None,
    );
    assert!(prompt.contains(
        "Stats updated: 2026-09-07; sample: stored thin sample; participation details omitted"
    ));
    assert!(!prompt.contains("sample: Appearances 1"));
    assert!(prompt.contains("Chance Creation: 1.01 (expected assists)"));
    assert!(prompt.contains("Shooting: 0.14 (expected goals)"));
    assert!(prompt.contains("Goalscoring: 0 (goals)"));
    assert!(!prompt.contains("95.4"));
    assert!(!prompt.contains("(elite)"));
    assert!(!prompt.contains("prior season percentile"));
    assert!(!prompt.contains("Prior reading"));
}

#[test]
fn missing_numeric_evidence_is_not_a_measured_zero_or_bottom_rank() {
    let absent: RatingDatapoint =
        serde_json::from_str(r#"{"label":"Scoring","value":null,"z":null,"pct":null}"#).unwrap();
    assert_eq!(absent.value, None);
    assert_eq!(absent.pct, None);
    assert_eq!(signed_z(&absent), None);
    assert_eq!(format_datapoint_evidence(&absent), "Scoring: unmeasured");
    assert!(!is_weakness(&absent));
    let zero = dp("Scoring", 0.0, -2.0, 0.0, 1);
    assert_eq!(zero.value, Some(0.0));
    assert!(format_datapoint_evidence(&zero).contains("percentile 0.0"));
    assert!(is_weakness(&zero));
}

#[test]
fn sample_value_and_rank_missingness_participate_in_input_identity() {
    let mut p = profile_player();
    let original = input_components(&p);
    p.sample.insert("Games Played".into(), 1.0);
    assert_ne!(original, input_components(&p));
    let sampled = input_components(&p);
    p.breakdown[0].value = Some(25.0);
    assert_ne!(sampled, input_components(&p));
    p.breakdown[0].pct = Some(0.0);
    let zero = input_components(&p);
    p.breakdown[0].pct = None;
    assert_ne!(zero, input_components(&p));
}

#[test]
fn sample_preserves_units_and_unknown_dates() {
    let mut p = profile_player();
    p.sample.insert("Games Played".into(), 10.0);
    p.sample.insert("Minutes Per Game".into(), 21.5);
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(
        prompt.contains("Stats updated: unknown; sample: Games Played 10, Minutes Per Game 21.5")
    );
    assert!(prompt.contains("Values: per-game averages"));
}

#[test]
fn rate_corroboration_requires_the_same_underlying_measurement() {
    let mut p = profile_player();
    p.breakdown = vec![dp("Shooting", 32.0, 2.0, 92.0, 1)];
    p.breakdown[0].measure = "shots on target".into();
    let mut other_measure = dp("Shooting", 0.2, 3.0, 99.0, 1);
    other_measure.measure = "expected goals".into();
    p.rate_modes.insert("per_90".into(), vec![other_measure]);
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Test Player"),
        &p,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(!prompt.contains("per-90 percentile"));
}

#[test]
fn season_changes_stay_with_their_own_skill() {
    let current = nfl_profile(
        "Guard",
        vec![
            dp("Creation", 1.0, 1.6, 66.0, 1),
            dp("Chance Creation", 1.0, 4.5, 95.0, 1),
        ],
    );
    let prior = nfl_profile("Guard", vec![dp("Creation", 1.0, 4.5, 95.0, 1)]);
    let changes = build_skill_changes(&current, &prior);
    assert_eq!(changes["Creation"].prior_pct, 95.0);
    assert!(!changes.contains_key("Chance Creation"));
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Morgan Rogers"),
        &current,
        None,
        Some(&changes),
        None,
        None,
        None,
    );
    let chance = prompt
        .lines()
        .find(|l| l.starts_with("- Chance Creation:"))
        .unwrap();
    assert!(!chance.contains("prior season percentile"));
    assert!(!chance.contains("slipped"));
    assert!(prompt.contains("Compatible cross-season measurements"));
    assert!(prompt.contains(
        "- Creation: prior 95.0; current 66.0; relative standing fell by 29.0 percentile points"
    ));
    assert!(!prompt.contains("- Chance Creation: prior"));
}

#[test]
fn comparison_block_orders_compatible_movements_by_magnitude() {
    let current = nfl_profile(
        "Center",
        vec![
            dp("Stable", 1.0, 0.0, 50.0, 1),
            dp("Large Rise", 1.0, 0.0, 90.0, 1),
            dp("Medium Fall", 1.0, 0.0, 20.0, 1),
        ],
    );
    let prior = nfl_profile(
        "Center",
        vec![
            dp("Stable", 1.0, 0.0, 50.0, 1),
            dp("Large Rise", 1.0, 0.0, 10.0, 1),
            dp("Medium Fall", 1.0, 0.0, 60.0, 1),
        ],
    );
    let changes = build_skill_changes(&current, &prior);
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &current,
        None,
        Some(&changes),
        None,
        None,
        None,
    );
    let rise = prompt.find("- Large Rise: prior").unwrap();
    let fall = prompt.find("- Medium Fall: prior").unwrap();
    let stable = prompt.find("- Stable: prior").unwrap();
    assert!(rise < fall && fall < stable);
    assert!(prompt.contains("relative standing rose by 80.0 percentile points"));
    assert!(prompt.contains("relative standing held within one percentile point (+0.0)"));
    assert!(prompt.contains("Use only these stated directions"));
}

fn dp(label: &str, value: f64, z: f64, pct: f64, sign: i32) -> RatingDatapoint {
    RatingDatapoint {
        label: label.to_string(),
        measure: label.to_string(),
        value: Some(value),
        z: Some(z),
        pct: Some(pct),
        sign,
        ..Default::default()
    }
}

#[test]
fn season_changes_require_the_same_underlying_measurement() {
    let mut current = nfl_profile("Guard", vec![dp("Shooting", 0.14, -0.3, 47.0, 1)]);
    let mut prior = nfl_profile("Guard", vec![dp("Shooting", 32.0, 4.0, 98.0, 1)]);
    current.breakdown[0].measure = "expected goals".into();
    prior.breakdown[0].measure = "shots on target".into();
    assert!(build_skill_changes(&current, &prior).is_empty());
    prior.breakdown[0].measure = "expected goals".into();
    assert_eq!(
        build_skill_changes(&current, &prior)["Shooting"].prior_pct,
        98.0
    );
    current.league_id = Some(8);
    prior.league_id = Some(9);
    assert!(build_skill_changes(&current, &prior).is_empty());
    prior.league_id = Some(8);
    assert_eq!(build_skill_changes(&current, &prior).len(), 1);
    current.breakdown[0].measure.clear();
    prior.breakdown[0].measure.clear();
    assert!(build_skill_changes(&current, &prior).is_empty());
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
        observed_at: None,
        sample: Default::default(),
        league_id: None,
        entity_type: "player".to_string(),
        season: 2025,
        position: "Guard".to_string(),
        composite_score: Some(67.0),
        breakdown: vec![scoring, dp("Defense", 2.5, -0.5, 40.0, 1)],
        scoped_ranks: HashMap::new(),
        rate_modes: HashMap::new(),
    }
}

#[test]
fn serialized_request_has_evidence_and_form_but_no_editorial_outline() {
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &profile_player(),
        None,
        None,
        None,
        None,
        None,
    );
    let client = crate::runtime::providers::ollama::OllamaClient::new(
        "http://localhost:11434",
        "offline-test",
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let request = client.request_body(
        &prompt,
        &crate::runtime::providers::ollama::GenerateOptions {
            system: Some(RATING_SYSTEM_PROMPT.to_string()),
            ..Default::default()
        },
    );
    let system = request["messages"][0]["content"].as_str().unwrap();
    let evidence = request["messages"][1]["content"].as_str().unwrap();
    assert!(system.contains(crate::composition::form::STORY_FORM));
    assert!(evidence.contains("Scoring: 24, percentile 95.0 (elite)"));
    assert!(evidence.contains("Defense: 2.5, percentile 40.0 (below average)"));
    for retired in [
        "DECISION CARD",
        "Headline strength",
        "Headline limitation",
        "THE SHAPE",
        "TWO OR THREE",
        "Harborview",
        "Strengths:",
        "Summary:",
    ] {
        assert!(
            !system.contains(retired) && !evidence.contains(retired),
            "{retired}"
        );
    }
}

fn faceted(label: &str, pct: f64, facet: &str) -> RatingDatapoint {
    let mut d = dp(label, 1.0, 0.0, pct, 1);
    d.facet = facet.to_string();
    d
}

fn nfl_profile(position: &str, breakdown: Vec<RatingDatapoint>) -> RatingProfile {
    RatingProfile {
        observed_at: None,
        sample: Default::default(),
        league_id: None,
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
        None,
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
    artifact.value = Some(0.0);
    artifact.z = Some(-0.28);
    let mut real_absence = faceted("Touchdowns", 2.0, "offense");
    real_absence.value = Some(0.0);
    real_absence.z = Some(-2.1);
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
    clumped.value = Some(1.0);
    clumped.z = Some(0.199);
    clumped.sign = -1;
    assert!(!is_weakness(&clumped));
    // Stafford's giveaways: 0th pct at sign-adjusted z -4.9 -> emphatically a weakness.
    let mut real = faceted("Giveaways", 0.0, "offense");
    real.value = Some(12.0);
    real.z = Some(4.9023);
    real.sign = -1;
    assert!(is_weakness(&real));
    // With only artifact-grade negatives, the decision names NO weakness.
    let mut strong = faceted("Air Yards Responsible", 92.9, "offense");
    strong.value = Some(919.0);
    strong.z = Some(1.0);
    let p = nfl_profile("WR", vec![strong, clumped]);
    let d = build_scouting_decision(&p);
    assert_eq!(d.primary_weakness_to_exploit, None);
}

#[test]
fn scouting_decision_weakness_is_positional_after_filter() {
    let mut giveaways = faceted("Giveaways", 20.0, "offense");
    giveaways.value = Some(12.0);
    giveaways.z = Some(2.0); // sign-adjusted -2.0: materially bad, a real weakness
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
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        prompt,
        "Entity: Test Player (NBA player, Guard); season 2025\n\
Stats updated: unknown; sample: unknown\n\
\nOverall standardized score (50 = average): 67\n\
Values: per-game averages, except percentages.\n\
\nCurrent-snapshot measurements. Percentiles, when present, rank the same measure among eligible entities in this sport and season; higher is better. Missing ranks and season comparisons are unmeasured.\n\
- Scoring: 24, percentile 95.0 (elite)\n\
- Defense: 2.5, percentile 40.0 (below average)\n\
"
    );
}

#[test]
fn prompt_team_no_composite_no_position() {
    // Team: position "" (no ", Guard" in the header), no composite line, one datapoint.
    let p = RatingProfile {
        observed_at: None,
        sample: Default::default(),
        league_id: None,
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
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        prompt,
        "Entity: Test FC (FOOTBALL team); season 2025\n\
Stats updated: unknown; sample: unknown\n\
Values: season totals, except percentages and named adjustments.\n\
\nCurrent-snapshot measurements. Percentiles, when present, rank the same measure among eligible entities in this sport and season; higher is better. Missing ranks and season comparisons are unmeasured.\n\
- Defense: 0.38, percentile 78.0 (strong)\n\
"
    );
}

#[test]
fn scouting_decision_requires_no_standout_when_top_is_only_above_average() {
    let p = RatingProfile {
        observed_at: None,
        sample: Default::default(),
        league_id: None,
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

// --- input_components canonical JSON: the input_hash pre-image ----------------------------------

#[test]
fn input_components_is_canonical_json() {
    // Datapoints walk the breakdown in STORED order (NOT pct-sorted): Scoring then Defense.
    // Top keys sorted: composite_score, datapoints, position, prompt_version, season
    // Datapoint keys are sorted too. The version is interpolated so future bumps do not require
    // hand-editing this shape assertion.
    let ic = input_components(&profile_player());
    assert_eq!(
        ic,
        format!(
            r#"{{"composite_score":67.0,"datapoints":[{{"label":"Scoring","measure":"Scoring","pct":95.0,"value":24.0}},{{"label":"Defense","measure":"Defense","pct":40.0,"value":2.5}}],"position":"Guard","prompt_version":"{RATING_PROMPT_VERSION}","sample":{{}},"season":2025}}"#
        )
    );
    // The hash is a deterministic function of those exact bytes.
    assert_eq!(hash_components(&ic), hash_components(&ic));
}

#[test]
fn input_components_omits_absent_optional_keys() {
    // No composite, no position, no rate modes → only season/datapoints (+ version).
    let p = RatingProfile {
        observed_at: None,
        sample: Default::default(),
        league_id: None,
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
        format!(
            r#"{{"datapoints":[],"prompt_version":"{RATING_PROMPT_VERSION}","sample":{{}},"season":2024}}"#
        )
    );
}

#[test]
fn rating_datapoint_tolerates_null_values() {
    // Sparse stored datapoints keep missing numbers distinct from measured zeros.
    let d: RatingDatapoint = serde_json::from_str(
        r#"{"label":"Penalties Won","value":null,"z":0.0,"pct":12.4,"scoped_pct":{"position":11.6,"x":null}}"#,
    )
    .expect("null tolerated like Go");
    assert_eq!(d.value, None);
    assert_eq!(d.pct, Some(12.4));
    assert_eq!(d.scoped_pct.get("position"), Some(&11.6));
    assert_eq!(d.scoped_pct.get("x"), None);
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
fn trim_float_formats_compactly() {
    assert_eq!(trim_float(3.0), "3"); // integral → %.0f
    assert_eq!(trim_float(24.0), "24");
    assert_eq!(trim_float(0.38), "0.38"); // abs < 1 → %.2f
    assert_eq!(trim_float(0.4), "0.4");
    assert_eq!(trim_float(1.01), "1.01");
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
        observed_at: None,
        sample: Default::default(),
        league_id: None,
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

#[test]
fn clean_commentary_strips_fences() {
    assert_eq!(
        clean_commentary("`A solid two-way wing.`"),
        "A solid two-way wing."
    );
    assert_eq!(clean_commentary("Plain prose."), "Plain prose.");
}

#[test]
fn rating_work_token_carries_contract_and_still_parses_season() {
    let v = rating_work_input_version(2025, Some("abc123"));
    assert_eq!(v, "rating:s2025:abc123");
    assert_eq!(rating_work_season(Some(&v)), Some(2025));
    assert_eq!(
        rating_work_input_version(2025, None),
        "rating:s2025:no-stats"
    );
    assert_eq!(
        rating_work_season(Some("rating:s2024:deadbeef")),
        Some(2024)
    );
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
    assert_eq!(v, "rating:s2025:xfer4211");

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
    assert!(!rating_work_is_transfer_triggered(Some(
        "rating:s2024:deadbeef"
    )));
    assert!(!rating_work_is_transfer_triggered(None));
}

#[test]
fn every_availability_event_on_one_day_collapses_to_a_single_work_row() {
    // Scott's constraint, 2026-08-23: "on an event day, the Scout is enqueued one time instead
    // of multiple." This is the whole mechanism — the marker keys on the DAY, so N events for
    // one entity on one date render the IDENTICAL input_version, and work::enqueue's
    // `WHERE input_version IS DISTINCT FROM EXCLUDED.input_version` collapses them into one row.
    // No debounce table, no dedup pass. If this assertion ever fails, a club losing three
    // players to knocks in an afternoon burns three model calls on the fleet's slowest seat.
    let first = rating_work_input_version_for_availability(2025, "2026-08-23");
    let second = rating_work_input_version_for_availability(2025, "2026-08-23");
    assert_eq!(first, second);
    assert_eq!(first, "rating:s2025:avail2026-08-23");

    // A DIFFERENT day must reopen — otherwise a fresh injury the next morning is absorbed by
    // yesterday's done row and the Scout never looks again.
    let next_day = rating_work_input_version_for_availability(2025, "2026-08-24");
    assert_ne!(first, next_day);

    // The season parse survives the marker, as it must for the transfer mark (the handler reads
    // the season back out of this token).
    assert_eq!(rating_work_season(Some(&first)), Some(2025));
}

#[test]
fn the_two_non_statistical_triggers_are_distinguishable_from_each_other_and_from_stats() {
    let avail = rating_work_input_version_for_availability(2025, "2026-08-23");
    let xfer = rating_work_input_version_for_transfer(2025, 4211);
    let stats = rating_work_input_version(2025, Some("abc123"));
    let no_stats = rating_work_input_version(2025, None);

    // Mutually exclusive. A mark that answered to both predicates would make trigger_type
    // meaningless, and trigger_type is the ONLY record of what woke the seat — the input_hash
    // deliberately does not move for either of these.
    assert!(rating_work_is_availability_triggered(Some(&avail)));
    assert!(!rating_work_is_transfer_triggered(Some(&avail)));
    assert!(rating_work_is_transfer_triggered(Some(&xfer)));
    assert!(!rating_work_is_availability_triggered(Some(&xfer)));

    // A real hash can never be mistaken for a mark: the slot otherwise holds a hex digest or
    // `no-stats`, and neither 'x' (xfer) nor 'v' (avail) is a hex digit.
    for token in [&stats, &no_stats] {
        assert!(!rating_work_is_availability_triggered(Some(token)));
        assert!(!rating_work_is_transfer_triggered(Some(token)));
    }

    // The debounce bypass covers both and ONLY both. If a periodic token ever bypassed, every
    // rating in the fleet would regenerate on every enumeration.
    assert!(rating_work_bypasses_debounce(Some(&avail)));
    assert!(rating_work_bypasses_debounce(Some(&xfer)));
    assert!(!rating_work_bypasses_debounce(Some(&stats)));
    assert!(!rating_work_bypasses_debounce(None));

    // The three-way that lands in stat_summaries.trigger_type. Every one of these values must
    // be admitted by the mig 228 CHECK — a value outside it fails the INSERT *after* the model
    // call, which is the bug mig 228 exists to close.
    assert_eq!(rating_trigger_type(Some(&avail)), "availability");
    assert_eq!(rating_trigger_type(Some(&xfer)), "transfer");
    assert_eq!(rating_trigger_type(Some(&stats)), "periodic");
    assert_eq!(rating_trigger_type(None), "periodic");
}

/// The Editor's TAG (2026-08-23). A `pk:` rating row is minted by mig 225 from
/// `slice_fingerprints->>'rating'`, which hashes the injury/suspension claims and nothing else —
/// so the row exists BECAUSE that news moved.
///
/// Both assertions here are load-bearing. If the bypass misses, the enqueue lands and the seat
/// skips it before the model call, which is precisely how the routing-subscription route was
/// measured to fail before the rating slice existed. If the trigger type misses, the Editor's
/// tag is filed under the nightly batch and the one provenance signal separating them is lost.
#[test]
fn a_packet_tagged_rating_row_bypasses_the_debounce_and_records_its_trigger() {
    let packet = "pk:9f8e7d6c5b4a3928";

    assert!(rating_work_bypasses_debounce(Some(packet)));
    assert_eq!(rating_trigger_type(Some(packet)), "availability");

    // It must not be confusable with the stats-derived versions in either direction: `pk:` rows
    // carry no season and no mark slot, so the mark readers must simply decline them.
    assert!(!rating_work_is_transfer_triggered(Some(packet)));
    assert!(!rating_work_is_availability_triggered(Some(packet)));
    assert!(rating_work_season(Some(packet)).is_none());

    // And a stats-derived version is never mistaken for a packet one — otherwise the whole
    // fleet would bypass the debounce on every enumeration.
    let stats = rating_work_input_version(2026, Some("a1b2c3d4e5f60718"));
    assert!(!rating_work_bypasses_debounce(Some(&stats)));
}

#[test]
fn rating_parser_rejects_empty_cards() {
    assert!(RatingParser.parse("").is_err());
}

#[test]
fn request_parser_rewrites_reversed_comparison_direction() {
    let directions = BTreeMap::from([
        ("Scoring".into(), RelativeDirection::Rose),
        ("Steals".into(), RelativeDirection::Fell),
    ]);
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new(
        "Scoring and steals comparison supplied.",
        &directions,
        &bands,
    );
    let reversed = parser
        .parse(r#"{"headline":"Clingan's profile","body":"Scoring declined relative to peers."}"#)
        .unwrap_err();
    assert!(reversed.is::<crate::composition::form::SurfaceError>());
    assert!(reversed.to_string().contains("evidence says it rose"));

    let accepted = parser
        .parse(r#"{"headline":"Clingan's profile","body":"Scoring rose while steals fell relative to peers."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("Scoring rose"));

    let grouped = parser
        .parse(r#"{"headline":"Clingan's profile","body":"Scoring, steals, and playmaking are below average, with relative standing improving across these areas."}"#)
        .unwrap_err();
    assert!(grouped.to_string().contains("Steals rose"));
}

#[test]
fn request_parser_rewrites_an_unsourced_height() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new("A center for Portland.", &directions, &bands);
    let error = parser
        .parse(r#"{"headline":"Clingan's profile","body":"The 6'9\" center protects the rim."}"#)
        .unwrap_err();
    assert!(error.is::<crate::composition::form::SurfaceError>());
    assert!(error.to_string().contains("invents height"));

    let possessive = parser
        .parse(r#"{"headline":"LeVert's profile","body":"LeVert’s scoring profile is measured."}"#)
        .expect("a typographic possessive is not a height")
        .expect("a reply");
    assert!(possessive.body.contains("LeVert’s"));
}

#[test]
fn request_parser_rewrites_mixed_blanket_claims_and_self_contradictory_form() {
    let directions = BTreeMap::from([
        ("Scoring".into(), RelativeDirection::Rose),
        ("Steals".into(), RelativeDirection::Fell),
    ]);
    let bands = BTreeMap::new();
    let parser =
        RatingRequestParser::new("Comparison and recent form supplied.", &directions, &bands);
    let blanket = parser
        .parse(r#"{"headline":"Mixed profile","body":"The player shows consistent improvement across key metrics."}"#)
        .unwrap_err();
    assert!(blanket.to_string().contains("mixed directions"));

    let form = parser
        .parse(r#"{"headline":"Mixed profile","body":"A downward trend is visible, but the player remains in strong form."}"#)
        .unwrap_err();
    assert!(form
        .to_string()
        .contains("both strong/rising and declining/falling"));
}

#[test]
fn request_parser_keeps_xg_and_xa_attached_to_their_measures() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new("xG and xA evidence supplied.", &directions, &bands);
    let xg = parser
        .parse(r#"{"headline":"Rogers profile","body":"His xG indicates strong creation."}"#)
        .unwrap_err();
    assert!(xg.to_string().contains("xG) is shooting/scoring evidence"));

    let xa = parser
        .parse(
            r#"{"headline":"Rogers profile","body":"Expected assists show stronger finishing."}"#,
        )
        .unwrap_err();
    assert!(xa.to_string().contains("xA) is creation evidence"));

    let grouped = parser
        .parse(r#"{"headline":"Rogers profile","body":"His xG and xA are both at elite percentiles (99.2 and 97.0)."}"#)
        .unwrap_err();
    assert!(grouped.to_string().contains("separate claims"));

    let accepted = parser
        .parse(r#"{"headline":"Rogers profile","body":"His xG supports the shooting read, while xA supports creation."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("xA supports creation"));
}

#[test]
fn request_parser_preserves_weighted_measures_and_thin_sample_coverage() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let prompt = "Discipline: 1 (yellow cards + 3 x red cards). The current sample has fewer than 10 appearances.";
    let parser = RatingRequestParser::new(prompt, &directions, &bands);
    let cards = parser
        .parse(
            r#"{"headline":"Rogers profile","body":"Discipline is poor: 1 yellow + 3 red cards."}"#,
        )
        .unwrap_err();
    assert!(cards.to_string().contains("weighted formula"));

    let games = parser
        .parse(r#"{"headline":"Rogers profile","body":"Rogers played 3 games with 257 minutes."}"#)
        .unwrap_err();
    assert!(games.to_string().contains("source coverage"));

    let bare_sample = parser
        .parse(
            r#"{"headline":"Rogers profile","body":"Rogers has 3 appearances and 257 minutes."}"#,
        )
        .unwrap_err();
    assert!(bare_sample.to_string().contains("source coverage"));

    let stability = parser
        .parse(r#"{"headline":"Rogers profile","body":"No change in relative standing is indicated; he remains reliable and consistently elite."}"#)
        .unwrap_err();
    assert!(stability
        .to_string()
        .contains("no computed cross-season direction"));

    let psychology = parser
        .parse(r#"{"headline":"Rogers profile","body":"His reported frustration may influence his decision-making under pressure."}"#)
        .unwrap_err();
    assert!(psychology.to_string().contains("psychological inference"));

    let long_body = "x".repeat(801);
    let oversized = parser
        .parse(&serde_json::json!({"headline": "Rogers profile", "body": long_body}).to_string())
        .unwrap_err();
    assert!(oversized.to_string().contains("exceeds 800 characters"));

    let accepted = parser
        .parse(r#"{"headline":"Rogers profile","body":"The stored snapshot records 3 appearances and 257 minutes. Discipline ranks poorly."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("stored snapshot"));
}

#[test]
fn request_parser_keeps_quality_words_in_the_supplied_percentile_band() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::from([
        ("Tackling".into(), "below average".into()),
        ("Chance Creation".into(), "elite".into()),
        ("Creation".into(), "strong".into()),
    ]);
    let parser = RatingRequestParser::new("Percentile bands supplied.", &directions, &bands);
    let error = parser
        .parse(r#"{"headline":"Rogers profile","body":"Tackling is above average, while Chance Creation is elite."}"#)
        .unwrap_err();
    assert!(error.to_string().contains("Tackling above average"));
    assert!(error.to_string().contains("below average"));

    let accepted = parser
        .parse(r#"{"headline":"Rogers profile","body":"Tackling is below average, while Chance Creation is elite."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("Chance Creation is elite"));
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

    // Absent line → None; the shared prose scrub still removes a retired form label.
    let bare = RatingParser
        .parse("Summary: The verdict stands.")
        .unwrap()
        .unwrap();
    assert!(bare.headline.is_none());
    assert_eq!(bare.body, "Summary: The verdict stands.");

    // Empty title folds to None, never an error.
    let empty = RatingParser
        .parse("Summary: x.\nHEADLINE: ")
        .unwrap()
        .unwrap();
    assert!(empty.headline.is_none());

    // Since 2026-08-24 the contract is 140 CHARACTERS and nothing else, so the thirteen-word
    // title that used to be the canonical violation now ships — it was 91% of the fleet's hook
    // rejections, and each one burned a finished report's title.
    let thirteen = RatingParser
        .parse("Summary: x.\nHEADLINE: one two three four five six seven eight nine ten eleven twelve thirteen")
        .expect("a junk title never fails the report")
        .expect("a reply");
    assert_eq!(
        thirteen.headline.as_deref(),
        Some("one two three four five six seven eight nine ten eleven twelve thirteen")
    );

    // An overlong optional title is dropped; it never costs the valid body.
    let overlong = RatingParser
        .parse(&format!("Summary: x.\nHEADLINE: {}", "x".repeat(200)))
        .expect("an overlong title never fails the report")
        .expect("a reply");
    assert_eq!(overlong.body, "Summary: x.");
    assert!(overlong.headline.is_none());
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
        &[],
        0,
    )
    .expect("a move renders");
    assert_eq!(
        out,
        "- Jul 29: current-club identity confirmed as New FC (previously Old FC) (transfer).\n"
    );
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
        &[],
        0,
    )
    .unwrap();
    assert_eq!(
        out,
        "- Jul 16: current-club identity confirmed as New FC (transfer).\n"
    );
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
        &[],
        0,
    )
    .unwrap();
    assert_eq!(
        arrival,
        "- Jul 29: Test Player's current-club identity confirmed here (previously Old FC) (transfer).\n"
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
        &[],
        0,
    )
    .unwrap();
    assert_eq!(
        departure,
        "- Jul 29: Test Player's current-club identity confirmed as New FC (transfer).\n"
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
        &[],
        0,
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
        &[],
        0,
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
    let out = render_personnel_block("team", 3468, &rows, MAX_PERSONNEL_LINES + 4, &[], 0).unwrap();
    assert_eq!(out.lines().count(), MAX_PERSONNEL_LINES + 1);
    assert!(out.ends_with("- (+4 older personnel changes in this window, not shown)\n"));

    // Nothing dropped ⇒ no drop line at all.
    let exact = render_personnel_block("team", 3468, &rows, MAX_PERSONNEL_LINES, &[], 0).unwrap();
    assert_eq!(exact.lines().count(), MAX_PERSONNEL_LINES);
    assert!(!exact.contains("not shown"));
}

fn avail(
    kind: &str,
    date_label: &str,
    event_kind: &str,
    player: &str,
    event_date: &str,
    expected: Option<&str>,
) -> AvailabilityChange {
    AvailabilityChange {
        kind: kind.to_string(),
        date_label: date_label.to_string(),
        event_kind: event_kind.to_string(),
        player_name: player.to_string(),
        team_name: Some("New FC".to_string()),
        team_id: Some(3468),
        event_date_label: event_date.to_string(),
        expected_return_label: expected.map(str::to_string),
    }
}

/// A newly applied injury renders as a dated fact, and the reported prognosis renders as a
/// REPORT ("reported back around") rather than as a date the player will return — mig 229:
/// `expected_return` is a claim, never ground truth.
#[test]
fn an_opened_absence_renders_the_prognosis_as_a_report() {
    let player = render_personnel_block(
        "player",
        1,
        &[],
        0,
        &[avail(
            "opened",
            "Aug 21",
            "injury",
            "Test Player",
            "Aug 20",
            Some("Sep 02"),
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        player,
        "- Aug 20: out with a recorded injury — reported back around Sep 02.\n"
    );

    // No prognosis ⇒ no clause invented.
    let bare = render_personnel_block(
        "player",
        1,
        &[],
        0,
        &[avail(
            "opened",
            "Aug 21",
            "suspension",
            "Test Player",
            "Aug 20",
            None,
        )],
        1,
    )
    .unwrap();
    assert_eq!(bare, "- Aug 20: out with a recorded suspension.\n");
    assert!(!bare.contains("reported back"));

    // A team read names WHO is missing.
    let team = render_personnel_block(
        "team",
        3468,
        &[],
        0,
        &[avail(
            "opened",
            "Aug 21",
            "injury",
            "Test Player",
            "Aug 20",
            None,
        )],
        1,
    )
    .unwrap();
    assert_eq!(team, "- Aug 20: Test Player out with a recorded injury.\n");
}

/// THE distinction mig 229 built two columns to keep: a RETURN is the player coming back, a
/// REVERT is us withdrawing the claim he was ever hurt. Rendering a revert as a return would tell
/// the Scout a player is fit on the strength of a record we just retracted.
#[test]
fn a_withdrawn_availability_record_never_reads_as_a_return() {
    let returned = render_personnel_block(
        "player",
        1,
        &[],
        0,
        &[avail(
            "returned",
            "Aug 30",
            "injury",
            "Test Player",
            "Aug 20",
            None,
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        returned,
        "- Aug 30: available again after the injury recorded Aug 20.\n"
    );

    let reverted = render_personnel_block(
        "player",
        1,
        &[],
        0,
        &[avail(
            "reverted",
            "Aug 25",
            "injury",
            "Test Player",
            "Aug 20",
            None,
        )],
        1,
    )
    .unwrap();
    assert_eq!(
        reverted,
        "- Aug 25: the injury recorded Aug 20 was WITHDRAWN — that record is not in force.\n"
    );
    // The two must not be confusable in either direction.
    assert!(!reverted.contains("available again"));
    assert!(!returned.contains("WITHDRAWN"));

    let team_reverted = render_personnel_block(
        "team",
        3468,
        &[],
        0,
        &[avail(
            "reverted",
            "Aug 25",
            "suspension",
            "Test Player",
            "Aug 20",
            None,
        )],
        1,
    )
    .unwrap();
    assert!(team_reverted.contains("Test Player's suspension recorded Aug 20 was WITHDRAWN"));
    assert!(!team_reverted.contains("available again"));
}

/// Both halves render into ONE block, transfers first, and each names its own drops (A5).
#[test]
fn transfers_and_availability_share_one_block_and_each_names_its_drops() {
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
    let avails: Vec<AvailabilityChange> = (0..MAX_AVAILABILITY_LINES)
        .map(|i| {
            avail(
                "opened",
                "Aug 21",
                "injury",
                &format!("Hurt {i}"),
                "Aug 20",
                None,
            )
        })
        .collect();
    let out = render_personnel_block(
        "team",
        3468,
        &rows,
        MAX_PERSONNEL_LINES + 2,
        &avails,
        MAX_AVAILABILITY_LINES + 3,
    )
    .unwrap();

    // Transfers, their drop line, availability, then its drop line — in that order.
    let signed = out
        .find("Player 0's current-club identity confirmed")
        .unwrap();
    let pers_drop = out.find("+2 older personnel changes").unwrap();
    let hurt = out.find("Hurt 0 out with a recorded injury").unwrap();
    let avail_drop = out.find("+3 older availability events").unwrap();
    assert!(signed < pers_drop && pers_drop < hurt && hurt < avail_drop);

    // Availability alone still produces a block — the section is not gated on transfers.
    assert!(
        render_personnel_block("team", 3468, &[], 0, &avails, MAX_AVAILABILITY_LINES).is_some()
    );
}

/// The Editor's TAGGED reports reach the Scout as CLAIMS — attributed, with contradictions
/// marked by code. Scott's 2026-08-23 ruling in test form: he judges legitimacy, so he must see
/// who said what and where they disagree. `⇄` is the mark, and BOTH members of a contested pair
/// are always carried (T3/D6) — collapsing the pair would be deciding for him.
#[test]
fn tagged_current_reports_arrive_attributed_and_contest_marked() {
    use crate::junctions::editor::render::{mark_contested, RenderClaim};

    let claim = |source: &str, fact: &str| RenderClaim {
        article_id: 1,
        source: source.to_string(),
        fact: fact.to_string(),
        published_at: Some(100),
        story_type: "injury".to_string(),
    };

    // Agreeing claims: attributed, unmarked.
    let agreeing = mark_contested(&[
        claim("BBC", "Palmer is out for six weeks"),
        claim("Sky", "Palmer faces six weeks out"),
    ]);
    let out = render_scout_reports(&agreeing).unwrap();
    assert!(out.contains("- BBC: Palmer is out for six weeks\n"));
    assert!(!out.contains('⇄'));

    // Contradicting claims: BOTH carried, BOTH marked.
    let contested = mark_contested(&[
        claim("BBC", "Palmer will miss the derby"),
        claim("The Athletic", "Palmer will not miss the derby"),
    ]);
    let out = render_scout_reports(&contested).unwrap();
    assert_eq!(out.matches('⇄').count(), 2, "both sides must be marked");
    assert!(out.contains("BBC") && out.contains("The Athletic"));

    // KNOWN GAP, asserted so it cannot regress silently. `mark_contested`'s negation list was
    // tuned for TRANSFER prose and contains "ruled" (as in "ruled out of contention"). On injury
    // prose "ruled out" therefore reads as negated on BOTH sides, the polarities match, and a
    // genuine contradiction goes unmarked. The claims are still both carried and both attributed
    // — the Scout sees the disagreement, he just does not get the pointer. Widening that list is
    // a change to the Insider's marker too, so it is deliberately NOT done as a side effect here.
    let unmarked = mark_contested(&[
        claim("BBC", "Palmer has been ruled out of the derby"),
        claim("The Athletic", "Palmer has not been ruled out of the derby"),
    ]);
    let out = render_scout_reports(&unmarked).unwrap();
    assert_eq!(
        out.matches('⇄').count(),
        0,
        "documents the transfer-tuned negation gap"
    );
    assert!(
        out.lines().count() == 2,
        "both claims still reach him regardless"
    );

    // Nothing reported ⇒ no section, same discipline as the personnel block.
    assert!(render_scout_reports(&[]).is_none());
}

/// The reports block must not be confusable with the adjudicated record, and the prompt has to
/// say which is which — a Scout reading a claim as a confirmed fact is the failure this design
/// exists to avoid.
#[test]
fn the_prompt_separates_current_reports_from_the_confirmed_record() {
    use crate::junctions::editor::render::{mark_contested, RenderClaim};
    let p = profile_player();
    let reports = mark_contested(&[RenderClaim {
        article_id: 1,
        source: "BBC".to_string(),
        fact: "Palmer is out for six weeks".to_string(),
        published_at: Some(100),
        story_type: "injury".to_string(),
    }]);
    let rendered = render_scout_reports(&reports).unwrap();
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Test Player"),
        &p,
        None,
        None,
        None,
        Some(&rendered),
        None,
    );
    let reported = prompt.find("Current attributed reports").unwrap();
    assert!(prompt[reported..].contains("- BBC: Palmer is out for six weeks"));
    assert!(prompt.contains("- BBC: Palmer is out for six weeks"));
    // The instructions that make it judgeable rather than quotable.
    assert!(prompt.contains("attributed reports"));
    assert!(prompt.contains("preserve uncertainty"));
    // And a claim must never be allowed to move a measured number.
    assert!(prompt.contains("reports do not alter measured statistics"));
}

#[test]
fn thin_current_sample_withholds_directional_cross_season_claims() {
    let mut p = profile_player();
    p.sample.insert("appearances".to_string(), 3.0);
    assert!(!inputs::supports_cross_season_comparison(&p));
    let prompt = build_stat_prompt(
        &req("FOOTBALL", "player", "Test Player"),
        &p,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(prompt.contains("fewer than 10 appearances"));
    assert!(prompt.contains("stored thin sample; participation details omitted"));
    assert!(!prompt.contains("Appearances 3"));
    assert!(prompt.contains("no cross-season change was computed"));
    assert!(prompt.contains("Do not claim improvement, decline, stability"));
    assert!(prompt.contains("at most the two highest printed"));
    assert!(prompt.contains("exact labels and bands"));
    assert!(prompt.contains("Omit participation totals and Discipline"));
    assert!(prompt.contains("Never say mid-season"));
    assert!(prompt.contains("unless this prompt explicitly says it is unresolved"));
    assert!(prompt.contains("do not mention printed measurements"));
    assert!(prompt.contains("Do not infer emotion"));
    assert!(prompt.contains("at most 800 characters"));

    p.sample.insert("appearances".to_string(), 10.0);
    assert!(inputs::supports_cross_season_comparison(&p));
}

#[test]
fn thin_current_sample_shows_only_its_two_highest_ranked_measures() {
    let mut p = profile_player();
    p.sample.insert("appearances".to_string(), 3.0);
    p.breakdown.push(dp("Passing", 9.0, 0.5, 80.0, 1));

    let prompt_profile = model_prompt_profile(&p, false);
    assert_eq!(prompt_profile.composite_score, None);
    assert_eq!(prompt_profile.breakdown.len(), 2);
    assert_eq!(prompt_profile.breakdown[0].label, "Scoring");
    assert_eq!(prompt_profile.breakdown[1].label, "Passing");
    assert_eq!(prompt_profile.sample["appearances"], 3.0);
}

/// No changes ⇒ no section. A heading with nothing under it asserts "nothing moved", which is a
/// claim the adjudication chain has not made — it may only mean nothing has been adjudicated yet.
#[test]
fn nothing_moved_renders_no_section_at_all() {
    assert!(render_personnel_block("player", 1, &[], 0, &[], 0).is_none());
    let p = profile_player();
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        None,
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
fn personnel_follows_measurements_without_generated_prose_memory() {
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
        &[],
        0,
    )
    .unwrap();
    let prompt = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        Some(&personnel),
        None,
        None,
        None,
        None,
    );
    let dp = prompt.find("Current-snapshot measurements.").unwrap();
    let pers = prompt
        .find("Personnel and availability since our last read")
        .unwrap();
    assert!(dp < pers);
    assert!(!prompt.contains("Prior reading"));
    assert!(prompt.contains(
        "- Jul 29: current-club identity confirmed as New FC (previously Old FC) (transfer).\n"
    ));
    // The tier-truth invariant travels with the block.
    assert!(prompt.contains("season measurements remain unchanged"));

    // Blank personnel ⇒ no section, same as blank memory.
    let blank = build_stat_prompt(
        &req("NBA", "player", "Test Player"),
        &p,
        Some(" \n "),
        None,
        None,
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
            pct: Some(97.5 - (i as f64) * 2.5),
            ..Default::default()
        })
        .collect();
    let got = ordered_facts(&breakdown);

    assert_eq!(
        got.len(),
        MAX_STAT_FACTS,
        "the 4,096-window budget still binds"
    );
    // Presented high to low.
    for w in got.windows(2) {
        assert!(w[0].pct >= w[1].pct, "datapoints stay in pct DESC order");
    }
    // Both ends are present. Before s21 this list was facts[0..14] — everything at or below
    // the 62nd percentile was invisible, so the bottom assertion is the whole point.
    assert_eq!(
        got.first().unwrap().pct,
        Some(97.5),
        "the best skill is shown"
    );
    assert_eq!(got.last().unwrap().pct, Some(0.0), "and so is the worst");
    // ...and the middle survives, which top-plus-bottom would also have missed.
    let middle = got
        .iter()
        .filter(|d| d.pct.is_some_and(|pct| pct > 20.0 && pct < 75.0))
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
        .parse(&format!(
            "{body}\nHEADLINE: Hornets — elite shooting, poor containment"
        ))
        .expect("a two-beat title must not fail the report")
        .expect("a reply");
    assert!(salvaged.body.contains("Strengths:"), "the report survives");

    // An unsalvageable title degrades to no title, and the report still ships.
    let dropped = RatingParser
        .parse(&format!(
            "{body}\nHEADLINE: Hornets: Elite shooter, poor containment inside"
        ))
        .expect("an unsalvageable title must not fail the report")
        .expect("a reply");
    assert!(
        dropped.body.contains("Limitations:"),
        "the report survives: {:?}",
        dropped.body
    );
    assert!(
        dropped
            .headline
            .as_deref()
            .is_none_or(|h| crate::composition::guards::hook_violation(h).is_none()),
        "a shipped title always satisfies the contract: {:?}",
        dropped.headline
    );
}

#[test]
fn claim_paragraphs_survive_the_production_parser() {
    let body = "The profile is ordinary. Most skills sit near average. The middle is the story.\n\nOne edge stands out. Finishing leads the supplied profile. That is the exception.\n\nAvailability is limited. Two absences are recorded. Depth matters now.\n\nThe rest is unchanged. The supplied comparison shows no movement. Continuity holds.";
    let raw = format!("{body}\nHEADLINE: An ordinary profile holds");
    let parsed = RatingParser.parse(&raw).unwrap().unwrap();
    assert_eq!(parsed.body, body);
}
