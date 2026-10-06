//! Cutover contract: exact publisher text plus advisory System 1 signals.
//!
//! This is not an editorial packet. No generated story, interpretation, or
//! character-owned decision is represented here.
use super::cognition::{self, Article, Excerpt};
use super::policy::{self, CHARACTER_ROUTES};
use crate::plugins::harvester::decision::{
    DecisionModel, DecisionRequest, DecisionResponse, ProbabilityAnswer,
};
use crate::plugins::meta::EntityMeta;
use crate::util::hash_components;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT: &str = "harvest-context-v7";
pub const HEADLINE_CONTRACT: &str = "harvest-headline-v3";
pub use policy::{CHARACTER_ROUTE_POLICY, HEADLINE_POLICY, HEADLINE_READ_THRESHOLD};

fn source_identity(article: &Article) -> Value {
    json!({"source": article.source, "url": article.url, "published_at": article.published_at})
}

#[derive(Clone, Debug)]
pub struct HeadlineGate {
    pub article_id: i64,
    pub headline: String,
    pub hypothesis: EntityMeta,
    pub request: DecisionRequest,
    pub response: DecisionResponse,
    pub input_hash: String,
    pub model_revision: String,
}

impl HeadlineGate {
    pub fn relevance_probability(&self) -> f64 {
        self.response.answers["relevance"].probability
    }

    pub fn admits_reading(&self) -> bool {
        policy::admits_headline(self.relevance_probability())
    }

    pub fn verify_against(&self, article: &Article) -> Result<()> {
        ensure!(
            self.article_id == article.article_id,
            "headline gate article changed"
        );
        ensure!(
            self.headline == article.title,
            "headline gate title changed"
        );
        ensure!(
            self.hypothesis == article.hypothesis,
            "headline gate entity changed"
        );
        let expected = cognition::prepare_relevance(article)?;
        ensure!(self.request == expected, "headline gate request changed");
        ensure!(
            self.input_hash == hash_components(&serde_json::to_string(&self.request)?),
            "headline gate input hash changed"
        );
        cognition::validate(&self.request, &self.response)?;
        ensure!(
            self.response.provenance["revision"].as_str() == Some(self.model_revision.as_str()),
            "headline gate model revision changed"
        );
        Ok(())
    }
}

