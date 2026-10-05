//! Graph investigation nominations, without factual authority.
//!
//! This Studio evidence seat has no storytelling persona. It reads
//! one article and returns typed structure: relations among already-vetted entities, and people
//! named in the text who are not yet in the entity world. Nothing it writes is read aloud, so
//! there is no voice here to tune and none should be invented.
//!
//! The model suggests exact source names and score lines for review. It cannot
//! authorize relations, roles, identity or fixture scores. Publisher text and
//! provenance are revalidated by the adapter before publication.

use super::GraphCandidate;

pub const GRAPH_SYSTEM_PROMPT: &str = r#"Task: suggest source-bound investigation requests from one sports article.

Copy names of people who appear in the supplied publisher text and are absent from the known entity list. Use exact source spelling. Return kind=other and team_context=null; role and affiliation must be established by the Investigator's evidence gate.

Copy at most one complete score line verbatim, with both team names and scores, as final_result_line. Return an empty string when none is present. This is a review request, not a verified fixture result.

Relation extraction is unavailable. Return relations=[]; do not infer predicates, direction, sentiment or certainty.

Return ONLY JSON:
{"relations":[],"persons":[{"name":"exact source name","kind":"other","team_context":null}],"final_result_line":""}"#;

pub const GRAPH_PROMPT_VERSION: &str = "g7-nominations";

/// build_graph_prompt lays out the article + numbered candidates (1-indexed, matching
/// the reply contract).
pub fn build_graph_prompt(
    source: &str,
    published: &str,
    title: &str,
    description: &str,
    candidates: &[GraphCandidate],
) -> String {
    let mut b = String::new();
    b.push_str(&format!(
        "Article source: {source}\nPublished: {published}\n"
    ));
    b.push_str(&format!("Title: {title}\n"));
    if !description.trim().is_empty() {
        b.push_str(&format!("Text: {description}\n"));
    }
    b.push_str(&format!(
        "\n{}\nKnown entities (use these numbers):\n",
        crate::plugins::support::prompt::IDENTITY_CARD_FRAMING
    ));
    for (i, c) in candidates.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, c.descriptor));
    }
    b.push_str("\nReturn the JSON now.");
    b
}
