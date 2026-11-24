#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only SQL, no schema needed"]
async fn sql_metrics_match_legacy_thresholds_and_creation() -> Result<()> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"))
        .await?;
    let mut scores = (-1000..=1000)
        .map(|i| Some(f64::from(i) / 10.0))
        .collect::<Vec<_>>();
    scores.extend([
        None,
        Some(-0.0),
        Some(f64::from_bits(1)),
        Some(-f64::from_bits(1)),
        Some(f64::MIN),
        Some(f64::MAX),
        Some(f64::NEG_INFINITY),
        Some(f64::INFINITY),
        Some(f64::NAN),
    ]);
    // Immediate floating-point neighbors detect accidental decimal rounding.
    for threshold in [5.0_f64, 10.0, 20.0, 35.0, 55.0, 80.0] {
        for bits in [
            threshold.to_bits() - 1,
            threshold.to_bits(),
            threshold.to_bits() + 1,
        ] {
            let value = f64::from_bits(bits);
            scores.extend([Some(value), Some(-value)]);
        }
    }
    for &score in &scores {
        assert_eq!(
            momentum_metrics(&pool, score).await?,
            (
                momentum_direction_from_score(score).into(),
                momentum_conviction_from_score(score)
            ),
            "score={score:?}"
        );
    }
    let mut creations = 0;
    // Every finished-card/trajectory availability combination retains the same
    // material decision, request/options, product and actual-call provenance.
    for mask in 0..8 {
        for score in [
            None,
            Some(-80.0),
            Some(-10.0),
            Some(0.0),
            Some(9.999999999999998),
            Some(10.0),
            Some(80.0),
        ] {
            let mut a = assignment();
            let rating = (mask & 1 != 0).then(|| a.context.rating.clone().unwrap());
            let vibe = (mask & 2 != 0).then(|| a.context.vibe.clone().unwrap());
            let mut snapshot = if mask & 4 != 0 {
                a.context.snapshot.clone()
            } else {
                Snapshot::default()
            };
            snapshot.momentum_score = score;
            a.context = MomentumContext::new(2026, rating, vibe, snapshot);
            let mut reply = serde_json::json!({"blurb":"Kerr's form and mood retain their separate windows.","headline":"Kerr finds a spark"});
            if score.is_none() {
                reply["estimated_score"] = 47.into();
            }
            let model = Model::new(&reply.to_string());
            let reference_model = Model::new(&model.response);
            let actual = create(&pool, &Studio::new(&model), &a).await?;
            let expected = create_from_reference(&Studio::new(&reference_model), &a).await?;
            assert_eq!(actual.is_none(), expected.is_none());
            let actual_calls = model.calls.lock().unwrap();
            let expected_calls = reference_model.calls.lock().unwrap();
            assert_eq!(actual_calls.len(), expected_calls.len());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert_eq!(
                    (actual.direction.as_str(), actual.score),
                    (expected.direction.as_str(), expected.score)
                );
                assert_eq!(actual.blurb, expected.blurb);
                assert_eq!(actual.headline, expected.headline);
                assert_eq!(actual.season, expected.season);
                assert_eq!(actual.input_components_json, expected.input_components_json);
                assert_eq!(actual.provenance.input_hash, expected.provenance.input_hash);
                if actual.was_called() {
                    assert_eq!(actual.provenance.model_version, "model-that-answered");
                    assert_eq!(
                        actual.call.as_ref().unwrap().request_body,
                        expected.call.as_ref().unwrap().request_body
                    );
                    assert_eq!(actual_calls.len(), 1);
                    assert_eq!(actual_calls[0].0, expected_calls[0].0);
                    assert_eq!(
                        format!("{:?}", actual_calls[0].1),
                        format!("{:?}", expected_calls[0].1)
                    );
                } else {
                    assert_eq!(actual.provenance.model_version, "configured-model");
                    assert!(actual_calls.is_empty());
                    assert!(actual.blurb.contains("cannot be assessed"));
                }
            }
            creations += 1;
        }
    }
    eprintln!(
        "{} SQL/legacy score cases and {creations} creation/request parity cases verified",
        scores.len()
    );
    Ok(())
}
