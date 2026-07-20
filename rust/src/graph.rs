//! Graph — the typed narrative extraction primitive (Plan - Narrative Graph, roadmap
//! item 4). Reads one scrubbed article plus its ALREADY-VETTED entities and extracts:
//!
//!   * typed relations among the listed entities (the six-predicate vocabulary of
//!     `narrative_events`, mig 154) with sentiment and confidence — the language signal
//!     that led volume by five weeks in the Rogers test, headed for typed
//!     `narrative_links` and the transfer-likelihood fusion;
//!   * person discoveries — coaches/agents/executives named in the article but absent
//!     from the seeded entity world (`narrative_persons` candidates).
//!
//! CLOSED CANDIDATE LIST: the model never resolves free-text entity names. It picks
//! subjects/objects by NUMBER from the vetted list (the resolve.rs trick), so the
//! Stage-6 entity-resolution risk of the original kickoff plan simply does not exist
//! here — scrub already did the resolving, and the ONLY novel names the model may emit
//! are person discoveries, which land as narrative_persons CANDIDATES (evidence-gated
//! promotion, never direct entityhood).
//!
//! Fail-closed (the §1.2 invariant): an unparseable reply is `None` — the caller writes
//! nothing. Individually invalid relations/persons are dropped, not repaired; a partial
//! salvage of valid entries from a valid JSON body is allowed (mirrors the scrub
//! parser's out-of-range index handling).
//!
//! This module is the PRIMITIVE (types, prompt, parser) — pure and unit-tested. The
//! stage handler (queue claim, debounce, `narrative_events` upsert, person evidence
//! accumulation) composes it; `examples/graph_probe.rs` is the measured probe run
//! BEFORE any wiring, per house culture.

use crate::harness::Parser;
use crate::ollama::GenerateOptions;
use anyhow::Result;
use serde::Deserialize;

pub const GRAPH_PROMPT_VERSION: &str = "g2"; // g2: person-extraction emphasis + object-attachment rule (probe 2026-07-19: g1 found 0 persons across 8 articles with Tuchel/Alonso present; one Rogers relation attached to the wrong club)

/// The six-predicate vocabulary — MUST mirror the `narrative_events_predicate_check`
/// constraint (mig 154). Grow both together, by migration, with eval evidence.
pub const PREDICATES: &[&str] = &[
    "trade_rumor",
    "trade_confirmed",
    "injury",
    "contract_dispute",
    "praise",
    "criticism",
];

/// Person kinds — mirrors `narrative_persons_kind_check` (mig 154). An out-of-vocabulary
/// role guess maps to "other" rather than dropping the discovery (the promotion gate,
/// not the extractor, decides who becomes an entity).
pub const PERSON_KINDS: &[&str] = &["coach", "agent", "executive", "family", "other"];

const GRAPH_SYSTEM_PROMPT: &str = "Task: extract structured narrative relations from one sports article.\n\nYou are given the article and a NUMBERED list of known entities the article is about (already verified). Extract:\n\n1. \"relations\": relations the article STATES OR CLEARLY IMPLIES between listed entities, or about one listed entity alone. Use entity NUMBERS only. Allowed predicates: trade_rumor, trade_confirmed, injury, contract_dispute, praise, criticism.\n   - subject: entity number (required). object: the number of the OTHER listed entity the relation is actually WITH — for a transfer, the club the player is joining or leaving per THIS article, not just any club mentioned. Use null ONLY when no listed entity is the counterparty.\n   - sentiment: -1.0 (very negative for the subject) to 1.0 (very positive). \n   - confidence: \"speculative\" (unsourced/rumored), \"reported\" (attributed to a source), \"confirmed\" (official/announced).\n   - Extract only what the text supports. No relation is the correct output for many articles.\n\n2. \"persons\": EVERY person named in the article text who is NOT on the entity list and is not a player. Head coaches and managers named in the text ALWAYS belong here (e.g. a manager mentioned in a transfer story). So do agents, sporting directors/executives, and family members. Use their name exactly as written.\n   - kind: coach | agent | executive | family | other.\n   - team_context: the number of the listed TEAM they are tied to, or null.\n   Never list players here; never list people not named in the text. An empty persons list is WRONG whenever a coach or manager is named in the text.\n\nReturn ONLY this JSON object, no commentary:\n{\"relations\":[{\"subject\":1,\"predicate\":\"trade_rumor\",\"object\":2,\"sentiment\":0.0,\"confidence\":\"reported\"}],\"persons\":[{\"name\":\"...\",\"kind\":\"coach\",\"team_context\":2}]}";

