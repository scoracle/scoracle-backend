#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; pure read-only SQL, no schema needed"]
async fn sql_activity_matches_legacy_runs_stages_publishers_and_caps() -> Result<()> {
    let mut connection = sqlx::Connection::connect(
        &std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"),
    )
    .await?;
    let stages = [
        "speculation",
        "concrete_interest",
        "advanced_talks",
        "here_we_go",
    ];
    let names = [
        "Wire", "WIRE", " Wire ", "ΣΟΣ", "σος", "İ", "i\u{307}", "Other",
    ];
    let sources = (0..32)
        .map(|i| SourceContext {
            classifier_world: None,
            classification_id: i,
            article_id: i,
            headline: String::new(),
            context: String::new(),
            source: names[i as usize % names.len()].into(),
            published_at_epoch: Some(100 - i),
        })
        .collect::<Vec<_>>();
    let mut cases = 0;
    // Every denial/report pattern through six reports, including a restart
    // after a denial; model finding order must never replace source order.
    for count in 0..=6 {
        for mask in 0..(1 << count) {
            for stage in 0..stages.len() {
                let findings = (0..count)
                    .map(|i| {
                        let status = if mask & (1 << i) == 0 {
                            Status::Reported
                        } else {
                            Status::Denied
                        };
                        Finding {
                            report_index: i,
                            counterparty: "Club".into(),
                            status,
                            stage: (status == Status::Reported)
                                .then(|| stages[(stage + i) % stages.len()].into()),
                            evidence_quote: "Exact quote".into(),
                        }
                    })
                    .collect::<Vec<_>>();
                for order in 0..3 {
                    let mut findings = findings.clone();
                    if order == 1 {
                        findings.reverse();
                    }
                    if order == 2 && findings.len() > 1 {
                        findings.rotate_left(1);
                    }
                    let reply = Reply {
                        headline: String::new(),
                        card_score: 0,
                        body: String::new(),
                        findings,
                    };
                    assert_eq!(
                        rumor_heat(&mut connection, &reply, &sources).await?,
                        reference_rumor_heat(&reply, &sources),
                        "count={count} mask={mask} stage={stage} order={order}"
                    );
                    cases += 1;
                }
            }
        }
    }
    // Publisher bonus/cap and identity match the original lowercase-only
    // comparison; whitespace and Unicode are deliberately not SQL-normalized.
    for count in 1..=32 {
        for stage in stages {
            for unique in [false, true] {
                let mut sources = sources.clone();
                if unique {
                    for (i, source) in sources.iter_mut().enumerate() {
                        source.source = format!("Wire {i}");
                    }
                }
                let mut findings = (0..count)
                    .map(|i| Finding {
                        report_index: i,
                        counterparty: "Club".into(),
                        status: Status::Reported,
                        stage: Some(stage.into()),
                        evidence_quote: "Exact quote".into(),
                    })
                    .collect::<Vec<_>>();
                // These are separate exact-name counterparties, each with
                // its own denial cutoff, but publisher bonus is global.
                findings.extend([
                    Finding {
                        report_index: 0,
                        counterparty: "club".into(),
                        status: Status::Denied,
                        stage: None,
                        evidence_quote: "Exact quote".into(),
                    },
                    Finding {
                        report_index: 1,
                        counterparty: "club".into(),
                        status: Status::Reported,
                        stage: Some("here_we_go".into()),
                        evidence_quote: "Exact quote".into(),
                    },
                    Finding {
                        report_index: 2,
                        counterparty: "Club ".into(),
                        status: Status::Reported,
                        stage: Some(stage.into()),
                        evidence_quote: "Exact quote".into(),
                    },
                ]);
                findings.reverse();
                let reply = Reply {
                    headline: String::new(),
                    card_score: 0,
                    body: String::new(),
                    findings,
                };
                assert_eq!(
                    rumor_heat(&mut connection, &reply, &sources).await?,
                    reference_rumor_heat(&reply, &sources)
                );
                let group = reply
                    .findings
                    .iter()
                    .filter(|f| f.counterparty == "Club")
                    .collect::<Vec<_>>();
                let (lead, selected) = current_run(&group);
                assert_eq!(lead.status, Status::Reported);
                let selected_reply = Reply {
                    headline: String::new(),
                    card_score: 0,
                    body: String::new(),
                    findings: selected.into_iter().cloned().collect(),
                };
                assert_eq!(
                    rumor_heat(&mut connection, &selected_reply, &sources).await?,
                    reference_rumor_heat(&selected_reply, &sources)
                );
                cases += 2;
            }
        }
    }
    let missing_source = Reply {
        headline: String::new(),
        card_score: 0,
        body: String::new(),
        findings: vec![Finding {
            report_index: sources.len(),
            counterparty: "Club".into(),
            status: Status::Reported,
            stage: Some("speculation".into()),
            evidence_quote: "Exact quote".into(),
        }],
    };
    assert!(rumor_heat(&mut connection, &missing_source, &sources)
        .await
        .is_err());
    eprintln!("{cases} SQL/legacy parity cases verified; missing source rejected");
    Ok(())
}
