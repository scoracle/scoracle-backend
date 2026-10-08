use super::*;

fn controls() -> Vec<(Source, Record)> {
    let sources: BTreeMap<_, Source> = include_str!("../../../fixtures/classifier/controls.jsonl")
        .lines()
        .map(|line| {
            let source: Source = serde_json::from_str(line).unwrap();
            (source.article_id, source)
        })
        .collect();
    include_str!("../../../fixtures/classifier/qualified-claims-controls.jsonl")
        .lines()
        .map(|line| {
            let record: Record = serde_json::from_str(line).unwrap();
            (sources[&record.article_id].clone(), record)
        })
        .collect()
}

#[test]
fn source_qualification_and_world_contract() {
    let mut packets = BTreeMap::new();
    for (source, record) in controls() {
        validate(&source, &record).unwrap();
        let prepared = request(&source, &record.target).unwrap();
        assert_eq!(prepared["input"]["body"], source.body);
        assert_eq!(
            prepared["source_identity"]["body_sha256"],
            record.body_sha256
        );
        assert!(!prepared.to_string().contains("review_focus"));
        // Turn retained fictional annotations into model-shaped replies to test the native
        // boundary. This is a contract check, not an extraction accuracy measurement.
        let mut reply = serde_json::to_value(&record).unwrap();
        fn quotes(body: &str, value: &mut Value) {
            match value {
                Value::Object(object) if object.contains_key("quote") => {
                    let text = object["quote"].as_str().unwrap().to_string();
                    let start = object["start"].as_u64().unwrap() as usize;
                    let occurrence = body
                        .match_indices(&text)
                        .position(|(offset, _)| offset == start)
                        .unwrap();
                    *value = json!({"quote": text, "occurrence": occurrence});
                }
                Value::Object(object) => {
                    for child in object.values_mut() {
                        quotes(body, child);
                    }
                }
                Value::Array(array) => {
                    for child in array {
                        quotes(body, child);
                    }
                }
                _ => {}
            }
        }
        quotes(&source.body, &mut reply["claims"]);
        let proposal = json!({"complete_source_review": true, "extraction_usable": true, "claims": reply["claims"]});
        let bound = qualify(&source, &record.target, &proposal.to_string()).unwrap();
        assert_eq!(
            serde_json::to_value(&bound).unwrap(),
            serde_json::to_value(&record).unwrap()
        );
        let packet = emotional_world(&source, &bound).unwrap();
        assert_eq!(packet["production_eligible"], false);
        let world = &packet["expression_world"];
        assert!(world.get("CLASSIFIER MEASUREMENTS").is_none());
        assert!(world.get("CLAIM REVIEW").is_none());
        for context in world["SOURCE CONTEXT"].as_array().into_iter().flatten() {
            assert!(source.body.contains(context.as_str().unwrap()));
        }
        let mut drift = source.clone();
        drift.body.push(' ');
        assert!(validate(&drift, &bound).is_err());
        let mut wrong_target = bound.clone();
        wrong_target.target = json!({"name": "Someone else"});
        assert!(validate(&source, &wrong_target).is_err());
        let mut duplicate = bound.clone();
        duplicate.claims.push(duplicate.claims[0].clone());
        assert!(validate(&source, &duplicate).is_err());
        packets.insert(source.article_id, packet);
    }
    for id in [-1, -6, -10, -11] {
        assert_eq!(
            packets[&id]["selection_status"],
            "no_eligible_emotional_claims"
        );
    }
    for id in [-4, -7, -9] {
        assert_eq!(packets[&id]["selection_status"], "unresolved");
        assert!(packets[&id]["expression_world"].is_null());
    }
    assert_eq!(
        packets[&-5]["expression_world"]["QUALIFIED CLAIMS"][0]["time_scope"],
        "historical"
    );
    assert_eq!(
        packets[&-13]["expression_world"]["QUALIFIED CLAIMS"][0]["time_scope"],
        "current"
    );
    let correction = &packets[&-12]["expression_world"];
    assert_eq!(correction["QUALIFIED CLAIMS"].as_array().unwrap().len(), 1);
    assert!(correction["SOURCE CONTEXT"][0]
        .as_str()
        .unwrap()
        .contains("The first report said"));

    let (source, record) = controls()
        .into_iter()
        .find(|(source, _)| source.article_id == -3)
        .unwrap();
    assert!(qualify(&source, &record.target, "not json").is_err());
    assert!(qualify(
        &source,
        &record.target,
        r#"{"complete_source_review":false,"extraction_usable":true,"claims":[]}"#
    )
    .is_err());
    assert!(qualify(
        &source,
        &record.target,
        r#"{"complete_source_review":true,"extraction_usable":false,"claims":[]}"#
    )
    .is_err());
    for qualifier in ["speaker", "subject", "negation", "reported_event_time"] {
        let mut missing = record.clone();
        missing.claims[0].qualifiers.insert(qualifier.into(), None);
        assert!(validate(&source, &missing).is_err(), "{qualifier}");
    }
    for (relation, time) in [("unknown", "current"), ("direct_subject", "unknown")] {
        let mut unknown = record.clone();
        unknown.claims[0].target_relation = relation.into();
        unknown.claims[0].time_scope = time.into();
        let packet = emotional_world(&source, &unknown).unwrap();
        assert_eq!(packet["selection_status"], "unresolved");
        assert!(packet["expression_world"].is_null());
    }
    let body = "Équipe: Alex said Alex feels worried.\n\nThe denial is elsewhere.";
    let span = bind(
        body,
        Quote {
            quote: "Alex".into(),
            occurrence: 1,
        },
    )
    .unwrap();
    assert_eq!(span.start, body.rfind("Alex").unwrap());
    assert!(bind(
        body,
        Quote {
            quote: "Alex".into(),
            occurrence: 2
        }
    )
    .is_err());
    assert!(bind(
        body,
        Quote {
            quote: "invented".into(),
            occurrence: 0
        }
    )
    .is_err());
    assert!(span_valid(
        body,
        &Span {
            start: 1,
            end: 2,
            quote: "É".into()
        }
    )
    .is_err());
    let end = bind(
        body,
        Quote {
            quote: "denial".into(),
            occurrence: 0,
        },
    )
    .unwrap();
    assert_eq!(paragraphs(body, &[&span, &end]).len(), 2);
    let mut bad = record.clone();
    bad.claims[0].candidate_dimensions = vec!["injury".into()];
    assert!(validate(&source, &bad).is_err());
    bad = record.clone();
    bad.complete_source_review = false;
    assert!(validate(&source, &bad).is_err());
    bad = record.clone();
    bad.extraction_usable = false;
    assert!(validate(&source, &bad).is_err());
}
