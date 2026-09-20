//! Graph extracts typed relations and person candidates from prepared article evidence.
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

impl Studio<'_> {
    /// An empty closed candidate list has no material and makes no model call.
    pub async fn extract_graph(
        &self,
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
        self.extract(
            &prompt,
            &graph_opts(),
            &GraphParser {
                candidates: &assignment.candidates,
            },
        )
        .await
        .map(Some)
    }
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

impl GraphParser<'_> {
    fn resolve(&self, idx: i64) -> Option<&GraphCandidate> {
        if idx >= 1 && (idx as usize) <= self.candidates.len() {
            Some(&self.candidates[idx as usize - 1])
        } else {
            None
        }
    }
}

impl Parser<GraphExtraction> for GraphParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<GraphExtraction>> {
        let (start, end) = match (raw.find('{'), raw.rfind('}')) {
            (Some(s), Some(e)) if e > s => (s, e),
            _ => return Ok(None),
        };
        #[derive(Deserialize)]
        struct RelReply {
            subject: Option<i64>,
            #[serde(default)]
            predicate: String,
            object: Option<i64>,
            sentiment: Option<f64>,
            #[serde(default)]
            confidence: String,
        }
        #[derive(Deserialize)]
        struct PersonReply {
            #[serde(default)]
            name: String,
            #[serde(default)]
            kind: String,
            team_context: Option<i64>,
        }
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            relations: Vec<RelReply>,
            #[serde(default)]
            persons: Vec<PersonReply>,
        }
        let reply: Reply = match serde_json::from_str(&raw[start..=end]) {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        let mut out = GraphExtraction::default();
        for r in reply.relations {
            let Some(subj_idx) = r.subject else { continue };
            let Some(subj) = self.resolve(subj_idx) else {
                continue;
            };
            let predicate = r.predicate.trim().to_lowercase();
            if !PREDICATES.contains(&predicate.as_str()) {
                continue;
            }
            let confidence = r.confidence.trim().to_lowercase();
            if !["speculative", "reported", "confirmed"].contains(&confidence.as_str()) {
                continue;
            }
            let (object_type, object_id) = match r.object {
                None => (None, None),
                Some(oi) => match self.resolve(oi) {
                    Some(obj) => {
                        if obj.entity_type == subj.entity_type && obj.entity_id == subj.entity_id {
                            continue; // self-loop
                        }
                        (Some(obj.entity_type.clone()), Some(obj.entity_id))
                    }
                    None => continue, // dangling object number: drop the relation
                },
            };
            out.relations.push(GraphRelation {
                subject_type: subj.entity_type.clone(),
                subject_id: subj.entity_id,
                predicate,
                object_type,
                object_id,
                sentiment: r.sentiment.map(|s| s.clamp(-1.0, 1.0)),
                confidence,
            });
        }

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
            let kind_raw = p.kind.trim().to_lowercase();
            let kind = if PERSON_KINDS.contains(&kind_raw.as_str()) {
                kind_raw
            } else {
                "other".to_string()
            };
            let (tc_type, tc_id) = match p.team_context.and_then(|i| self.resolve(i)) {
                Some(c) if c.entity_type == "team" => {
                    (Some(c.entity_type.clone()), Some(c.entity_id))
                }
                _ => (None, None), // non-team or dangling context: keep person, drop tie
            };
            out.persons.push(GraphPerson {
                name,
                kind,
                team_context_type: tc_type,
                team_context_id: tc_id,
            });
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests;
