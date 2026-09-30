//! Service-free tests for the Scout character and its prepared materials.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` resolves to the Studio character module.

use crate::plugins::scout::adapter::{
    rating_trigger_type, rating_work_bypasses_debounce, rating_work_input_version,
    rating_work_input_version_for_availability, rating_work_input_version_for_transfer,
    rating_work_is_availability_triggered, rating_work_is_transfer_triggered, rating_work_season,
};
use crate::util::hash_components;

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
    let prompt = world(
        &req("FOOTBALL", "player", "Morgan Rogers"),
        &current,
        Some(&changes),
        &RatingExclusions::default(),
    );
    // A one-appearance sample is stated as a boundary rather than as a
    // participation figure, and it carries no prior-season percentile — the
    // current observation is unranked, so an old rank is not comparable to it.
    assert_eq!(
        fresh(&prompt)["limit"]["kind"],
        serde_json::json!("one_appearance")
    );
    assert!(!prompt.contains("\"Appearances\""));
    assert!(fresh(&prompt)["values"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["label"] == serde_json::json!("Chance Creation")
            && v["value"] == serde_json::json!(1.01)));
    assert!(!prompt.contains("95.4"));
    assert!(fresh(&prompt)["values"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.get("band").is_none_or(|b| b.is_null())));
    assert!(!prompt.contains("prior_percentile"));
    assert!(!prompt.contains("Prior reading"));
}

#[test]
fn quality_z_is_normalized_once_and_unknown_polarity_stays_unknown() {
    // A measure whose raw direction is "lower is better" must present a
    // sign-adjusted z where positive is ALWAYS favourable, or the model reads a
    // good measure as a bad one.
    let world_of = |datapoint: RatingDatapoint| {
        let mut p = profile_player();
        p.breakdown = vec![datapoint];
        p.sample.insert("Games Played".to_string(), 20.0);
        world(
            &req("NBA", "player", "Test Player"),
            &p,
            None,
            &RatingExclusions::default(),
        )
    };
    for (raw_z, sign, expected) in [(-1.5, -1, 1.5), (1.5, -1, -1.5), (1.5, 1, 1.5)] {
        let presented = fresh(&world_of(dp("Turnovers", 4.0, raw_z, 90.0, sign)))["values"].clone();
        let value = &presented.as_array().unwrap()[0];
        assert_eq!(
            value["quality_z"],
            serde_json::json!(expected),
            "a lower-is-better measure with z {raw_z} must present as {expected}"
        );
        // The polarity itself is not presented as a word any more; the normalized
        // z and the band are what the model reads, and both are already oriented.
        assert!(value["band"].is_string());
    }
    // An unknown or invalid polarity yields no z at all, rather than a zero that
    // would read as exactly average.
    for sign in [0, 2, -2] {
        let presented = fresh(&world_of(dp("Turnovers", 4.0, 1.5, 90.0, sign)))["values"].clone();
        let value = &presented.as_array().unwrap()[0];
        assert!(
            value.get("quality_z").is_none_or(|z| z.is_null()),
            "unknown polarity must not manufacture a score: {value}"
        );
    }
}
#[test]
fn observed_zero_missing_measurement_and_recent_direction_stay_distinct() {
    let mut profile = profile_player();
    profile.composite_score = None;
    profile.breakdown = vec![
        RatingDatapoint {
            label: "Goalscoring".into(),
            measure: "goals".into(),
            value: Some(0.0),
            sign: 1,
            ..Default::default()
        },
        RatingDatapoint {
            label: "Chance Creation".into(),
            measure: "expected assists".into(),
            sign: 1,
            ..Default::default()
        },
    ];
    let subject = req("FOOTBALL", "player", "Test Player");
    for absent in [None, Some(""), Some("  ")] {
        let world = world_with_trend(
            &subject,
            &profile,
            None,
            &RatingExclusions::default(),
            absent,
        );
        let fresh = fresh(&world);
        // A measured zero and a missing measurement stay distinct. The zero is
        // carried as a value; the missing one carries no value at all, and the
        // absence of a quality z keeps unknown polarity from reading as average.
        assert_eq!(
            measure_with(&fresh, "value")["label"],
            serde_json::json!("Goalscoring")
        );
        assert_eq!(
            measure_with(&fresh, "value")["value"],
            serde_json::json!(0.0)
        );
        let unmeasured = fresh["values"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["label"] == serde_json::json!("Chance Creation"))
            .unwrap();
        assert!(unmeasured.get("value").is_none_or(|v| v.is_null()));
        assert!(unmeasured.get("quality_z").is_none_or(|v| v.is_null()));
        assert!(!world.contains("quality_z"));
        // No computed trend means the part is omitted entirely. Absence is
        // readable as "not computed", which is not the same as "steady".
        assert!(fresh.get("trend").is_none() || fresh["trend"].is_null());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&world).unwrap()["trend"],
            serde_json::Value::Null
        );
    }
    for observed in [
        "overall scores holding steady over recent games; 5 scored events",
        "overall scores trending down over recent games; 5 scored events",
    ] {
        let world = world_with_trend(
            &subject,
            &profile,
            None,
            &RatingExclusions::default(),
            Some(observed),
        );
        assert!(world.contains(observed));
        assert_ne!(
            serde_json::from_str::<serde_json::Value>(&world).unwrap()["trend"],
            serde_json::Value::Null
        );
    }
}

