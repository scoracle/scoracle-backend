//! Cascading relevance classification followed by verbatim source publication.
//! Harvester owns this contract; tagged character plugins own the final judgment.

use crate::studio::decision::{ChoiceQuestion, DecisionRequest, DecisionResponse};
use crate::util::hash_components;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT: &str = "harvest-v4";
pub const RELEVANCE_QUESTIONS: &str = "harvest-relevance-v4";
pub const CHARACTER_QUESTIONS: &str = "harvest-theme-routing-v4";
pub const POLICY: &str = "google-laya-theme-cascade-v2";

/// Question key, stable plugin identity, and the perspective Laya should match.
pub const CHARACTER_PLUGINS: &[(&str, &str, &str)] = &[
    (
        "narrative",
        "scoracle.character.narrative",
        "news developments, match reporting, results, or an ongoing sports story",
    ),
    (
        "emotional_charge",
        "scoracle.character.vibe",
        "feelings, reactions, celebration, anger, criticism, or anticipation",
    ),
    (
        "transfers",
        "scoracle.character.transfers",
        "transfers, trades, contracts, hiring, firing, or organizational changes",
    ),
    (
        "availability",
        "scoracle.character.rating",
        "performance, injury, suspension, availability, selection, or lineups",
    ),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Hypothesis {
    pub name: String,
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Article {
    pub article_id: i64,
    pub title: String,
    pub source: String,
    pub url: String,
    pub published_at: Option<String>,
    /// Google's best (lowest) result position for this article and entity sweep.
    #[serde(default)]
    pub feed_rank: Option<i32>,
    /// Google's RSS description, retained as retrieval provenance. It never substitutes
    /// for the retained publisher opening used by Laya or the character plugins.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    pub hypothesis: Hypothesis,
    #[serde(default)]
    pub baseline: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Excerpt {
    pub text: String,
    /// Half-open UTF-8 byte range into the unchanged retained article body.
    pub start: usize,
    pub end: usize,
    pub selection: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreparedText {
    /// The smaller exact prefix shown to Laya.
    pub model_input: Excerpt,
    /// The exact first three paragraphs delivered to selected character plugins.
    pub character_context: Excerpt,
}

fn bounded_prefix_end(content: &str, max_words: usize, max_bytes: usize) -> usize {
    let mut end = content.len().min(max_bytes);
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    let mut in_word = false;
    let mut words = 0;
    let mut last_boundary = 0;
    for (i, c) in content[..end].char_indices() {
        if c.is_whitespace() {
            in_word = false;
            last_boundary = i;
        } else if !in_word {
            if words == max_words {
                end = i;
                break;
            }
            words += 1;
            in_word = true;
        }
    }
    if end < content.len()
        && !content[end..].starts_with(char::is_whitespace)
        && !content[..end].ends_with(char::is_whitespace)
        && last_boundary > 0
    {
        end = last_boundary;
    }
    content[..end].trim_end().len()
}

fn excerpt(body: &str, end: usize, selection: &str) -> Excerpt {
    let start = body.len() - body.trim_start().len();
    let content = &body[start..];
    let end = content[..end].trim_end().len();
    Excerpt {
        text: content[..end].to_string(),
        start,
        end: start + end,
        selection: selection.into(),
    }
}

fn model_input(body: &str, selection: &str) -> Excerpt {
    let content = body.trim_start();
    excerpt(body, bounded_prefix_end(content, 100, 1200), selection)
}

/// Copy the first `count` nonempty paragraphs verbatim. A blank line separates
/// paragraphs; single newlines within a paragraph remain part of the source.
pub fn first_paragraphs(body: &str, count: usize) -> Excerpt {
    let content = body.trim_start();
    if count == 0 {
        return excerpt(body, 0, "first_zero_paragraphs");
    }
    let mut completed = 0;
    let mut in_paragraph = false;
    let mut paragraph_end = 0;
    let mut line_start = 0;
    for line in content.split_inclusive('\n') {
        if line.trim().is_empty() {
            if in_paragraph {
                completed += 1;
                in_paragraph = false;
            }
        } else {
            if !in_paragraph && completed == count {
                return excerpt(body, paragraph_end, "first_three_paragraphs");
            }
            in_paragraph = true;
            paragraph_end = line_start + line.len();
        }
        line_start += line.len();
    }
    excerpt(body, content.len(), "available_opening_paragraphs")
}

pub fn prepare_text(body: &str) -> PreparedText {
    PreparedText {
        model_input: model_input(body, "bounded_verbatim_publisher_opening"),
        character_context: first_paragraphs(body, 3),
    }
}

fn classification_source(article: &Article) -> (&str, &'static str) {
    (&article.body, "bounded_verbatim_publisher_opening")
}

fn prepare_article_text(article: &Article) -> PreparedText {
    let (source, selection) = classification_source(article);
    PreparedText {
        model_input: model_input(source, selection),
        character_context: if article.body.trim().is_empty() {
            Excerpt {
                text: String::new(),
                start: 0,
                end: 0,
                selection: "publisher_fetch_pending".into(),
            }
        } else {
            first_paragraphs(&article.body, 3)
        },
    }
}

fn question(instructions: &str, irrelevant: &str, relevant: &str) -> ChoiceQuestion {
    ChoiceQuestion {
        kind: "choice".into(),
        instructions: instructions.into(),
        criteria: BTreeMap::from([
            ("irrelevant".into(), irrelevant.into()),
            ("relevant".into(), relevant.into()),
        ]),
    }
}

fn state(article: &Article, prepared: &PreparedText) -> String {
    format!(
        "Target entity: {} ({}, {})\nPublisher: {}\nHeadline: {}\nPublisher opening:\n{}",
        article.hypothesis.name,
        article.hypothesis.entity_type,
        article.hypothesis.sport,
        article.source,
        article.title,
        prepared.model_input.text
    )
}

/// Step one: one deliberately simple entity-relevance question.
pub fn prepare_relevance(article: &Article) -> Result<(PreparedText, DecisionRequest)> {
    ensure!(
        !article.hypothesis.name.trim().is_empty(),
        "missing target entity"
    );
    ensure!(!article.title.trim().is_empty(), "missing article headline");
    let (classification_source, _) = classification_source(article);
    ensure!(
        !classification_source.trim().is_empty(),
        "missing publisher body"
    );
    let prepared = prepare_article_text(article);
    let questions = BTreeMap::from([(
        "relevance".into(),
        question(
            &format!(
                "Is this article substantively about or directly consequential to {} ({})? A passing mention or a different entity with the same name is irrelevant.",
                article.hypothesis.name, article.hypothesis.sport
            ),
            "The source is not substantively about or directly consequential to this exact target entity",
            "The source is substantively about or directly consequential to this exact target entity",
        ),
    )]);
    Ok((
        prepared.clone(),
        DecisionRequest {
            state: state(article, &prepared),
            questions,
        },
    ))
}

/// Step two: independent probabilities for each character theme. This request is
/// evaluated only after step one succeeds.
pub fn prepare_character_routing(article: &Article, prepared: &PreparedText) -> DecisionRequest {
    let questions = CHARACTER_PLUGINS
        .iter()
        .map(|(key, _, perspective)| {
            (
                (*key).to_string(),
                question(
                    &format!(
                        "Does this opening contain material about {} for this theme? Judge only material tied to the target entity.",
                        article.hypothesis.name
                    ),
                    &format!("The opening contains no target-linked material concerning {perspective}"),
                    &format!("The opening contains target-linked material concerning {perspective}"),
                ),
            )
        })
        .collect();
    DecisionRequest {
        state: state(article, prepared),
        questions,
    }
}

/// Reject malformed distributions. Model uncertainty is not a protocol failure.
pub fn validate(request: &DecisionRequest, response: &DecisionResponse) -> Result<()> {
    ensure!(
        response.answers.len() == request.questions.len(),
        "answer count mismatch"
    );
    for (id, q) in &request.questions {
        let a = response
            .answers
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("missing answer {id}"))?;
        ensure!(
            a.probabilities.keys().eq(q.criteria.keys()),
            "option mismatch: {id}"
        );
        ensure!(
            a.probabilities.contains_key(&a.choice),
            "unknown choice: {id}"
        );
        ensure!(
            a.probabilities
                .values()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p)),
            "invalid probability: {id}"
        );
        ensure!(
            (a.probabilities.values().sum::<f64>() - 1.0).abs() < 0.001,
            "distribution not normalized: {id}"
        );
        let maximum = a.probabilities.values().copied().fold(0.0, f64::max);
        ensure!(
            a.probabilities[&a.choice] + 0.000001 >= maximum,
            "choice disagrees with distribution: {id}"
        );
    }
    ensure!(
        response.provenance.is_object(),
        "missing provider provenance"
    );
    for key in ["model", "revision", "adapter", "device"] {
        ensure!(
            response.provenance[key]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "missing provider {key}"
        );
    }
    for id in request.questions.keys() {
        ensure!(
            response.provenance["coverage"][id]["truncated"] == json!(false),
            "provider did not confirm complete opening coverage: {id}"
        );
    }
    Ok(())
}

pub fn passed_relevance(request: &DecisionRequest, response: &DecisionResponse) -> Result<bool> {
    validate(request, response)?;
    ensure!(
        request.questions.len() == 1 && request.questions.contains_key("relevance"),
        "not a Harvester relevance request"
    );
    Ok(response.answers["relevance"].choice == "relevant")
}

/// Compile the cascade without changing source text. A successful gate requires a
/// character-routing result; a failed gate must not pretend the second call happened.
pub fn compile(
    article: &Article,
    prepared: PreparedText,
    relevance_request: &DecisionRequest,
    relevance_response: DecisionResponse,
    character_stage: Option<(&DecisionRequest, DecisionResponse)>,
) -> Result<Value> {
    // A byte-valid slice alone does not bind a decision to its headline, entity,
    // or prompt contract. Reconstruct both requests before accepting a packet.
    let (expected_text, expected_request) = prepare_relevance(article)?;
    ensure!(
        serde_json::to_value(&prepared)? == serde_json::to_value(&expected_text)?,
        "prepared source differs from the current extraction contract"
    );
    ensure!(
        serde_json::to_value(relevance_request)? == serde_json::to_value(&expected_request)?,
        "relevance request differs from the current article or prompt contract"
    );
    let relevant = passed_relevance(relevance_request, &relevance_response)?;
    let (classification_source, _) = classification_source(article);
    ensure!(
        classification_source.get(prepared.model_input.start..prepared.model_input.end)
            == Some(prepared.model_input.text.as_str()),
        "model input is not verbatim publisher context"
    );
    if relevant {
        ensure!(!article.body.trim().is_empty(), "missing publisher body");
        ensure!(
            article
                .body
                .get(prepared.character_context.start..prepared.character_context.end)
                == Some(prepared.character_context.text.as_str()),
            "character context is not verbatim publisher text"
        );
    }

    let mut character_signals = serde_json::Map::new();
    let mut character_probabilities = serde_json::Map::new();
    let mut character_tags = Vec::new();
    let mut character_request = Value::Null;
    let mut character_provenance = Value::Null;
    let mut character_raw_response = Value::Null;

    match (relevant, character_stage) {
        (false, None) => {}
        (false, Some(_)) => anyhow::bail!("character routing ran after failed relevance gate"),
        (true, None) => anyhow::bail!("missing character routing after relevance success"),
        (true, Some((request, response))) => {
            ensure!(
                serde_json::to_value(request)?
                    == serde_json::to_value(prepare_character_routing(article, &prepared))?,
                "character request differs from the current article or prompt contract"
            );
            validate(request, &response)?;
            ensure!(
                request.questions.len() == CHARACTER_PLUGINS.len()
                    && CHARACTER_PLUGINS
                        .iter()
                        .all(|(key, _, _)| request.questions.contains_key(*key)),
                "not a Harvester character-routing request"
            );
            for (key, plugin_id, _) in CHARACTER_PLUGINS {
                let answer = &response.answers[*key];
                character_signals.insert((*plugin_id).into(), serde_json::to_value(answer)?);
                character_probabilities
                    .insert((*plugin_id).into(), json!(answer.probabilities["relevant"]));
                if answer.choice == "relevant" {
                    character_tags.push(*plugin_id);
                }
            }
            character_request = serde_json::to_value(request)?;
            character_provenance = response.provenance;
            character_raw_response = response.raw_response;
        }
    }

    let body_hash =
        (!article.body.is_empty()).then(|| hex::encode(Sha256::digest(article.body.as_bytes())));
    let disposition = if relevant { "accept" } else { "reject" };
    let character_input_hash = if character_request.is_null() {
        Value::Null
    } else {
        json!(hash_components(&serde_json::to_string(&character_request)?))
    };
    Ok(json!({
        "contract_version": CONTRACT,
        "question_set_versions": {
            "relevance": RELEVANCE_QUESTIONS,
            "character_routing": CHARACTER_QUESTIONS
        },
        "policy_version": POLICY,
        "article_id": article.article_id,
        "source": article.source,
        "url": article.url,
        "published_at": article.published_at,
        "headline": article.title,
        "hypothesis": article.hypothesis,
        "retrieval": {
            "provider": "google_news",
            "kind": "ranked_entity_query",
            "feed_rank": article.feed_rank,
            "description": article.description
        },
        "body_hash": body_hash,
        "input_hashes": {
            "relevance": hash_components(&serde_json::to_string(relevance_request)?),
            "character_routing": character_input_hash
        },
        "model_input_excerpt": prepared.model_input,
        "excerpt": if relevant { serde_json::to_value(&prepared.character_context)? } else { Value::Null },
        "signals": {
            "relevance": relevance_response.answers["relevance"],
            "characters": character_signals,
            "character_relevance_probabilities": character_probabilities
        },
        "provenance": {
            "relevance": relevance_response.provenance,
            "character_routing": character_provenance
        },
        "raw_responses": {
            "relevance": relevance_response.raw_response,
            "character_routing": character_raw_response
        },
        "requests": {
            "relevance": relevance_request,
            "character_routing": character_request
        },
        "disposition": disposition,
        "character_tags": character_tags,
        "context": if relevant {
            json!({
                "kind": "publisher_source",
                "headline": article.title,
                "text": prepared.character_context.text,
                "note": "Verbatim headline and first three source paragraphs selected by Harvester; the character plugin determines permissible claims and output form."
            })
        } else {
            Value::Null
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::decision::ChoiceAnswer;

    fn article() -> Article {
        Article {
            article_id: 1,
            title: "A title".into(),
            source: "A source".into(),
            url: "https://example.com/a".into(),
            published_at: None,
            feed_rank: Some(3),
            description: String::new(),
            body:
                "  First paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n\nFourth paragraph."
                    .into(),
            hypothesis: Hypothesis {
                name: "Arsenal".into(),
                sport: "FOOTBALL".into(),
                entity_type: "team".into(),
                entity_id: 1,
            },
            baseline: Value::Null,
        }
    }

    fn response(request: &DecisionRequest, relevant: bool) -> DecisionResponse {
        let winner = if relevant { "relevant" } else { "irrelevant" };
        DecisionResponse {
            answers: request
                .questions
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        ChoiceAnswer {
                            choice: winner.into(),
                            probabilities: BTreeMap::from([
                                ("irrelevant".into(), if relevant { 0.1 } else { 0.9 }),
                                ("relevant".into(), if relevant { 0.9 } else { 0.1 }),
                            ]),
                        },
                    )
                })
                .collect(),
            provenance: json!({"model":"fixture", "revision":"fixture-v1", "adapter":"fixture", "device":"test",
                "coverage": request.questions.keys().map(|id| (id.clone(),json!({"truncated":false}))).collect::<BTreeMap<_,_>>() }),
            raw_response: Value::Null,
        }
    }

    #[test]
    fn extraction_is_exactly_the_first_three_paragraphs() {
        let a = article();
        let x = first_paragraphs(&a.body, 3);
        assert_eq!(&a.body[x.start..x.end], x.text);
        assert_eq!(
            x.text,
            "First paragraph.\n\nSecond paragraph.\n\nThird paragraph."
        );
    }

    #[test]
    fn paragraph_extraction_keeps_all_sentences_and_skips_extra_blank_lines() {
        let body = "  First. Still first!\r\nAnother line.\r\n\r\n\r\nSecond. Also second.\r\n \r\nThird? Yes.\r\n\r\nFourth.";
        let x = first_paragraphs(body, 3);
        assert_eq!(&body[x.start..x.end], x.text);
        assert_eq!(x.selection, "first_three_paragraphs");
        assert!(x.text.contains("Still first!\r\nAnother line."));
        assert!(x.text.ends_with("Third? Yes."));
        assert!(!x.text.contains("Fourth."));
    }

    #[test]
    fn laya_input_is_a_bounded_verbatim_prefix() {
        let body = format!("  {}", "word ".repeat(400));
        let x = prepare_text(&body).model_input;
        assert_eq!(&body[x.start..x.end], x.text);
        assert!(x.text.split_whitespace().count() <= 100);
        assert!(x.text.len() <= 1200);
        assert_eq!(x.selection, "bounded_verbatim_publisher_opening");
    }

    #[test]
    fn google_description_never_substitutes_for_the_publisher_opening() {
        let mut a = article();
        a.description = "UNRELATED_THIN_RSS_DESCRIPTION".into();
        let (prepared, request) = prepare_relevance(&a).unwrap();
        assert!(!prepared.model_input.text.contains("UNRELATED_THIN"));
        assert!(!request.state.contains("UNRELATED_THIN"));
        assert!(request.state.contains("First paragraph"));
    }

    #[test]
    fn successful_cascade_tags_plugins_and_never_rewrites_source() {
        let a = article();
        let (x, gate) = prepare_relevance(&a).unwrap();
        let routing = prepare_character_routing(&a, &x);
        let mut routing_response = response(&routing, true);
        routing_response
            .answers
            .get_mut("availability")
            .unwrap()
            .choice = "irrelevant".into();
        routing_response
            .answers
            .get_mut("availability")
            .unwrap()
            .probabilities = BTreeMap::from([("irrelevant".into(), 0.8), ("relevant".into(), 0.2)]);
        let packet = compile(
            &a,
            x.clone(),
            &gate,
            response(&gate, true),
            Some((&routing, routing_response)),
        )
        .unwrap();
        assert_eq!(packet["disposition"], "accept");
        assert_eq!(packet["context"]["headline"], a.title);
        assert_eq!(packet["context"]["text"], x.character_context.text);
        assert_eq!(packet["retrieval"]["feed_rank"], 3);
        assert_eq!(
            packet["signals"]["character_relevance_probabilities"]["scoracle.character.rating"],
            0.2
        );
        assert_eq!(
            packet["character_tags"],
            json!([
                "scoracle.character.narrative",
                "scoracle.character.vibe",
                "scoracle.character.transfers"
            ])
        );
    }

    #[test]
    fn failed_gate_stops_before_character_scoring() {
        let a = article();
        let (x, gate) = prepare_relevance(&a).unwrap();
        let packet = compile(&a, x, &gate, response(&gate, false), None).unwrap();
        assert_eq!(packet["disposition"], "reject");
        assert_eq!(packet["character_tags"], json!([]));
        assert!(packet["context"].is_null());
        assert!(packet["requests"]["character_routing"].is_null());
    }

    #[test]
    fn malformed_results_and_rewritten_source_are_errors() {
        let a = article();
        let (mut x, gate) = prepare_relevance(&a).unwrap();
        let mut malformed = response(&gate, true);
        malformed.provenance["coverage"]["relevance"]["truncated"] = json!(true);
        assert!(passed_relevance(&gate, &malformed).is_err());
        x.character_context.text = "A model summary.".into();
        let routing = prepare_character_routing(&a, &x);
        assert!(compile(
            &a,
            x,
            &gate,
            response(&gate, true),
            Some((&routing, response(&routing, true)))
        )
        .is_err());
    }

    #[test]
    fn cached_baseline_never_enters_either_model_state() {
        let mut a = article();
        a.baseline = json!({"read":"SECRET_TEACHER_ANSWER"});
        let (x, gate) = prepare_relevance(&a).unwrap();
        let routing = prepare_character_routing(&a, &x);
        assert!(!serde_json::to_string(&gate)
            .unwrap()
            .contains("SECRET_TEACHER_ANSWER"));
        assert!(!serde_json::to_string(&routing)
            .unwrap()
            .contains("SECRET_TEACHER_ANSWER"));
        let packet = compile(
            &a,
            x,
            &gate,
            response(&gate, true),
            Some((&routing, response(&routing, true))),
        )
        .unwrap();
        assert!(!packet.to_string().contains("SECRET_TEACHER_ANSWER"));
        assert!(packet.get("baseline").is_none());
    }
}
