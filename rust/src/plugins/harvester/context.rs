//! Cutover contract: exact publisher text plus advisory Laya signals.
//!
//! This is not an editorial packet. No generated story, interpretation, or
//! character-owned decision is represented here.
use super::cognition::{self, Article, Excerpt, Hypothesis, CHARACTER_PLUGINS};
use crate::studio::decision::{ChoiceAnswer, DecisionModel};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT: &str = "harvest-context-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarvestContext {
    pub contract_version: String,
    pub article_id: i64,
    pub headline: String,
    pub source: String,
    pub url: String,
    pub hypothesis: Hypothesis,
    pub feed_rank: Option<i32>,
    pub body_sha256: String,
    pub model_input: Excerpt,
    pub context: Excerpt,
    /// Laya's observed answer. It is not an admission decision in this contract.
    pub entity_choice: String,
    /// Complete uncalibrated distributions keyed by stable character plugin ID.
    pub character_distributions: BTreeMap<String, ChoiceAnswer>,
    /// Laya's proposed routes. All four characters retain final authority.
    pub recommended_characters: Vec<String>,
    pub model_provenance: Value,
    pub model_revision: String,
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
            serde_json::to_value(&self.hypothesis)? == serde_json::to_value(&article.hypothesis)?,
            "query entity changed"
        );
        ensure!(self.feed_rank == article.feed_rank, "Google rank changed");
        ensure!(
            self.body_sha256 == hex::encode(Sha256::digest(article.body.as_bytes())),
            "publisher body changed"
        );
        let (expected, _) = cognition::prepare_relevance(article)?;
        ensure!(
            serde_json::to_value(&self.model_input)?
                == serde_json::to_value(&expected.model_input)?,
            "Laya input is not the exact bounded publisher opening"
        );
        let context = cognition::first_sentences(&article.body, 3);
        ensure!(
            serde_json::to_value(&self.context)? == serde_json::to_value(&context)?,
            "character context is not the exact first three source sentences"
        );
        ensure!(
            article.body.get(self.context.start..self.context.end)
                == Some(self.context.text.as_str()),
            "character context byte range does not match publisher text"
        );
        Ok(())
    }
}

pub async fn classify(model: &dyn DecisionModel, article: &Article) -> Result<HarvestContext> {
    let (prepared, relevance_request) = cognition::prepare_relevance(article)?;
    let relevance_response = model.evaluate(&relevance_request).await?;
    let _ = cognition::passed_relevance(&relevance_request, &relevance_response)?;

    // Until routing is calibrated, the entity choice is advisory. Still evaluate
    // every perspective so no candidate vanishes before the character's own guard.
    let character_request = cognition::prepare_character_routing(article, &prepared);
    let character_response = model.evaluate(&character_request).await?;
    cognition::validate(&character_request, &character_response)?;
    ensure!(
        relevance_response.provenance["model"] == character_response.provenance["model"]
            && relevance_response.provenance["revision"]
                == character_response.provenance["revision"],
        "Laya checkpoint changed within one article"
    );
    let model_revision = relevance_response.provenance["revision"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    ensure!(
        !model_revision.trim().is_empty(),
        "Laya checkpoint revision is missing"
    );
    ensure!(
        relevance_response.provenance["model"]
            .as_str()
            .is_some_and(|model| !model.trim().is_empty()),
        "Laya model identity is missing"
    );
    let mut character_distributions = BTreeMap::new();
    let mut recommended_characters = Vec::new();
    for (question, plugin_id, _) in CHARACTER_PLUGINS {
        let answer = character_response.answers[*question].clone();
        if answer.choice == "relevant" {
            recommended_characters.push((*plugin_id).to_string());
        }
        character_distributions.insert((*plugin_id).to_string(), answer);
    }
    let result = HarvestContext {
        contract_version: CONTRACT.into(),
        article_id: article.article_id,
        headline: article.title.clone(),
        source: article.source.clone(),
        url: article.url.clone(),
        hypothesis: article.hypothesis.clone(),
        feed_rank: article.feed_rank,
        body_sha256: hex::encode(Sha256::digest(article.body.as_bytes())),
        model_input: prepared.model_input,
        context: cognition::first_sentences(&article.body, 3),
        entity_choice: relevance_response.answers["relevance"].choice.clone(),
        character_distributions,
        recommended_characters,
        model_provenance: json!({
            "relevance": relevance_response.provenance,
            "character_routing": character_response.provenance,
            "question_set_versions": {
                "relevance": cognition::RELEVANCE_QUESTIONS,
                "character_routing": cognition::CHARACTER_QUESTIONS,
            }
        }),
        model_revision,
    };
    result.verify_against(article)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::decision::{DecisionRequest, DecisionResponse};
    use async_trait::async_trait;

    struct Stub {
        reject_entity: bool,
        malformed: bool,
    }
    #[async_trait]
    impl DecisionModel for Stub {
        async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
            let answers = request
                .questions
                .iter()
                .map(|(id, _)| {
                    let choice = if id == "relevance" && self.reject_entity {
                        "irrelevant"
                    } else {
                        "relevant"
                    };
                    (
                        id.clone(),
                        ChoiceAnswer {
                            choice: choice.into(),
                            probabilities: BTreeMap::from([
                                (
                                    "irrelevant".into(),
                                    if choice == "irrelevant" { 0.8 } else { 0.2 },
                                ),
                                (
                                    "relevant".into(),
                                    if choice == "relevant" { 0.8 } else { 0.2 },
                                ),
                            ]),
                        },
                    )
                })
                .collect();
            Ok(DecisionResponse {
                answers,
                provenance: json!({
                    "model":"stub","revision":"r1","adapter":"test","device":"cpu",
                    "coverage": request.questions.keys().map(|id| (id.clone(),json!({"truncated":self.malformed}))).collect::<BTreeMap<_,_>>()
                }),
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
            description: "RSS-only text".into(),
            body: "  Équipe returned. Fans cheered! Third sentence? Fourth.".into(),
            hypothesis: Hypothesis {
                name: "Équipe".into(),
                entity_type: "team".into(),
                entity_id: 9,
                sport: "FOOTBALL".into(),
            },
            baseline: Value::Null,
        }
    }

    #[tokio::test]
    async fn entity_rejection_still_retains_exact_context_and_all_routing_signals() {
        let article = article();
        let result = classify(
            &Stub {
                reject_entity: true,
                malformed: false,
            },
            &article,
        )
        .await
        .unwrap();
        assert_eq!(result.entity_choice, "irrelevant");
        assert_eq!(result.character_distributions.len(), 4);
        assert_eq!(
            &article.body[result.context.start..result.context.end],
            result.context.text
        );
        assert!(!result.model_input.text.contains("RSS-only"));
        let roundtrip: HarvestContext =
            serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
        roundtrip.verify_against(&article).unwrap();
        let mut changed = article.clone();
        changed.body.push_str(" altered");
        assert!(roundtrip.verify_against(&changed).is_err());
    }

    #[tokio::test]
    async fn truncated_laya_output_is_an_error_not_a_relevance_decision() {
        assert!(classify(
            &Stub {
                reject_entity: true,
                malformed: true
            },
            &article()
        )
        .await
        .is_err());
    }
}