#[test]
fn polarity_and_z_changes_invalidate_the_material_hash() {
    let original = profile_player();
    let baseline = hash_components(&input_components(&original));
    let mut polarity = original.clone();
    polarity.breakdown[0].sign = -1;
    assert_ne!(baseline, hash_components(&input_components(&polarity)));
    let mut magnitude = original.clone();
    magnitude.breakdown[0].z = Some(0.2);
    assert_ne!(baseline, hash_components(&input_components(&magnitude)));
    magnitude.breakdown[0].z = None;
    assert_ne!(baseline, hash_components(&input_components(&magnitude)));
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
    let world = world(
        &req("NBA", "player", "Test Player"),
        &p,
        None,
        &RatingExclusions::default(),
    );
    let fresh = fresh(&world);
    // Unit-bearing stat-definition labels survive, because "21.5" and "21.5 per
    // match" are different facts.
    assert_eq!(fresh["sample"]["Games Played"], serde_json::json!(10.0));
    assert_eq!(fresh["sample"]["Minutes Per Game"], serde_json::json!(21.5));
    // An unknown observation date stays null rather than becoming a date.
    assert!(fresh["observed_at"].is_null());
}

#[test]
fn rate_corroboration_requires_the_same_underlying_measurement() {
    let mut p = profile_player();
    p.breakdown = vec![dp("Shooting", 32.0, 2.0, 92.0, 1)];
    p.breakdown[0].measure = "shots on target".into();
    let mut other_measure = dp("Shooting", 0.2, 3.0, 99.0, 1);
    other_measure.measure = "expected goals".into();
    p.rate_modes.insert("per_90".into(), vec![other_measure]);
    let world = world(
        &req("FOOTBALL", "player", "Test Player"),
        &p,
        None,
        &RatingExclusions::default(),
    );
    // A rate mode measuring a DIFFERENT underlying quantity is not corroboration
    // of the same skill. It may still be presented — it is a real elite rate — but
    // in its own part, so the model cannot read it as the same skill's standing.
    let package: serde_json::Value = serde_json::from_str(&world).unwrap();
    let values = package["fresh"]["values"].as_array().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["measure"], serde_json::json!("shots on target"));
    assert_eq!(values[0]["label"], serde_json::json!("Shooting"));
    // The rate standout is a different measurement, presented separately.
    let standouts = package["rate_standouts"].as_array().unwrap();
    assert!(standouts
        .iter()
        .any(|s| s["mode"] == serde_json::json!("per_90")
            && s["label"] == serde_json::json!("Shooting")
            && !values.iter().any(|v| v["percentile"] == s["percentile"])));
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
    let world = world(
        &req("FOOTBALL", "player", "Morgan Rogers"),
        &current,
        Some(&changes),
        &RatingExclusions::default(),
    );
    let values = fresh(&world)["values"].clone();
    let listed = values.as_array().unwrap();
    let by_label = |label: &str| {
        listed
            .iter()
            .find(|v| v["label"] == serde_json::json!(label))
            .cloned()
            .unwrap_or_else(|| panic!("{label} is missing from {values}"))
    };
    // Creation carries its own prior percentile. Chance Creation has no
    // compatible comparison, so selection drops it rather than presenting it
    // beside a comparison that belongs to a different skill.
    assert_eq!(
        by_label("Creation")["prior_percentile"],
        serde_json::json!(95.0)
    );
    assert_eq!(by_label("Creation")["percentile"], serde_json::json!(66.0));
    assert!(
        !listed
            .iter()
            .any(|v| v["label"] == serde_json::json!("Chance Creation")),
        "a measurement with no comparison must not borrow another's: {values}"
    );
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
    let world = world(
        &req("NBA", "player", "Test Player"),
        &current,
        Some(&changes),
        &RatingExclusions::default(),
    );
    // The selection orders by movement magnitude, so the largest move is carried
    // first and a held anchor trails it. The arithmetic itself is the plugin's:
    // the world carries both percentiles and the model may not compute a delta.
    // Selection, not presentation, decides which comparisons are worth the
    // space: it ranks by movement magnitude and keeps the largest movers plus a
    // held anchor. The world then carries each with its own prior percentile,
    // and the model computes no delta of its own.
    let values = fresh(&world)["values"].clone();
    let by_label = |label: &str| {
        values
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["label"] == serde_json::json!(label))
            .cloned()
            .unwrap_or_else(|| panic!("{label} was dropped: {values}"))
    };
    assert_eq!(
        by_label("Large Rise")["percentile"],
        serde_json::json!(90.0)
    );
    assert_eq!(
        by_label("Large Rise")["prior_percentile"],
        serde_json::json!(10.0)
    );
    assert_eq!(
        by_label("Medium Fall")["percentile"],
        serde_json::json!(20.0)
    );
    assert_eq!(
        by_label("Medium Fall")["prior_percentile"],
        serde_json::json!(60.0)
    );
    assert_eq!(
        by_label("Stable")["prior_percentile"],
        serde_json::json!(50.0)
    );
}

#[test]
fn comparison_block_foregrounds_held_anchors_beyond_the_change_limit() {
    let labels = ["Rise A", "Rise B", "Fall A", "Fall B", "Held Elite"];
    let current = nfl_profile(
        "Center",
        labels
            .iter()
            .zip([90.0, 80.0, 20.0, 30.0, 99.0])
            .map(|(label, pct)| dp(label, 1.0, 0.0, pct, 1))
            .collect(),
    );
    let prior = nfl_profile(
        "Center",
        labels
            .iter()
            .zip([10.0, 20.0, 80.0, 70.0, 99.2])
            .map(|(label, pct)| dp(label, 1.0, 0.0, pct, 1))
            .collect(),
    );
    let changes = build_skill_changes(&current, &prior);
    let world = world(
        &req("NBA", "player", "Test Player"),
        &current,
        Some(&changes),
        &RatingExclusions::default(),
    );
    let values = fresh(&world)["values"].clone();
    // Beyond the movement limit the selection still surfaces a HELD anchor, and
    // it is an anchor rather than a movement: its own current band is present and
    // its prior percentile is within a point, so nothing supports a direction.
    let held = values
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == serde_json::json!("Held Elite"))
        .cloned()
        .unwrap_or_else(|| panic!("a held anchor should be carried: {values}"));
    assert_eq!(held["percentile"], serde_json::json!(99.0));
    assert_eq!(held["band"], serde_json::json!("elite"));
    assert_eq!(held["prior_percentile"], serde_json::json!(99.2));
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

