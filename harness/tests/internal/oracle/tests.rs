#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only SQL, no schema needed"]
async fn sql_metrics_match_legacy_scores_convergence_omens_and_requests() -> Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"))
        .await?;
    let levels = [i32::MIN, 0, 35, 36, 69, 70, 100, i32::MAX];
    let sentiments = [i32::MIN, 0, 40, 41, 59, 60, 100, i32::MAX];
    let directions = [
        None,
        Some("rising"),
        Some("falling"),
        Some("steady"),
        Some("heating_up"),
        Some("cooling_off"),
        Some("unknown"),
    ];
    let impacts = [
        f64::NEG_INFINITY,
        -1.0,
        0.0,
        1.49,
        1.5,
        1.51,
        49.49999999999999,
        49.5,
        49.50000000000001,
        99.5,
        101.0,
        f64::INFINITY,
        f64::NAN,
        -f64::NAN,
    ];
    let mut cases = 0;
    for mask in 0..32 {
        for (r, level) in levels.into_iter().enumerate() {
            for (v, sentiment) in sentiments.into_iter().enumerate() {
                for (d, direction) in directions.into_iter().enumerate() {
                    let mut c = cards();
                    c.rating.as_mut().unwrap().notability = level;
                    c.vibe.as_mut().unwrap().sentiment = sentiment;
                    c.insider.as_mut().unwrap().score = levels[(r + v + d) % levels.len()];
                    c.narratives[0].impact = impacts[(r + v + d) % impacts.len()];
                    c.momentum.direction = direction.map(str::to_owned);
                    if mask & 1 == 0 {
                        c.narratives.clear();
                    }
                    if mask & 2 == 0 {
                        c.rating = None;
                    }
                    if mask & 4 == 0 {
                        c.vibe = None;
                    }
                    if mask & 8 == 0 {
                        c.momentum = SynthMomentum::default();
                    }
                    if mask & 16 == 0 {
                        c.insider = None;
                    }
                    let convergence = pillar_convergence(&build_pillar_divergence(
                        c.rating.as_ref(),
                        c.vibe.as_ref(),
                        &c.momentum,
                    ));
                    assert_eq!(
                        load_metrics(&pool, &c).await?,
                        (
                            crown_score(&c),
                            convergence,
                            compute_omen(convergence, &c.momentum)
                        ),
                        "mask={mask} level={level} sentiment={sentiment} direction={direction:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    let mut creations = 0;
    for mask in 0..32 {
        for variant in 0..4 {
            let mut c = cards();
            c.rating.as_mut().unwrap().notability = [35, 70, 69, 82][variant];
            c.vibe.as_mut().unwrap().sentiment = [40, 60, 59, 72][variant];
            c.momentum.direction = Some(["rising", "falling", "steady", "unknown"][variant].into());
            // Multiple unsorted narratives exercise strongest-impact selection
            // and the unchanged request's stable ordering/three-report limit.
            let original = c.narratives[0].clone();
            for impact in [20.0, 99.5, 75.0, 1.5] {
                let mut n = original.clone();
                n.impact = impact;
                c.narratives.push(n);
            }
            if variant % 2 != 0 {
                c.narratives.reverse();
            }
            if mask & 1 == 0 {
                c.narratives.clear();
            }
            if mask & 2 == 0 {
                c.rating = None;
            }
            if mask & 4 == 0 {
                c.vibe = None;
            }
            if mask & 8 == 0 {
                c.momentum = SynthMomentum::default();
            }
            if mask & 16 == 0 {
                c.insider = None;
            }
            let a = assignment(c);
            let model = || FakeModel {
                response: r#"{"reading":"Northbridge FC have gathered pace across the available readings.","headline":"Northbridge gathers pace"}"#,
                fail: false,
                calls: Mutex::new(vec![]),
            };
            let actual_model = model();
            let reference_model = model();
            let actual = create(&pool, &Studio::new(&actual_model), &a).await?;
            let expected = create_from_reference(&Studio::new(&reference_model), &a).await?;
            assert_eq!(
                (actual.score, actual.convergence, actual.omen),
                (expected.score, expected.convergence, expected.omen)
            );
            assert_eq!(actual.reading, expected.reading);
            assert_eq!(actual.headline, expected.headline);
            assert_eq!(actual.season, expected.season);
            assert_eq!(actual.input_components_json, expected.input_components_json);
            assert_eq!(actual.provenance.input_hash, expected.provenance.input_hash);
            assert_eq!(actual.was_called(), expected.was_called());
            assert_eq!(
                *actual_model.calls.lock().unwrap(),
                *reference_model.calls.lock().unwrap()
            );
            if let (Some(actual), Some(expected)) = (&actual.call, &expected.call) {
                assert_eq!(actual.request_body, expected.request_body);
            }
            creations += 1;
        }
    }
    eprintln!(
        "{cases} SQL/legacy metric cases and {creations} creation/request parity cases verified"
    );
    Ok(())
}