pub async fn classify_headline(
    model: &dyn DecisionModel,
    article: &Article,
) -> Result<HeadlineGate> {
    let request = cognition::prepare_relevance(article)?;
    let response = model.evaluate(&request).await?;
    let model_revision = response.provenance["revision"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let gate = HeadlineGate {
        article_id: article.article_id,
        headline: article.title.clone(),
        hypothesis: article.hypothesis.clone(),
        input_hash: hash_components(&serde_json::to_string(&request)?),
        request,
        response,
        model_revision,
    };
    gate.verify_against(article)?;
    Ok(gate)
}

/// Each score is bound to the exact window the model actually received.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemePass {
    pub start: usize,
    pub end: usize,
    pub scores: BTreeMap<String, f64>,
    pub provenance: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarvestContext {
    pub contract_version: String,
    pub article_id: i64,
    pub headline: String,
    pub source: String,
    pub url: String,
    pub hypothesis: EntityMeta,
    pub feed_rank: Option<i32>,
    pub body_sha256: String,
    /// Complete scored source span; empty for rejected headlines.
    /// Actual individual windows are in provenance.
    pub model_input: Excerpt,
    pub context: Excerpt,
    /// Plugin admission (legacy SQL vocabulary), not a model-authored verdict.
    pub entity_choice: String,
    /// Maximum observed score per predicate across all exact source windows.
    pub predicate_scores: BTreeMap<String, f64>,
    pub recommended_characters: Vec<String>,
    pub model_provenance: Value,
    pub model_revision: String,
    pub headline_gate_input_hash: String,
}

fn route_policy() -> Value {
    json!(CHARACTER_ROUTES
        .iter()
        .map(|route| (
            route.destination.id.as_str(),
            route
                .predicates
                .iter()
                .map(|p| (p.key, p.threshold))
                .collect::<BTreeMap<_, _>>()
        ))
        .collect::<BTreeMap<_, _>>())
}

fn select_routes(scores: &BTreeMap<String, f64>) -> Vec<String> {
    CHARACTER_ROUTES
        .iter()
        .filter(|route| {
            route
                .predicates
                .iter()
                .any(|p| scores.get(p.key).is_some_and(|score| *score >= p.threshold))
        })
        .map(|route| route.destination.id.to_string())
        .collect()
}

fn aggregate(passes: &[ThemePass]) -> BTreeMap<String, f64> {
    let mut scores = BTreeMap::<String, f64>::new();
    for pass in passes {
        for (key, score) in &pass.scores {
            scores
                .entry(key.clone())
                .and_modify(|old| *old = old.max(*score))
                .or_insert(*score);
        }
    }
    scores
}

impl HarvestContext {
    pub fn verify_against(&self, article: &Article) -> Result<()> {
        ensure!(
            self.contract_version == CONTRACT,
            "unknown context contract"
        );
        ensure!(self.article_id == article.article_id, "article id changed");
        ensure!(self.headline == article.title, "headline changed");
        ensure!(
            self.source == article.source && self.url == article.url,
            "source changed"
        );
        ensure!(
            self.hypothesis == article.hypothesis,
            "query entity changed"
        );
        ensure!(self.feed_rank == article.feed_rank, "Google rank changed");
        ensure!(
            self.body_sha256 == hex::encode(Sha256::digest(article.body.as_bytes())),
            "publisher body changed"
        );
        let prepared = cognition::prepare_text(if self.entity_choice == "relevant" {
            &article.body
        } else {
            ""
        })?;
        ensure!(
            self.context == prepared.character_context,
            "character context is not the exact first three source paragraphs"
        );
        ensure!(
            self.model_input == self.context,
            "scored source span differs from delivered context"
        );
        let prov = &self.model_provenance;
        ensure!(
            prov["source_identity"] == source_identity(article),
            "source attribution or publication date changed"
        );
        ensure!(
            prov["headline_policy"] == HEADLINE_POLICY
                && prov["headline_read_threshold"] == HEADLINE_READ_THRESHOLD
                && prov["character_route_policy"] == CHARACTER_ROUTE_POLICY
                && prov["route_thresholds"] == route_policy()
                && prov["question_set_versions"]
                    == json!({"relevance": cognition::RELEVANCE_QUESTIONS, "character_routing": cognition::CHARACTER_QUESTIONS}),
            "context policy or predicate version changed"
        );
        let request = cognition::prepare_relevance(article)?;
        ensure!(
            self.headline_gate_input_hash == hash_components(&serde_json::to_string(&request)?)
                && prov["headline_gate_input_hash"] == self.headline_gate_input_hash,
            "headline gate input changed"
        );
        let probability = prov["headline_relevance_probability"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("missing headline probability"))?;
        let response = DecisionResponse {
            answers: BTreeMap::from([("relevance".into(), ProbabilityAnswer { probability })]),
            provenance: prov["relevance"].clone(),
            raw_response: Value::Null,
        };
        cognition::validate(&request, &response)?;
        ensure!(
            response.provenance["revision"].as_str() == Some(self.model_revision.as_str()),
            "context model revision changed"
        );
        let admitted = policy::admits_headline(probability);
        ensure!(
            self.entity_choice == if admitted { "relevant" } else { "irrelevant" },
            "context admission differs from plugin policy"
        );
        let passes: Vec<ThemePass> = serde_json::from_value(prov["theme_passes"].clone())?;
        if admitted {
            ensure!(
                !prepared.model_inputs.is_empty(),
                "missing admitted publisher body"
            );
            ensure!(
                passes.len() == prepared.model_inputs.len(),
                "incomplete publisher window coverage"
            );
            for (pass, input) in passes.iter().zip(&prepared.model_inputs) {
                ensure!(
                    pass.start == input.start && pass.end == input.end,
                    "theme window differs from exact source"
                );
                let theme_response = DecisionResponse {
                    answers: pass
                        .scores
                        .iter()
                        .map(|(key, probability)| {
                            (
                                key.clone(),
                                ProbabilityAnswer {
                                    probability: *probability,
                                },
                            )
                        })
                        .collect(),
                    provenance: pass.provenance.clone(),
                    raw_response: Value::Null,
                };
                cognition::validate(
                    &cognition::prepare_character_routing(article, input),
                    &theme_response,
                )?;
                ensure!(
                    pass.provenance["model"] == response.provenance["model"]
                        && pass.provenance["revision"] == response.provenance["revision"],
                    "context checkpoint changed between stages"
                );
            }
        } else {
            ensure!(passes.is_empty(), "irrelevant article has theme windows");
        }
        ensure!(
            self.predicate_scores == aggregate(&passes),
            "predicate scores differ from source windows"
        );
        ensure!(
            self.recommended_characters == select_routes(&self.predicate_scores)
                && prov["selected_characters"] == json!(self.recommended_characters),
            "Harvester destinations differ from theme decisions"
        );
        Ok(())
    }
}

pub async fn classify(model: &dyn DecisionModel, article: &Article) -> Result<HarvestContext> {
    let gate = classify_headline(model, article).await?;
    classify_after_headline(model, article, &gate).await
}

pub async fn classify_after_headline(
    model: &dyn DecisionModel,
    article: &Article,
    gate: &HeadlineGate,
) -> Result<HarvestContext> {
    gate.verify_against(article)?;
    let relevant = gate.admits_reading();
    let prepared = cognition::prepare_text(if relevant { &article.body } else { "" })?;
    let mut passes = Vec::new();
    if relevant {
        ensure!(
            !prepared.model_inputs.is_empty(),
            "missing publisher body after headline gate"
        );
        for input in &prepared.model_inputs {
            let request = cognition::prepare_character_routing(article, input);
            let response = model.evaluate(&request).await?;
            cognition::validate(&request, &response)?;
            ensure!(
                gate.response.provenance["model"] == response.provenance["model"]
                    && gate.response.provenance["revision"] == response.provenance["revision"],
                "System 1 checkpoint changed within one article"
            );
            passes.push(ThemePass {
                start: input.start,
                end: input.end,
                scores: response
                    .answers
                    .into_iter()
                    .map(|(key, answer)| (key, answer.probability))
                    .collect(),
                provenance: response.provenance,
            });
        }
    }
    let predicate_scores = aggregate(&passes);
    let recommended_characters = select_routes(&predicate_scores);
    let result = HarvestContext {
        contract_version: CONTRACT.into(),
        article_id: article.article_id,
        headline: article.title.clone(),
        source: article.source.clone(),
        url: article.url.clone(),
        hypothesis: article.hypothesis.clone(),
        feed_rank: article.feed_rank,
        body_sha256: hex::encode(Sha256::digest(article.body.as_bytes())),
        model_input: prepared.character_context.clone(),
        context: prepared.character_context,
        entity_choice: if relevant { "relevant" } else { "irrelevant" }.into(),
        predicate_scores,
        model_provenance: json!({
            "relevance": gate.response.provenance, "source_identity": source_identity(article),
            "headline_gate_input_hash": gate.input_hash,
            "headline_relevance_probability": gate.relevance_probability(),
            "headline_policy": HEADLINE_POLICY, "headline_read_threshold": HEADLINE_READ_THRESHOLD,
            "character_route_policy": CHARACTER_ROUTE_POLICY, "route_thresholds": route_policy(),
            "theme_passes": passes, "selected_characters": recommended_characters,
            "question_set_versions": {"relevance": cognition::RELEVANCE_QUESTIONS, "character_routing": cognition::CHARACTER_QUESTIONS},
        }),
        recommended_characters,
        model_revision: gate.model_revision.clone(),
        headline_gate_input_hash: gate.input_hash.clone(),
    };
    result.verify_against(article)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Stub {
        headline: f64,
        malformed: bool,
        late_only: bool,
        calls: AtomicUsize,
    }
    impl Default for Stub {
        fn default() -> Self {
            Self {
                headline: 0.8,
                malformed: false,
                late_only: false,
                calls: AtomicUsize::new(0),
            }
        }
    }
    #[async_trait]
    impl DecisionModel for Stub {
        async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(DecisionResponse {
                answers: request
                    .questions
                    .keys()
                    .map(|id| {
                        let probability = if id == "relevance" {
                            self.headline
                        } else if !self.late_only
                            || (id == "player_move" && request.state.contains("LATE_TRANSFER"))
                        {
                            0.8
                        } else {
                            0.05
                        };
                        (id.clone(), ProbabilityAnswer { probability })
                    })
                    .collect(),
                provenance: json!({"model":"stub","revision":"r1","adapter":"test","device":"cpu",
                    "coverage": request.questions.keys().map(|id| (id.clone(),json!({"truncated":self.malformed}))).collect::<BTreeMap<_,_>>() }),
                raw_response: Value::Null,
            })
        }
    }
    fn article() -> Article {
        Article {
            article_id: 4,
            title: "Équipe returns".into(),
            source: "Source".into(),
            url: "https://example.test/4".into(),
            published_at: None,
            feed_rank: Some(1),
            body: "  Équipe returned. Fans cheered! Third sentence? Fourth.".into(),
            hypothesis: EntityMeta {
                name: "Équipe".into(),
                entity_type: "team".into(),
                entity_id: 9,
                sport: "FOOTBALL".into(),
            },
        }
    }
    #[tokio::test]
    async fn entity_rejection_skips_all_theme_calls() {
        let model = Stub {
            headline: 0.2,
            ..Stub::default()
        };
        let mut a = article();
        // A rejected headline must never incur a body-coverage requirement.
        a.body = "unread ".repeat(901);
        let result = classify(&model, &a).await.unwrap();
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.entity_choice, "irrelevant");
        assert!(result.predicate_scores.is_empty());
        assert!(result.recommended_characters.is_empty());
        assert!(result.context.text.is_empty());
        result.verify_against(&a).unwrap();
    }
    #[tokio::test]
    async fn plugin_controls_admission_below_half_probability() {
        let model = Stub {
            headline: 0.3,
            ..Stub::default()
        };
        let result = classify(&model, &article()).await.unwrap();
        assert_eq!(result.entity_choice, "relevant");
        assert_eq!(result.recommended_characters.len(), 4);
        assert_eq!(result.predicate_scores.len(), 7);
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn routes_follow_predicate_specific_thresholds() {
        let mut scores = BTreeMap::from([("performance".into(), 0.69)]);
        assert!(select_routes(&scores).is_empty());
        scores.insert("performance".into(), 0.70);
        assert_eq!(select_routes(&scores), vec!["scoracle.character.rating"]);
        scores.insert("contract".into(), 0.50);
        assert_eq!(
            select_routes(&scores),
            vec!["scoracle.character.transfers", "scoracle.character.rating"]
        );
    }
    #[tokio::test]
    async fn later_source_material_is_scored_and_every_window_is_required() {
        let mut a = article();
        a.body = format!(
            "{}\n\nNeutral second paragraph.\n\nÉquipe LATE_TRANSFER signed a player.",
            "Background material. ".repeat(90)
        );
        let model = Stub {
            late_only: true,
            ..Stub::default()
        };
        let result = classify(&model, &a).await.unwrap();
        let inputs = cognition::prepare_text(&a.body).unwrap().model_inputs;
        assert!(inputs.len() > 1);
        assert_eq!(model.calls.load(Ordering::SeqCst), 1 + inputs.len());
        assert_eq!(
            result.recommended_characters,
            vec!["scoracle.character.transfers"]
        );
        assert_eq!(result.model_input, result.context);
        let mut missing = result.clone();
        missing.model_provenance["theme_passes"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(missing.verify_against(&a).is_err());
    }
    #[tokio::test]
    async fn saved_scores_coverage_and_policy_cannot_be_changed() {
        let a = article();
        let original = classify(&Stub::default(), &a).await.unwrap();
        for fault in [
            "missing",
            "range",
            "nan",
            "coverage",
            "revision",
            "policy",
            "headline",
            "admission",
            "destinations",
            "window",
        ] {
            let mut changed = original.clone();
            match fault {
                "missing" => {
                    changed.model_provenance["theme_passes"][0]["scores"]
                        .as_object_mut()
                        .unwrap()
                        .remove("fitness");
                }
                "range" => {
                    changed.model_provenance["theme_passes"][0]["scores"]["fitness"] = json!(2.0)
                }
                "nan" => {
                    changed.predicate_scores.insert("fitness".into(), f64::NAN);
                }
                "coverage" => {
                    changed.model_provenance["theme_passes"][0]["provenance"]["coverage"] =
                        Value::Null
                }
                "revision" => {
                    changed.model_provenance["theme_passes"][0]["provenance"]["revision"] =
                        json!("other")
                }
                "policy" => changed.model_provenance["route_thresholds"] = json!({}),
                "headline" => {
                    changed.model_provenance["headline_relevance_probability"] = json!(1.1)
                }
                "admission" => changed.entity_choice = "irrelevant".into(),
                "window" => changed.model_provenance["theme_passes"][0]["start"] = json!(0),
                _ => changed.recommended_characters.clear(),
            }
            assert!(changed.verify_against(&a).is_err(), "{fault}");
        }
    }
    #[tokio::test]
    async fn saved_context_binds_entity_attribution_date_and_unicode_source() {
        let a = article();
        let context = classify(&Stub::default(), &a).await.unwrap();
        for fault in [
            "headline", "entity", "sport", "id", "source", "url", "date", "body",
        ] {
            let mut changed = a.clone();
            match fault {
                "headline" => changed.title.push_str(" changed"),
                "entity" => changed.hypothesis.entity_id += 1,
                "sport" => changed.hypothesis.sport = "NBA".into(),
                "id" => changed.article_id += 1,
                "source" => changed.source = "Other publisher".into(),
                "url" => changed.url.push_str("/different"),
                "date" => changed.published_at = Some("2026-09-27".into()),
                _ => changed.body.push_str(" changed outside opening"),
            }
            assert!(context.verify_against(&changed).is_err(), "{fault}");
        }
        let mut changed = context.clone();
        changed.model_input.end = changed.model_input.start + 1;
        assert!(changed.verify_against(&a).is_err());
        let roundtrip: HarvestContext =
            serde_json::from_value(serde_json::to_value(context).unwrap()).unwrap();
        roundtrip.verify_against(&a).unwrap();
    }
    #[tokio::test]
    async fn wrong_entity_or_changed_question_cannot_reuse_headline_gate() {
        let model = Stub::default();
        let a = article();
        let gate = classify_headline(&model, &a).await.unwrap();
        let mut changed = a.clone();
        changed.hypothesis.sport = "NBA".into();
        assert!(classify_after_headline(&model, &changed, &gate)
            .await
            .is_err());
        let mut altered = gate.clone();
        altered.request.state.push_str(" changed");
        assert!(classify_after_headline(&model, &a, &altered).await.is_err());
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn protocol_missing_source_and_coverage_failures_are_not_negatives() {
        for probability in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert!(classify(
                &Stub {
                    headline: probability,
                    ..Stub::default()
                },
                &article()
            )
            .await
            .is_err());
        }
        assert!(classify(
            &Stub {
                malformed: true,
                ..Stub::default()
            },
            &article()
        )
        .await
        .is_err());
        let mut a = article();
        a.body.clear();
        assert!(classify(&Stub::default(), &a).await.is_err());
        a.body = "word ".repeat(901);
        assert!(classify(&Stub::default(), &a).await.is_err());
    }
    #[tokio::test]
    async fn provider_failure_never_becomes_negative_evidence() {
        struct Failed;
        #[async_trait]
        impl DecisionModel for Failed {
            async fn evaluate(&self, _: &DecisionRequest) -> Result<DecisionResponse> {
                anyhow::bail!("provider unavailable")
            }
        }
        assert!(classify(&Failed, &article()).await.is_err());
    }
}