fn req(sport: &str, entity_type: &str, name: &str) -> Subject {
    Subject {
        entity_type: entity_type.to_string(),
        entity_name: name.to_string(),
        sport: sport.to_string(),
        sport_name: String::new(),
    }
}

/// The prepared world a profile produces, rendered.
///
/// This replaces the flat-prompt builder these tests used. The invariants are
/// unchanged — what the model may be told about a profile is the same — but they
/// are now asserted against the assembled package, which is what production
/// actually sends. A test that pinned the old prompt's wording is a test of a
/// string nobody reads.
///
/// The order mirrors the adapter exactly: the selection that decides which
/// measurements and which comparisons are carried runs first, and only what
/// survives becomes the world. A helper that skipped the selection would pass
/// tests production cannot.
fn world(
    subject: &Subject,
    profile: &RatingProfile,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
    exclusions: &RatingExclusions,
) -> String {
    world_with_trend(subject, profile, comparisons, exclusions, None)
}

fn world_with_trend(
    subject: &Subject,
    profile: &RatingProfile,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
    exclusions: &RatingExclusions,
    trend: Option<&str>,
) -> String {
    let supports_cross_season = supports_cross_season_comparison(profile);
    let selected = model_prompt_profile(profile, supports_cross_season, comparisons);
    parts::Parts {
        subject: crate::plugins::meta::EntityMeta {
            name: subject.entity_name.clone(),
            entity_type: subject.entity_type.clone(),
            entity_id: 1,
            sport: subject.sport.clone(),
        },
        sport_name: subject.sport_name.clone(),
        season: profile.season,
        profile: parts::profile_parts(
            &selected,
            &subject.sport_name,
            supports_cross_season,
            comparisons,
            exclusions,
        ),
        rate_standouts: parts::rate_standout_parts(profile),
        // A blank note is a trend that was not computed, not one that is empty.
        trend: trend
            .map(str::trim)
            .filter(|note| !note.is_empty())
            .map(|note| parts::Trend {
                direction: "steady".into(),
                sample_size: 5,
                note: Some(note.to_string()),
            }),
        memory: Default::default(),
    }
    .render()
}

/// The `fresh` part as parsed JSON, for assertions about structure rather than
/// about a rendered sentence.
fn fresh(world: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(world).expect("the world is a JSON package")["fresh"]
        .clone()
}