/// The model budget for one extraction call. Temperature 0.2 (tight but a judgment
/// call, matching scrub adjudication); JSON mode tightens contract adherence.
pub fn graph_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(GRAPH_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: 768,
        num_ctx: 0,
        json_mode: true,
        format_schema: None,
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
    b.push_str(&format!("Article source: {source}\nPublished: {published}\n"));
    b.push_str(&format!("Title: {title}\n"));
    if !description.trim().is_empty() {
        b.push_str(&format!("Text: {description}\n"));
    }
    b.push_str("\nKnown entities (use these numbers):\n");
    for (i, c) in candidates.iter().enumerate() {
        b.push_str(&format!("{}. {}\n", i + 1, c.descriptor));
    }
    b.push_str("\nReturn the JSON now.");
    b
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
mod tests {
    use super::*;

    fn candidates() -> Vec<GraphCandidate> {
        vec![
            GraphCandidate {
                entity_type: "player".into(),
                entity_id: 10,
                descriptor: "Morgan Rogers, an English midfielder, currently at Aston Villa."
                    .into(),
            },
            GraphCandidate {
                entity_type: "team".into(),
                entity_id: 18,
                descriptor: "Chelsea (team)".into(),
            },
        ]
    }

    #[test]
    fn parser_fail_closed() {
        let c = candidates();
        let p = GraphParser { candidates: &c };
        assert!(p.parse("").unwrap().is_none());
        assert!(p.parse("I could not find relations.").unwrap().is_none());
        assert!(p.parse("{not json").unwrap().is_none());
    }

    #[test]
    fn parser_valid_relation_resolves_numbers() {
        let c = candidates();
        let p = GraphParser { candidates: &c };
        let out = p
            .parse(
                r#"{"relations":[{"subject":1,"predicate":"trade_rumor","object":2,"sentiment":0.4,"confidence":"reported"}],"persons":[]}"#,
            )
            .unwrap()
            .unwrap();
        assert_eq!(out.relations.len(), 1);
        let r = &out.relations[0];
        assert_eq!((r.subject_type.as_str(), r.subject_id), ("player", 10));
        assert_eq!(
            (r.object_type.as_deref(), r.object_id),
            (Some("team"), Some(18))
        );
        assert_eq!(r.predicate, "trade_rumor");
        assert_eq!(r.confidence, "reported");
    }

    #[test]
    fn parser_drops_invalid_entries_keeps_valid() {
        let c = candidates();
        let p = GraphParser { candidates: &c };
        let out = p
            .parse(
                r#"{"relations":[
                    {"subject":9,"predicate":"injury","object":null,"sentiment":0,"confidence":"reported"},
                    {"subject":1,"predicate":"levitation","object":2,"sentiment":0,"confidence":"reported"},
                    {"subject":1,"predicate":"praise","object":7,"sentiment":0,"confidence":"reported"},
                    {"subject":1,"predicate":"injury","object":null,"sentiment":-9,"confidence":"maybe"},
                    {"subject":1,"predicate":"injury","object":null,"sentiment":-9,"confidence":"reported"}
                ],"persons":[]}"#,
            )
            .unwrap()
            .unwrap();
        // Only the last survives: bad subject, bad predicate, dangling object, bad
        // confidence are each dropped; sentiment -9 clamps to -1.
        assert_eq!(out.relations.len(), 1);
        assert_eq!(out.relations[0].sentiment, Some(-1.0));
    }

    #[test]
    fn parser_person_discovery_rules() {
        let c = candidates();
        let p = GraphParser { candidates: &c };
        let out = p
            .parse(
                r#"{"relations":[],"persons":[
                    {"name":"Enzo Maresca","kind":"coach","team_context":2},
                    {"name":"Enzo Maresca","kind":"coach","team_context":2},
                    {"name":"","kind":"agent","team_context":null},
                    {"name":"Morgan Rogers","kind":"other","team_context":null},
                    {"name":"Rafaela Pimenta","kind":"superagent","team_context":1}
                ]}"#,
            )
            .unwrap()
            .unwrap();
        // Duplicate, empty, and already-listed names drop; unknown kind maps to
        // "other"; a PLAYER team_context is dropped (kept person, no tie).
        assert_eq!(out.persons.len(), 2);
        assert_eq!(out.persons[0].name, "Enzo Maresca");
        assert_eq!(out.persons[0].kind, "coach");
        assert_eq!(out.persons[0].team_context_id, Some(18));
        assert_eq!(out.persons[1].name, "Rafaela Pimenta");
        assert_eq!(out.persons[1].kind, "other");
        assert_eq!(out.persons[1].team_context_id, None);
    }

    #[test]
    fn prompt_numbers_candidates() {
        let c = candidates();
        let prompt = build_graph_prompt("skysports", "2026-07-19", "Title here", "Body", &c);
        assert!(prompt.contains("1. Morgan Rogers"));
        assert!(prompt.contains("2. Chelsea (team)"));
        assert!(prompt.contains("Return the JSON now."));
    }
}
