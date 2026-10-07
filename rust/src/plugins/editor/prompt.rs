//! Full retained article scoring, never a generated editorial packet.
use crate::plugins::harvester::cognition::{Article, Excerpt};
use crate::plugins::system_one::{DecisionRequest, PredicateQuestion};
use std::collections::BTreeMap;

pub const MODEL_ENDPOINT_ENV: &str = "EDITOR_MODEL_ENDPOINT";
pub const RELEVANCE_THRESHOLD: f64 = 0.5;
pub const QUESTION_VERSION: &str = "editor-source-routing-v1";

pub fn prepare(article: &Article, input: &Excerpt) -> DecisionRequest {
    let mut request =
        crate::plugins::harvester::cognition::prepare_character_routing(article, input);
    request.state = crate::tools::fetch::decode_entities(&input.text);
    request.questions.insert("article_relevance".into(), PredicateQuestion::Boolean {
        instructions: format!("This text reports information directly about {}. Incidental mentions, navigation and related-story links do not establish relevance.", article.hypothesis.reference()),
        criteria: BTreeMap::from([("false".into(), "Not reported".into()), ("true".into(), "Reported".into())]),
    });
    request
}
