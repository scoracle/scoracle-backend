//! Cascading relevance classification followed by verbatim source publication.
//! Harvester owns this contract; tagged character plugins own the final judgment.

use super::policy::{CHARACTER_ROUTES, MAX_THEME_WINDOWS};
use crate::plugins::harvester::decision::{DecisionRequest, DecisionResponse, PredicateQuestion};
use crate::tools::meta::EntityMeta;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const RELEVANCE_QUESTIONS: &str = "harvest-headline-relevance-v3";
pub const CHARACTER_QUESTIONS: &str = "harvest-theme-routing-v6";

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
    #[serde(default)]
    pub body: String,
    pub hypothesis: EntityMeta,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Excerpt {
    pub text: String,
    /// Half-open UTF-8 byte range into the unchanged retained article body.
    pub start: usize,
    pub end: usize,
    pub selection: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreparedText {
    /// Bounded, exact source windows covering all retained non-whitespace text.
    pub model_inputs: Vec<Excerpt>,
    /// The exact first three paragraphs delivered to selected character plugins.
    pub character_context: Excerpt,
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

/// Exact retained source span, with only outer whitespace excluded.
pub fn full_article(body: &str) -> Excerpt {
    excerpt(body, body.trim_start().trim_end().len(), "retained_article")
}

/// Cover every retained paragraph. Gaps may contain whitespace only; an oversized
/// opening is a coverage error, never a negative classification.
pub fn prepare_text(body: &str) -> Result<PreparedText> {
    Ok(PreparedText {
        model_inputs: crate::tools::source::windows(body, MAX_THEME_WINDOWS)?
            .into_iter()
            .map(|window| Excerpt {
                text: window.text,
                start: window.start,
                end: window.end,
                selection: "verbatim_publisher_window".into(),
            })
            .collect(),
        character_context: first_paragraphs(body, 3),
    })
}

fn question(instructions: String) -> PredicateQuestion {
    PredicateQuestion::Boolean {
        instructions,
        criteria: BTreeMap::from([
            ("false".into(), "Not reported".into()),
            ("true".into(), "Reported".into()),
        ]),
    }
}

/// No inference of unstated affiliations or indirect consequences is requested.
pub fn prepare_relevance(article: &Article) -> Result<DecisionRequest> {
    ensure!(
        !article.hypothesis.name.trim().is_empty(),
        "missing target entity"
    );
    ensure!(
        !article.hypothesis.sport.trim().is_empty(),
        "missing target sport"
    );
    ensure!(
        !article.hypothesis.entity_type.trim().is_empty(),
        "missing target entity type"
    );
    ensure!(!article.title.trim().is_empty(), "missing article headline");
    Ok(DecisionRequest {
        state: article.title.clone(),
        questions: BTreeMap::from([(
            "relevance".into(),
            question(format!(
                "This headline explicitly refers to {}.",
                article.hypothesis.reference()
            )),
        )]),
    })
}

/// Publisher evidence only: title/publisher metadata cannot create theme evidence.
pub fn prepare_character_routing(article: &Article, input: &Excerpt) -> DecisionRequest {
    DecisionRequest {
        state: input.text.clone(),
        questions: CHARACTER_ROUTES
            .iter()
            .flat_map(|route| route.predicates.iter())
            .map(|predicate| {
                (
                    predicate.key.into(),
                    question(
                        predicate
                            .statement
                            .replace("{target}", &article.hypothesis.reference()),
                    ),
                )
            })
            .collect(),
    }
}

/// Reject malformed scores. Model uncertainty is not a protocol failure.
pub fn validate(request: &DecisionRequest, response: &DecisionResponse) -> Result<()> {
    ensure!(
        response.answers.len() == request.questions.len(),
        "answer count mismatch"
    );
    for id in request.questions.keys() {
        let a = response
            .answers
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("missing answer {id}"))?;
        ensure!(
            a.probability.is_finite() && (0.0..=1.0).contains(&a.probability),
            "invalid probability: {id}"
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
                .is_some_and(|s| !s.trim().is_empty()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn article() -> Article {
        Article {
            article_id: 1,
            title: "A title".into(),
            source: "A source".into(),
            url: "https://example.com/a".into(),
            published_at: None,
            feed_rank: Some(3),
            body:
                "  First paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n\nFourth paragraph."
                    .into(),
            hypothesis: EntityMeta {
                name: "Arsenal".into(),
                sport: "FOOTBALL".into(),
                entity_type: "team".into(),
                entity_id: 1,
            },
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
    fn bounded_windows_cover_every_source_character_except_whitespace() {
        let body = format!("  {}\n\nLast paragraph.  ", "Équipe played. ".repeat(200));
        let prepared = prepare_text(&body).unwrap();
        assert!(prepared.model_inputs.len() > 1);
        let mut cursor = prepared.character_context.start;
        for x in &prepared.model_inputs {
            assert!(body[cursor..x.start].chars().all(char::is_whitespace));
            assert_eq!(&body[x.start..x.end], x.text);
            assert!(x.text.split_whitespace().count() <= 100);
            assert!(x.text.len() <= 1200);
            cursor = x.end;
        }
        assert_eq!(cursor, prepared.character_context.end);
    }

    #[test]
    fn target_metadata_is_in_every_question_and_separate_from_source_evidence() {
        let a = article();
        let prepared = prepare_text(&a.body).unwrap();
        let requests = [
            prepare_relevance(&a).unwrap(),
            prepare_character_routing(&a, &prepared.model_inputs[0]),
        ];
        assert_eq!(requests[0].state, a.title);
        assert_eq!(requests[1].state, prepared.model_inputs[0].text);
        for request in requests {
            assert!(!request.state.contains("Arsenal"));
            for question in request.questions.values() {
                let PredicateQuestion::Boolean { instructions, .. } = question;
                assert!(instructions.contains("Arsenal, the FOOTBALL team"));
            }
        }
        let mut missing = a;
        missing.hypothesis.entity_type.clear();
        assert!(prepare_relevance(&missing).is_err());
    }

    #[test]
    fn editor_scoring_reads_beyond_the_delivered_opening() {
        let mut a = article();
        a.body = format!(
            "First opening paragraph.\n\nSecond opening paragraph.\n\nThird opening paragraph.\n\n{}",
            "FOURTH_PARAGRAPH_ONLY ".repeat(120)
        );
        let prepared = prepare_text(&a.body).unwrap();
        let relevance = prepare_relevance(&a).unwrap();
        let themes = prepare_character_routing(&a, &prepared.model_inputs[0]);
        assert_eq!(
            prepared.model_inputs[0].start,
            prepared.character_context.start
        );
        assert!(prepared.model_inputs.last().unwrap().end > prepared.character_context.end);
        assert!(!relevance.state.contains("FOURTH_PARAGRAPH_ONLY"));
        assert!(themes.state.contains("FOURTH_PARAGRAPH_ONLY"));
    }

    #[test]
    fn google_description_never_substitutes_for_the_publisher_opening() {
        let mut input = serde_json::to_value(article()).unwrap();
        input["description"] = json!("UNRELATED_THIN_RSS_DESCRIPTION");
        let a: Article = serde_json::from_value(input).unwrap();
        let prepared = prepare_text(&a.body).unwrap();
        let request = prepare_relevance(&a).unwrap();
        assert!(!prepared.model_inputs[0].text.contains("UNRELATED_THIN"));
        assert!(!request.state.contains("UNRELATED_THIN"));
        assert_eq!(request.state, "A title");
        assert!(!request.state.contains("First paragraph"));
    }
}
