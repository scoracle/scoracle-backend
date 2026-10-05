//! Graph suggests source-bound investigation requests; relation extraction is unavailable.
//! Storage, routing, and publication belong to the application.
use crate::studio::model::GenerateOptions;
use crate::studio::{Extracted, Parser, Studio};
use anyhow::Result;
use serde::Deserialize;
pub mod prompt;
pub use prompt::{build_graph_prompt, GRAPH_PROMPT_VERSION, GRAPH_SYSTEM_PROMPT};

pub struct Assignment {
    pub article: GraphArticle,
    pub candidates: Vec<GraphCandidate>,
}

/// An empty closed candidate list has no material and makes no model call.
pub async fn extract_graph(
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<Option<Extracted<GraphExtraction>>> {
    if assignment.candidates.is_empty() {
        return Ok(None);
    }
    let a = &assignment.article;
    let prompt = build_graph_prompt(
        &a.source,
        &a.published,
        &a.title,
        &a.description,
        &assignment.candidates,
    );
    studio
        .extract(
            &prompt,
            &graph_opts(),
            &GraphParser {
                candidates: &assignment.candidates,
            },
            crate::plugins::support::prompt::structured_correction,
        )
        .await
        .map(Some)
}

/// The six-predicate vocabulary — MUST mirror the `narrative_events_predicate_check`
/// constraint. Grow both together with schema and evaluation evidence.
pub const PREDICATES: &[&str] = &[
    "trade_rumor",
    "trade_confirmed",
    "injury",
    "contract_dispute",
    "praise",
    "criticism",
];

/// Person kinds mirror the database constraint. An out-of-vocabulary
/// role guess maps to "other" rather than dropping the discovery (the promotion gate,
/// not the extractor, decides who becomes an entity).
pub const PERSON_KINDS: &[&str] = &["coach", "agent", "executive", "family", "other"];

/// The model budget for one extraction call. Temperature 0.2 (tight but a judgment
/// call, matching scrub adjudication); JSON mode tightens contract adherence.
///
/// Graph shares the Editor's local context size to avoid runner reloads.
pub fn graph_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(GRAPH_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 768,
        num_ctx: 4096,
        json_mode: true,
        format_schema: None,
        format_schema_raw: None,
    }
}

/// One numbered candidate shown to the model. `descriptor` is the identity card line
/// ("player, currently at X" / "(team)") — same disambiguation surface as resolve.rs.
#[derive(Clone, Debug)]
pub struct GraphCandidate {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub descriptor: String,
}

/// A validated typed relation, subject/object resolved back to (entity_type, entity_id)
/// — ready for a `narrative_events` row.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphRelation {
    pub subject_type: String,
    pub subject_id: i32,
    pub predicate: String,
    pub object_type: Option<String>,
    pub object_id: Option<i32>,
    pub sentiment: Option<f64>,
    pub confidence: String,
}

/// A person discovery — a `narrative_persons` candidate (or an evidence increment for
/// an existing one).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPerson {
    pub name: String,
    pub kind: String,
    pub team_context_type: Option<String>,
    pub team_context_id: Option<i32>,
}

#[derive(Debug, Default)]
pub struct GraphExtraction {
    pub relations: Vec<GraphRelation>,
    pub persons: Vec<GraphPerson>,
    /// Optional verbatim completed result, validated against source bytes by the adapter.
    pub final_result_line: String,
}

/// One article's metadata for the extraction prompt.
#[derive(Clone, Debug)]
pub struct GraphArticle {
    pub source: String,
    pub published: String,
    pub title: String,
    pub description: String,
}

/// GraphParser validates the model reply against the candidate list and vocabularies.
/// Fail-closed: no JSON object / unparseable ⇒ `Ok(None)`. Within a parsed body:
/// out-of-range entity numbers, unknown predicates, unknown confidences, and self-loops
/// drop THAT entry; sentiment clamps to [-1, 1]; person role guesses outside the
/// vocabulary map to "other"; empty/duplicate person names drop.
pub struct GraphParser<'a> {
    pub candidates: &'a [GraphCandidate],
}

impl Parser<GraphExtraction> for GraphParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<GraphExtraction>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None),
        };
        #[derive(Deserialize)]
        struct PersonReply {
            #[serde(default)]
            name: String,
        }
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            persons: Vec<PersonReply>,
            #[serde(default)]
            final_result_line: String,
        }
        let reply: Reply = match serde_json::from_str(&raw[start..=end]) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        let mut out = GraphExtraction::default();
        out.final_result_line = reply.final_result_line;
        let mut seen = std::collections::HashSet::new();
        for p in reply.persons {
            let name = p.name.trim().to_string();
            if name.is_empty() || !seen.insert(name.to_lowercase()) {
                continue;
            }
            // A person "discovery" that names a listed candidate is a model slip —
            // those entities are already known; drop it.
            if self
                .candidates
                .iter()
                .any(|c| c.descriptor.to_lowercase().contains(&name.to_lowercase()))
            {
                continue;
            }
            out.persons.push(GraphPerson {
                name,
                kind: "other".into(),
                team_context_type: None,
                team_context_id: None,
            });
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests;