/// The label of the one measurement in the `fresh` part that carries `key`.
fn measure_with<'a>(fresh: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    fresh["values"]
        .as_array()
        .expect("values is an array")
        .iter()
        .find(|value| value.get(key).is_some_and(|v| !v.is_null()))
        .unwrap_or_else(|| panic!("no measurement carries {key}: {fresh}"))
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
    let prompt = world(
        &req("NBA", "player", "Test Player"),
        &profile_player(),
        None,
        &RatingExclusions::default(),
    );
    let client = crate::runtime::providers::ollama::OllamaClient::new(
        "http://localhost:11434",
        "offline-test",
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let request = client.request_body(
        &prompt,
        &crate::studio::model::GenerateOptions {
            system: Some(crate::plugins::scout::cognition::prompt::TASK.to_string()),
            ..Default::default()
        },
    );
    let system = request["messages"][0]["content"].as_str().unwrap();
    let evidence = request["messages"][1]["content"].as_str().unwrap();
    assert!(system.contains("Articulate the supplied profile into a scouting read"));
    // The package carries the evidence and the declared form, and nothing else.
    let package: serde_json::Value = serde_json::from_str(evidence).unwrap();
    for part in ["identity", "fresh", "memory", "voice", "form"] {
        assert!(package.get(part).is_some(), "package is missing {part}");
    }
    let values = fresh(evidence);
    assert_eq!(
        measure_with(&values, "value")["label"],
        serde_json::json!("Scoring")
    );
    assert_eq!(
        measure_with(&values, "value")["percentile"],
        serde_json::json!(95.0)
    );
    assert_eq!(
        measure_with(&values, "value")["band"],
        serde_json::json!("elite")
    );
    assert_eq!(package["form"]["keys"], serde_json::json!(["body"]));
    for retired in [
        "strengths and limitations",
        "strengths, limitations and tensions",
        "primary_strength_to_stop",
        "primary_weakness_to_exploit",
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
    let mut sample: BTreeMap<String, f64> = Default::default();
    // A comparison test that runs on a profile with no participation count
    // asserts nothing: the selection refuses to present a prior percentile
    // without one, and the assertion passes vacuously.
    sample.insert("Games Played".to_string(), 16.0);
    RatingProfile {
        observed_at: None,
        sample,
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
// never reach the prompt or input_hash. A QB's defensive stat sheet is outside his facet.
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
fn retired_metric_never_reaches_prompt_or_preimage() {
    // The Session D golden: Dan Burn-shaped profile — a display-tier metric holds the top
    // percentile. After the filter, the built prompt and input_components hash pre-image
    // must exclude it while retaining curated measurements.
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

    let world = world(
        &req("FOOTBALL", "player", "Test Defender"),
        &p,
        None,
        &RatingExclusions::default(),
    );
    for retired in ["Clearances", "Duels"] {
        assert!(
            !world.contains(retired),
            "retired metric {retired:?} leaked into the prepared world"
        );
    }
    assert!(world.contains("Interceptions"));

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

// --- rendered-world byte-fixtures: the deterministic parity axis. The expected strings are
// pinned so a change to the assembled package fails here, offline and without a model.
// They are byte assertions on purpose: `assembly::World` renders in insertion
// order, and a reordered or reshaped part is a behavior change, not a refactor.
// -----------------------------------------------------------------------------------------------

#[test]
fn a_players_world_is_byte_stable() {
    let world = world(
        &req("NBA", "player", "Test Player"),
        &profile_player(),
        None,
        &RatingExclusions::default(),
    );
    assert!(
        world.starts_with(concat!(
            r#"{"identity":{"name":"Test Player","entity_type":"player","sport":"basketball"},"#,
            r#""fresh":{"season":2025,"observed_at":null,"sample":{},"values":["#,
            r#"{"label":"Scoring","measure":"Scoring","value":24.0,"percentile":95.0,"band":"elite","quality_z":3.1},"#,
            r#"{"label":"Defense","measure":"Defense","value":2.5,"percentile":40.0,"band":"below average","quality_z":-0.5}"#,
            r#"],"composite":null,"supports_cross_season":false,"limit":{"kind":"unknown_sample","minimum":10.0}},"#,
            r#""memory":{},"rate_standouts":[],"trend":null,"#,
            r#""voice":""#,
        )),
        "the world through the evidence parts is byte-stable"
    );
    // The voice and form tails are the plugin's own, so they are asserted
    // separately rather than pasted into the byte fixture above.
    let tail = world.split(r#""voice":""#).nth(1).unwrap();
    assert!(tail.starts_with("You are The Scout"));
    assert!(
        tail.contains(r#""form":{"keys":["body"],"max_chars":1200,"paragraph_max_chars":null}"#)
    );
    // The Scout records a non-participation on the paragraph rule, so the form
    // states the body ceiling and no paragraph ceiling. A plugin that adopts one
    // later changes this line, which is the point of pinning it.
    assert_eq!(
        crate::plugins::scout::cognition::prose()
            .dims
            .paragraph_max_chars,
        crate::plugins::scout::cognition::SCOUT_PARAGRAPH_MAX_CHARS
    );
}

#[test]
fn a_teams_world_omits_a_composite_it_does_not_have() {
    // A team has no position and this profile has no composite. Neither may be
    // manufactured: a missing composite is not a composite of 50, and the
    // selection rule that drops it is what stops a neutral value appearing.
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
    let world = world(
        &req("FOOTBALL", "team", "Test FC"),
        &p,
        None,
        &RatingExclusions::default(),
    );
    let fresh = fresh(&world);
    assert!(fresh["composite"].is_null());
    assert_eq!(fresh["values"][0]["band"], serde_json::json!("strong"));
    assert_eq!(fresh["values"][0]["percentile"], serde_json::json!(78.0));
    // Position is a player concept; a team's world does not invent one.
    assert!(!world.contains("position"));
    assert!(!world.contains("Guard"));
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
            r#"{{"composite_score":67.0,"datapoints":[{{"cohort":null,"label":"Scoring","measure":"Scoring","pct":95.0,"sign":1,"value":24.0,"z":3.1}},{{"cohort":null,"label":"Defense","measure":"Defense","pct":40.0,"sign":1,"value":2.5,"z":-0.5}}],"position":"Guard","prompt_version":"{RATING_PROMPT_VERSION}","sample":{{}},"season":2025}}"#
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
        .parse(r#"{"body":"Scoring declined relative to peers."}"#)
        .unwrap_err();
    assert!(reversed.is::<crate::plugins::support::form::SurfaceError>());
    assert!(reversed.to_string().contains("evidence says it rose"));

    let accepted = parser
        .parse(r#"{"body":"Scoring rose while steals fell relative to peers."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("Scoring rose"));

    let grouped = parser
        .parse(r#"{"body":"Scoring, steals, and playmaking are below average, with relative standing improving across these areas."}"#)
        .unwrap_err();
    assert!(grouped.to_string().contains("Steals rose"));

    let stable_rebounder_directions =
        BTreeMap::from([("Rebounding".into(), RelativeDirection::Rose)]);
    let stable_rebounder = RatingRequestParser::new(
        "Rebounding comparison supplied.",
        &stable_rebounder_directions,
        &bands,
    )
    .parse(r#"{"body":"He remains a consistent rebounder."}"#)
    .unwrap_err();
    assert!(stable_rebounder
        .to_string()
        .contains("says Rebounding held"));
}

#[test]
fn request_parser_rewrites_an_unsourced_height() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new("A center for Portland.", &directions, &bands);
    let error = parser
        .parse(r#"{"body":"The 6'9\" center protects the rim."}"#)
        .unwrap_err();
    assert!(error.is::<crate::plugins::support::form::SurfaceError>());
    assert!(error.to_string().contains("invents height"));

    let possessive = parser
        .parse(r#"{"body":"LeVert’s scoring profile is measured."}"#)
        .expect("a typographic possessive is not a height")
        .expect("a reply");
    assert!(possessive.body.contains("LeVert’s"));
}

#[test]
fn request_parser_rewrites_numeric_values_absent_from_the_assignment() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new(
        "Rating: 6.74. Finishing: percentile 91.2. Relative standing rose by 3.5 percentile points.",
        &directions,
        &bands,
    );
    let invented = parser
        .parse(r#"{"body":"The rating is 3.71 after a 0.44 rise."}"#)
        .unwrap_err();
    assert!(invented.is::<crate::plugins::support::form::SurfaceError>());
    assert!(invented.to_string().contains("numeric value 3.71"));

    let grounded = parser
        .parse(r#"{"body":"The rating is 6.74 after a 3.5-point rise."}"#)
        .unwrap()
        .unwrap();
    assert!(grounded.body.contains("6.74"));
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
        .parse(r#"{"body":"The player shows consistent improvement across key metrics."}"#)
        .unwrap_err();
    assert!(blanket.to_string().contains("mixed directions"));

    let form = parser
        .parse(r#"{"body":"A downward trend is visible, but the player remains in strong form."}"#)
        .unwrap_err();
    assert!(form
        .to_string()
        .contains("both strong/rising and declining/falling"));
}

#[test]
fn request_parser_does_not_turn_percentile_movement_into_development() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new(
        r#"{"fresh":{"supports_cross_season":true}}"#,
        &directions,
        &bands,
    );
    let error = parser
        .parse(r#"{"body":"This reflects consistent development across the season."}"#)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("does not establish player development"));
}

#[test]
fn request_parser_keeps_xg_and_xa_attached_to_their_measures() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let parser = RatingRequestParser::new("xG and xA evidence supplied.", &directions, &bands);
    let xg = parser
        .parse(r#"{"body":"His xG indicates strong creation."}"#)
        .unwrap_err();
    assert!(xg.to_string().contains("xG) is shooting/scoring evidence"));

    let xa = parser
        .parse(r#"{"body":"Expected assists show stronger finishing."}"#)
        .unwrap_err();
    assert!(xa.to_string().contains("xA) is creation evidence"));

    let grouped = parser
        .parse(r#"{"body":"His xG and xA are both at elite percentiles (99.2 and 97.0)."}"#)
        .unwrap_err();
    assert!(grouped.to_string().contains("separate claims"));

    let accepted = parser
        .parse(r#"{"body":"His xG supports the shooting read, while xA supports creation."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("xA supports creation"));
}

#[test]
fn request_parser_preserves_weighted_measures_and_thin_sample_coverage() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::new();
    let prompt = r#"{"fresh":{"supports_cross_season":false,"sample":{"appearances":3,"minutes":257},"values":[{"label":"Discipline","value":1,"measure":"yellow cards + 3 x red cards"}]}}"#;
    let parser = RatingRequestParser::new(prompt, &directions, &bands);
    let cards = parser
        .parse(r#"{"body":"Discipline is poor: 1 yellow + 3 red cards."}"#)
        .unwrap_err();
    assert!(cards.to_string().contains("weighted formula"));

    let games = parser
        .parse(r#"{"body":"Rogers played 3 games with 257 minutes."}"#)
        .unwrap_err();
    assert!(games.to_string().contains("source coverage"));

    let bare_sample = parser
        .parse(r#"{"body":"Rogers has 3 appearances and 257 minutes."}"#)
        .unwrap_err();
    assert!(bare_sample.to_string().contains("source coverage"));

    let stability = parser
        .parse(r#"{"body":"No change in relative standing is indicated; he remains reliable and consistently elite."}"#)
        .unwrap_err();
    assert!(stability
        .to_string()
        .contains("no computed cross-season direction"));

    let psychology = parser
        .parse(r#"{"body":"His reported frustration may influence his decision-making under pressure."}"#)
        .unwrap_err();
    assert!(psychology.to_string().contains("psychological inference"));

    // The retired thin-sample boundary told the model to stay under 800
    // characters. That was an instruction the model was asked to honour, and it
    // is gone: the shared body ceiling now governs every Scout body, and a thin
    // sample is bounded by the `limit` in its world rather than by a length the
    // model had to remember. The ceiling is still enforced, and it is the shared
    // one.
    let long_body = "x".repeat(crate::plugins::support::form::BODY_MAX_CHARS + 1);
    let oversized = parser
        .parse(&serde_json::json!({"body": long_body}).to_string())
        .unwrap_err();
    assert!(oversized.to_string().contains("characters"));

    let accepted = parser
        .parse(r#"{"body":"The stored snapshot records 3 appearances and 257 minutes. Discipline ranks poorly."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("stored snapshot"));
}

#[test]
fn thin_sample_stability_is_claim_shaped_not_word_shaped() {
    let directions = BTreeMap::new();
    let bands = BTreeMap::from([("Goals Against".into(), "elite".into())]);
    let prompt = r#"{"fresh":{"supports_cross_season":false,"values":[{"label":"Goals Against","value":1,"percentile":100.0,"band":"elite"}]}}"#;
    let parser = RatingRequestParser::new(prompt, &directions, &bands);

    // Within-snapshot consistency is a description of standing, not a cross-time claim.
    let snapshot = parser
        .parse(r#"{"body":"The team maintains consistent defensive discipline in this stored snapshot."}"#)
        .unwrap()
        .unwrap();
    assert!(snapshot.body.contains("consistent defensive discipline"));

    // Reaching across time is still rejected.
    let cross_season = parser
        .parse(r#"{"body":"He stayed stable compared with last season."}"#)
        .unwrap_err();
    assert!(cross_season
        .to_string()
        .contains("no computed cross-season direction"));

    // A direction verb tied to a named measure has no per-measure trend evidence.
    let measure_trend = parser
        .parse(r#"{"body":"Goals Against has declined recently."}"#)
        .unwrap_err();
    assert!(measure_trend.to_string().contains("no per-measure trend"));

    // The supplied overall-score trend line is not a measure, so it stays free.
    let supplied_trend = parser
        .parse(r#"{"body":"The overall scores are trending down over recent games."}"#)
        .unwrap()
        .unwrap();
    assert!(supplied_trend.body.contains("trending down"));

    // One-appearance assignments carry the same no-comparison contract.
    let one = RatingRequestParser::new(
        r#"{"fresh":{"supports_cross_season":false,"limit":{"kind":"one_appearance","appearances":1}}}"#,
        &directions,
        &bands,
    )
    .parse(r#"{"body":"Unchanged from the previous season."}"#)
    .unwrap_err();
    assert!(one.to_string().contains("cross-season"));
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
        .parse(r#"{"body":"Tackling is above average, while Chance Creation is elite."}"#)
        .unwrap_err();
    assert!(error.to_string().contains("Tackling above average"));
    assert!(error.to_string().contains("below average"));

    let accepted = parser
        .parse(r#"{"body":"Tackling is below average, while Chance Creation is elite."}"#)
        .unwrap()
        .unwrap();
    assert!(accepted.body.contains("Chance Creation is elite"));
}

#[test]
fn a_declined_body_is_a_pass_and_a_dropped_slot_is_not() {
    // `body: null` is the Scout declining to publish prose. `{}` is a dropped
    // slot, which is a contract violation. The two must not collapse — the
    // whole point of a declared abstention is that it is distinguishable.
    assert!(RatingParser.parse(r#"{"body": null}"#).unwrap().is_none());
    assert!(RatingParser.parse(r#"{}"#).is_err());
    assert!(RatingParser.parse(r#"{"headline": "A title"}"#).is_err());
    assert!(RatingParser.parse(r#"{"body": ""}"#).is_err());
    // A declined body cannot smuggle undeclared fields through the decoder.
    assert!(RatingParser.parse(r#"{"body": null, "score": 3}"#).is_err());
}

#[test]
fn claim_paragraphs_survive_the_production_parser() {
    // A multi-paragraph read is the normal shape, and the shared validator
    // measures paragraphs rather than lines: a wrapped line is not a break.
    let body = "The profile is ordinary. Most skills sit near average. The middle is the story.\n\nOne edge stands out. Finishing leads the supplied profile. That is the exception.\n\nAvailability is limited. Two absences are recorded. Depth matters now.\n\nThe rest is unchanged. The supplied comparison shows no movement. Continuity holds.";
    let parsed = RatingParser
        .parse(&serde_json::json!({"body": body}).to_string())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.body, body);
}

#[test]
fn a_thin_sample_states_its_boundary_instead_of_describing_it() {
    let mut p = profile_player();
    p.sample.insert("appearances".to_string(), 3.0);
    assert!(!supports_cross_season_comparison(&p));
    let world = world(
        &req("FOOTBALL", "player", "Test Player"),
        &p,
        None,
        &RatingExclusions::default(),
    );
    let profile_part = fresh(&world);
    // The boundary is a TYPED part of the world, not a paragraph the model has
    // to obey. The plugin decides the limit; the model is told which limit it is.
    assert_eq!(
        profile_part["limit"]["kind"],
        serde_json::json!("thin_sample")
    );
    assert_eq!(profile_part["limit"]["appearances"], serde_json::json!(3.0));
    assert_eq!(profile_part["limit"]["minimum"], serde_json::json!(10.0));
    // A thin sample has no cross-season support, so no prior percentile may be
    // presented even when a comparison would otherwise be available.
    assert_eq!(
        profile_part["supports_cross_season"],
        serde_json::json!(false)
    );
    assert!(profile_part["values"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.get("prior_percentile").is_none_or(|p| p.is_null())));
    // Participation totals are withheld from the sample: a stored count is
    // coverage, and presenting it as a figure invites the model to read it as
    // playing time. The count is not lost — the limit states it.
    assert!(profile_part["sample"].get("appearances").is_none());
    assert_eq!(profile_part["limit"]["appearances"], serde_json::json!(3.0));
    // The manual says what the boundary forbids, in the same terms the guard
    // enforces. A limit in the world that the manual does not describe is a
    // limit the model will talk past.
    let manual = crate::plugins::scout::cognition::prompt::TASK
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(manual.contains("source coverage, not proof of playing time"));
    assert!(manual.contains("Absence of a comparison is not stability"));

    p.sample.insert("appearances".to_string(), 10.0);
    assert!(supports_cross_season_comparison(&p));
}

#[test]
fn thin_sample_selection_omits_discipline_in_code() {
    let mut p = profile_player();
    p.sample.insert("appearances".to_string(), 3.0);
    // Elite Discipline among the two highest percentiles: the old boundary handed it to
    // the model with an order to omit; selection now withholds it in code.
    p.breakdown.push(dp("Discipline", 0.2, 1.9, 95.0, -1));
    p.breakdown.push(dp("Passing", 9.0, 0.5, 80.0, 1));

    assert_eq!(thin_sample_omitted_stat_labels(&p), vec!["Discipline"]);
    let prompt_profile = model_prompt_profile(&p, false, None);
    assert_eq!(prompt_profile.breakdown.len(), 2);
    assert!(prompt_profile
        .breakdown
        .iter()
        .all(|datapoint| datapoint.label != "Discipline"));
    assert_eq!(prompt_profile.breakdown[0].label, "Scoring");
    assert_eq!(prompt_profile.breakdown[1].label, "Passing");

    // A full sample keeps Discipline available.
    p.sample.insert("appearances".to_string(), 12.0);
    assert!(thin_sample_omitted_stat_labels(&p).is_empty());
    let full_prompt_profile = model_prompt_profile(&p, true, None);
    assert!(full_prompt_profile
        .breakdown
        .iter()
        .any(|datapoint| datapoint.label == "Discipline"));
}

#[test]
fn thin_current_sample_shows_only_its_two_highest_ranked_measures() {
    let mut p = profile_player();
    p.sample.insert("appearances".to_string(), 3.0);
    p.breakdown.push(dp("Passing", 9.0, 0.5, 80.0, 1));

    let prompt_profile = model_prompt_profile(&p, false, None);
    assert_eq!(prompt_profile.composite_score, None);
    assert_eq!(prompt_profile.breakdown.len(), 2);
    assert_eq!(prompt_profile.breakdown[0].label, "Scoring");
    assert_eq!(prompt_profile.breakdown[1].label, "Passing");
    assert_eq!(prompt_profile.sample["appearances"], 3.0);
}

#[test]
fn unselected_measures_are_named_not_silent() {
    let p = profile_player();
    let subject = req("FOOTBALL", "player", "Test Player");
    let bare = world(&subject, &p, None, &RatingExclusions::default());
    assert!(fresh(&bare).get("not_selected").is_none_or(|n| n.is_null()));

    let exclusions = RatingExclusions {
        budget_truncated_stat_labels: vec!["Long Throws".into(), "Aerial Duels".into()],
        off_facet_stat_labels: vec!["Tackling".into()],
        degenerate_zero_stat_labels: vec!["Ground Yards Responsible".into()],
        display_tier_stat_labels: vec!["Progression".into()],
        thin_sample_omitted_stat_labels: Vec::new(),
    };
    let world = world(&subject, &p, None, &exclusions);
    let fresh = fresh(&world);
    // A measure that exists but was not selected must be NAMED, or the model
    // reads its absence as "unmeasured" or "zero at the source".
    let not_selected: Vec<String> = fresh["not_selected"]
        .as_array()
        .expect("unselected labels must be named")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        not_selected,
        vec![
            "Aerial Duels",
            "Ground Yards Responsible",
            "Long Throws",
            "Progression",
            "Tackling",
        ],
        "one deduplicated, ordered list; reasons stay in the ledger"
    );
    // The reasons themselves do not reach the model: they are plugin bookkeeping.
    assert!(!world.contains("budget"));
    assert!(!world.contains("artifact"));
}

#[test]
fn comparative_model_profile_keeps_changes_and_strongest_held_anchors() {
    let labels = [
        "Rise A",
        "Rise B",
        "Fall A",
        "Fall B",
        "Held High",
        "Held Mid",
        "Held Low",
    ];
    let current = nfl_profile(
        "Center",
        labels
            .iter()
            .zip([90.0, 80.0, 20.0, 30.0, 99.0, 85.0, 50.0])
            .map(|(label, pct)| dp(label, 1.0, 0.0, pct, 1))
            .collect(),
    );
    let prior = nfl_profile(
        "Center",
        labels
            .iter()
            .zip([10.0, 20.0, 80.0, 70.0, 99.2, 85.0, 50.0])
            .map(|(label, pct)| dp(label, 1.0, 0.0, pct, 1))
            .collect(),
    );
    let changes = build_skill_changes(&current, &prior);
    let prompt_profile = model_prompt_profile(&current, true, Some(&changes));
    let labels = prompt_profile
        .breakdown
        .iter()
        .map(|datapoint| datapoint.label.as_str())
        .collect::<HashSet<_>>();

    assert_eq!(labels.len(), 6);
    assert!(labels.contains("Rise A"));
    assert!(labels.contains("Fall B"));
    assert!(labels.contains("Held High"));
    assert!(labels.contains("Held Mid"));
    assert!(!labels.contains("Held Low"));
}

/// No changes ⇒ no section. A heading with nothing under it asserts "nothing moved", which is a
/// claim the adjudication chain has not made — it may only mean nothing has been adjudicated yet.
// --- Studio boundary: prepared creation runs without Postgres, queues, or model hosts ---------
use crate::studio::model::{GenerateResult, Inference};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

struct FakeModel {
    response: String,
    fail: bool,
    calls: Mutex<Vec<(String, GenerateOptions)>>,
}

impl FakeModel {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
            fail: false,
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Inference for FakeModel {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        self.calls
            .lock()
            .unwrap()
            .push((prompt.to_string(), opts.clone()));
        if self.fail {
            anyhow::bail!("model unavailable");
        }
        Ok((
            GenerateResult {
                response: self.response.clone(),
                thinking: "private".to_string(),
                model: "model-that-answered".to_string(),
                total_duration: Duration::from_millis(25),
                prompt_eval_count: 20,
                eval_count: 12,
                completion_reason: Some("stop".to_string()),
                raw_response_body: "{}".to_string(),
            },
            serde_json::json!({"actual_request": true, "prompt": prompt}),
        ))
    }

    fn model(&self) -> &str {
        "configured-model"
    }

    fn request_body(&self, _: &str, _: &GenerateOptions) -> serde_json::Value {
        panic!("creation provenance must use the request actually sent")
    }
}

fn assignment() -> Assignment {
    Assignment {
        subject: Subject {
            entity_type: "player".to_string(),
            entity_name: "Vale Kerr".to_string(),
            sport: "NBA".to_string(),
            sport_name: String::new(),
        },
        season: 2026,
        notability: 72,
        notability_components: serde_json::json!({"top_pct": 91.0}),
        rating_trajectory: RatingTrajectory {
            key: "rising".to_string(),
            label: Some("overall scores trending up over recent games".to_string()),
            components: serde_json::json!({"sample_size": 5}),
        },
        input_components: serde_json::json!({"season": 2026}).to_string(),
        input_hash: "prepared-hash".to_string(),
        exclusions: RatingExclusions::default(),
        opts: GenerateOptions {
            system: Some(crate::plugins::scout::cognition::prompt::TASK.to_string()),
            temperature: Some(RATING_TEMPERATURE),
            num_predict: RATING_NUM_PREDICT,
            num_ctx: 4096,
            json_mode: false,
            format_schema: Some(prose().schema()),
            format_schema_raw: None,
        },
        built_prompt: scout_parts().render(),
        parts: scout_parts(),
    }
}

/// The world behind [`assignment`].
///
/// A small world with an elite, source-selected measurement.
fn scout_parts() -> crate::plugins::scout::cognition::parts::Parts {
    crate::plugins::scout::cognition::parts::Parts {
        subject: crate::plugins::meta::EntityMeta {
            name: "Vale Kerr".into(),
            entity_type: "player".into(),
            entity_id: 9,
            sport: "NBA".into(),
        },
        sport_name: "Basketball".into(),
        season: 2026,
        profile: crate::plugins::scout::cognition::parts::Profile {
            season: 2026,
            observed_at: None,
            sport_name: None,
            sample: BTreeMap::new(),
            values: vec![crate::plugins::scout::cognition::parts::MeasuredValue {
                label: "Blocks Per Game".into(),
                measure: "blocks_per_game".into(),
                value: None,
                percentile: Some(91.0),
                cohort: None,
                band: Some("elite".into()),
                quality_z: None,
                prior_percentile: None,
            }],
            composite: None,
            supports_cross_season: false,
            not_selected: Vec::new(),
            limit: None,
        },
        rate_standouts: Vec::new(),
        trend: None,
        memory: Default::default(),
    }
}

#[tokio::test]
async fn prepared_assignment_creates_without_application_services() {
    // The model now articulates the assembled world, so it returns the keyed
    // prose map this plugin declares rather than palette indexes.
    let body = "Vale Kerr's blocked shots rank elite in this profile at the 91st percentile, \
               and the stored sample is what that standing rests on.";
    let model = FakeModel::new(&serde_json::json!({"body": body}).to_string());
    let output = create(&Studio::new(&model), assignment()).await.unwrap();
    assert_eq!(output.body.as_deref(), Some(body));
    // The title is the plugin's own: it names the entity and the kind of read,
    // which is a fact rather than something to articulate.
    assert_eq!(
        output.headline.as_deref(),
        Some("Vale Kerr: measured profile")
    );
    assert_eq!(output.provenance.model_version, "model-that-answered");
    assert_eq!(
        output.provenance.input_hash.as_deref(),
        Some("prepared-hash")
    );
    // Provenance must describe the request actually sent, which is the assembled
    // world — not a captured string and not the palette menu.
    let request = &output.call.as_ref().unwrap().request_body;
    assert_eq!(request["actual_request"], true);
    assert_eq!(request["prompt"], assignment().built_prompt);
}

#[test]
fn no_stats_and_unchanged_are_explicit_uncalled_results() {
    let marker = no_stats(2026, "configured-model");
    assert!(marker.skipped_no_stats);
    assert!(!marker.skipped_unchanged);
    assert!(marker.body.is_none());
    assert!(marker.call.is_none());

    let skipped = unchanged(assignment(), "configured-model");
    assert!(!skipped.skipped_no_stats);
    assert!(skipped.skipped_unchanged);
    assert_eq!(skipped.rating_trajectory.as_deref(), Some("rising"));
    assert_eq!(
        skipped.provenance.input_hash.as_deref(),
        Some("prepared-hash")
    );
    assert!(skipped.call.is_none());
}

#[tokio::test]
async fn model_failure_cannot_become_a_scout_product() {
    let mut model = FakeModel::new("");
    model.fail = true;
    let error = create(&Studio::new(&model), assignment())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("model unavailable"));
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_called_pass_is_a_product_rather_than_an_error() {
    // An explicit null body is the Scout declining, and a decline that completed
    // is a result: it publishes a marker saying so. It is not a missing profile
    // (an uncalled marker) and not a malformed card (an error). The palette path
    // could not express this at all — it had no null to accept.
    let model = FakeModel::new(r#"{"body": null}"#);
    let output = create(&Studio::new(&model), assignment()).await.unwrap();
    assert!(output.abstained);
    assert!(output.body.is_none());
    assert!(
        !output.skipped_no_stats,
        "a decline is not a missing profile"
    );
    assert!(!output.skipped_unchanged);
    // The measurement still stands: declining to articulate is not declining to
    // measure, and the provenance has to reflect that.
    assert_eq!(output.notability, Some(72));
    assert_eq!(output.rating_trajectory.as_deref(), Some("rising"));
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

#[test]
fn incompatible_history_does_not_erase_current_measurements() {
    let mut current = profile_player();
    current.league_id = Some(1);
    for changed_league in [true, false] {
        let mut prior = current.clone();
        prior.season -= 1;
        prior.league_id = if changed_league {
            Some(2)
        } else {
            current.league_id
        };
        if !changed_league {
            for d in &mut prior.breakdown {
                d.measure = "retired measurement".into();
            }
        }
        let changes = build_skill_changes(&current, &prior);
        assert!(changes.is_empty());
        let subject = req("NBA", "player", "Test Player");
        let baseline = world(&subject, &current, None, &RatingExclusions::default());
        // An empty comparison set must leave the current measurements exactly as
        // they were. If it does not, a rejected history is erasing live evidence.
        let actual = world(
            &subject,
            &current,
            Some(&changes),
            &RatingExclusions::default(),
        );
        assert_eq!(
            actual, baseline,
            "incompatible history erased current evidence"
        );
    }
}

#[test]
fn absent_measurements_and_withheld_unidentified_measurements_have_distinct_context() {
    let mut profile = profile_player();
    for d in &mut profile.breakdown {
        d.measure.clear();
    }
    let subject = req("NBA", "player", "Test Player");
    // Measurements exist but their identity is withheld: the world says so, so
    // the model uses only what is named rather than inventing a measure.
    let withheld = world(&subject, &profile, None, &RatingExclusions::default());
    assert_eq!(
        fresh(&withheld)["limit"]["kind"],
        serde_json::json!("withheld_identity"),
        "an unidentified measurement is not a usable one"
    );
    // No measurements at all is a different situation, and it must read
    // differently: there is nothing to withhold an identity from.
    profile.breakdown.clear();
    let absent = world(&subject, &profile, None, &RatingExclusions::default());
    assert_eq!(
        fresh(&absent)["limit"]["kind"],
        serde_json::json!("no_measurements")
    );
    assert!(fresh(&absent)["values"].as_array().unwrap().is_empty());
    // The composite is withheld too, because a sample that cannot support a
    // comparison cannot support a composite either — and a withheld composite
    // must not be read as an average one.
    assert_eq!(fresh(&absent)["composite"], serde_json::Value::Null);
}

#[test]
fn the_header_carries_the_curated_sport_display_name() {
    let mut subject = req("FOOTBALL", "team", "Arsenal");
    subject.sport_name = "Football (Soccer)".into();
    // The curated display name is what the model is shown, so it is never left
    // to guess what a sport id means. It is plugin policy, carried in the world.
    let world = world(
        &subject,
        &profile_player(),
        None,
        &RatingExclusions::default(),
    );
    assert_eq!(
        fresh(&world)["sport_name"],
        serde_json::json!("Football (Soccer)")
    );
}
