#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only query, no schema needed"]
async fn sql_activity_matches_legacy_formula_and_served_product() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"))
        .await
        .unwrap();
    let now = 1_790_467_200;
    let publishers = [
        "Wire", "WIRE", " Wire ", "ΣΟΣ", "σος", "İ", "i\u{307}", "Other",
    ];
    let epochs = [
        None,
        Some(now + 1),
        Some(now),
        Some(now - 43200),
        Some(now - 43201),
        Some(now - 86400),
        Some(now - 86401),
        Some(now - 172800),
        Some(now - 172801),
        Some(i64::MIN),
        Some(i64::MAX),
    ];
    let mut cases = 0;
    for count in 0..=32 {
        for epoch in epochs {
            let corpus = (0..count)
                .map(|i| CorpusItem {
                    classifier_world: None,
                    id: i + 1,
                    title: String::new(),
                    context: format!("Report {i}."),
                    source: publishers[i as usize % publishers.len()].into(),
                    published_at_epoch: epoch,
                })
                .collect::<Vec<_>>();
            for clock in [now, i64::MIN, i64::MAX] {
                let actual = load(&pool, &corpus, clock).await.unwrap();
                let expected = reference(&corpus, clock);
                assert_eq!(
                    actual.card_score, expected.card_score,
                    "count={count} epoch={epoch:?} clock={clock}"
                );
                assert_eq!(actual.summary.score, expected.summary.score);
                assert_eq!(actual.summary.components, expected.summary.components);
                cases += 1;
            }
        }
    }
    // Mixed epochs exercise max(non-null), rather than the first selected report.
    let corpus = epochs
        .iter()
        .enumerate()
        .map(|(i, &epoch)| CorpusItem {
            classifier_world: None,
            id: i as i64 + 1,
            title: String::new(),
            context: format!("Report {i}."),
            source: publishers[i % publishers.len()].into(),
            published_at_epoch: epoch,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        load(&pool, &corpus, now).await.unwrap().card_score,
        reference(&corpus, now).card_score
    );
    let a = assignment(
        vec![CorpusItem {
            classifier_world: None,
            id: 1,
            title: "Cedar result".into(),
            context: "Cedar won.".into(),
            source: "Wire".into(),
            published_at_epoch: Some(now - 3600),
        }],
        now,
    );
    let model = Model(AtomicUsize::new(0));
    let output = journalist::create(&pool, &Studio::new(&model), &a, now, 4096)
        .await
        .unwrap();
    assert_eq!(output.card_score, Some(10));
    assert_eq!(output.narratives[0].impact, 32);
    assert_eq!(
        output.narratives[0].impact_components,
        reference(&a.selected, now).summary.components
    );
    assert_eq!(
        output.provenance.input_hash.as_deref(),
        Some(a.input_hash.as_str())
    );
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    eprintln!("{cases} SQL/legacy parity cases, mixed epochs and one served product verified");
}
