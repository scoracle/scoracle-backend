use super::*;
use crate::studio::decision::{ChoiceAnswer, DecisionRequest, DecisionResponse};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Classifier {
    calls: AtomicUsize,
    fail: bool,
}

#[async_trait::async_trait]
impl DecisionModel for Classifier {
    async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        anyhow::ensure!(!self.fail, "classifier unavailable");
        Ok(DecisionResponse {
            answers: request
                .questions
                .iter()
                .map(|(id, q)| {
                    let winner = match id.as_str() {
                        "relevance" | "journalist" | "scout" => "relevant",
                        _ => "irrelevant",
                    };
                    (
                        id.clone(),
                        ChoiceAnswer {
                            choice: winner.into(),
                            probabilities: q
                                .criteria
                                .keys()
                                .map(|k| (k.clone(), if k == winner { 1.0 } else { 0.0 }))
                                .collect(),
                        },
                    )
                })
                .collect(),
            provenance: json!({"model":"test", "revision":"test", "adapter":"test", "device":"test",
                "coverage":request.questions.keys().map(|k|(k.clone(),json!({"truncated":false}))).collect::<BTreeMap<_,_>>() }),
            raw_response: json!({"retained":"provider response"}),
        })
    }
}

fn article() -> Article {
    Article {
        article_id: 42,
        title: "Captain returns to training".into(),
        source: "Sports desk".into(),
        url: "https://example.test/story".into(),
        published_at: None,
        feed_rank: Some(2),
        description: String::new(),
        body: "  The captain returned today.\n\nA second paragraph.".into(),
        hypothesis: cognition::Hypothesis {
            name: "Example FC".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "FOOTBALL".into(),
        },
        baseline: json!({"teacher":"not model input"}),
    }
}

#[tokio::test]
async fn standalone_plugin_runs_the_cascade_and_produces_attributed_context() {
    let model = Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let plugin = Harvester::new(model.clone());
    let a = article();
    let packet = plugin.harvest(&a).await.unwrap();
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    assert_eq!(packet["plugin_id"], "scoracle.internal.harvester");
    assert_eq!(packet["disposition"], "accept");
    assert_eq!(
        packet["character_tags"],
        json!(["scoracle.character.narrative", "scoracle.character.rating"])
    );
    let start = packet["excerpt"]["start"].as_u64().unwrap() as usize;
    let end = packet["excerpt"]["end"].as_u64().unwrap() as usize;
    assert_eq!(packet["context"]["text"], &a.body[start..end]);
    assert_eq!(
        packet["raw_responses"]["character_routing"]["retained"],
        "provider response"
    );
    assert!(!packet.to_string().contains("not model input"));
    assert!(packet.get("baseline").is_none());
}

#[tokio::test]
async fn bad_inputs_and_provider_failures_never_become_rejected_articles() {
    let model = Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: true,
    });
    let plugin = Harvester::new(model.clone());
    let mut a = article();
    a.body.clear();
    assert!(plugin.harvest(&a).await.is_err());
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    let error = plugin.harvest(&article()).await.unwrap_err();
    assert!(error.to_string().contains("classifier unavailable"));
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn classification_requires_the_retained_publisher_opening() {
    let model = Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let plugin = Harvester::new(model.clone());
    let mut a = article();
    a.body.clear();
    assert!(plugin.classify(&a).await.is_err());
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn serialized_packet_preserves_unicode_headline_and_source_byte_ranges() {
    let plugin = Harvester::new(Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: false,
    }));
    let mut a = article();
    a.title = "Équipe: captain’s return 🏀".into();
    for body in [
        " \nÉquipe returned. “Ready?”\nYes! Fourth sentence.",
        "\tOnly one sentence — résumé.",
        "A short opening without punctuation",
    ] {
        a.body = body.into();
        let packet = plugin.harvest(&a).await.unwrap();
        let stored = serde_json::to_vec(&packet).unwrap();
        let decoded: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        assert_eq!(decoded["headline"], a.title);
        assert_eq!(decoded["context"]["headline"], a.title);
        for key in ["excerpt", "model_input_excerpt"] {
            let x = &decoded[key];
            let start = x["start"].as_u64().unwrap() as usize;
            let end = x["end"].as_u64().unwrap() as usize;
            assert_eq!(a.body.get(start..end), x["text"].as_str());
        }
        assert_eq!(decoded["context"]["text"], decoded["excerpt"]["text"]);
    }
}

#[tokio::test]
async fn compilation_rejects_changed_headline_entity_context_and_prompts() {
    let plugin = Harvester::new(Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: false,
    }));
    let original = article();
    let classified = plugin.classify(&original).await.unwrap();
    let mut changed = original.clone();
    changed.title = "An entirely different headline".into();
    assert!(plugin.compile(&changed, classified.clone()).is_err());
    changed = original.clone();
    changed.hypothesis.name = "Another club".into();
    assert!(plugin.compile(&changed, classified.clone()).is_err());
    changed = original.clone();
    changed.hypothesis.entity_id += 1;
    assert!(plugin.compile(&changed, classified.clone()).is_err());
    changed = original.clone();
    changed.article_id += 1;
    assert!(plugin.compile(&changed, classified.clone()).is_err());
    changed = original.clone();
    changed
        .body
        .push_str(" Changed text outside the classified opening.");
    assert!(plugin.compile(&changed, classified.clone()).is_err());
    let mut altered = classified.clone();
    altered.prepared.character_context = cognition::first_sentences(&original.body, 1);
    assert!(plugin.compile(&original, altered).is_err());
    let mut altered = classified.clone();
    altered
        .relevance_request
        .questions
        .get_mut("relevance")
        .unwrap()
        .instructions = "Changed prompt".into();
    assert!(plugin.compile(&original, altered).is_err());
    let mut altered = classified;
    altered.character_request.as_mut().unwrap().state = "A different article".into();
    assert!(plugin.compile(&original, altered).is_err());
}

#[tokio::test]
async fn malformed_or_truncated_results_never_compile_as_rejects() {
    let plugin = Harvester::new(Arc::new(Classifier {
        calls: AtomicUsize::new(0),
        fail: false,
    }));
    let a = article();
    let classified = plugin.classify(&a).await.unwrap();
    for stage in ["relevance", "journalist"] {
        for fault in [
            "truncation",
            "missing_coverage",
            "invalid_distribution",
            "missing_answer",
        ] {
            let mut altered = classified.clone();
            let response = if stage == "relevance" {
                &mut altered.relevance_response
            } else {
                altered.character_response.as_mut().unwrap()
            };
            match fault {
                "truncation" => response.provenance["coverage"][stage]["truncated"] = json!(true),
                "missing_coverage" => response.provenance["coverage"] = json!(null),
                "invalid_distribution" => {
                    let answer = response.answers.get_mut(stage).unwrap();
                    answer.choice = "irrelevant".into();
                    answer.probabilities.insert("irrelevant".into(), 2.0);
                }
                _ => {
                    response.answers.remove(stage);
                }
            }
            assert!(plugin.compile(&a, altered).is_err(), "{stage}: {fault}");
        }
    }
}
