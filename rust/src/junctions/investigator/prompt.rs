//! The Investigator's prose-triage contract (PLAN-one-rail 5.4's deferred arm, built
//! 2026-08-09) — the seat's FIRST model-called path, for the D-T8 class: a name the news
//! wrote one way ("Airious Bailey") that Wikipedia knows another ("Ace Bailey"), where the
//! Wikidata entity search refuses honestly because it matches labels and aliases only.
//!
//! **The whole contract is VERBATIM QUOTES, and that is a security property, not a style:**
//! every field the model returns is a contiguous run of characters from the page it was
//! shown, so the gate verifies each one by normalized substring containment
//! ([`super::gate::contains_normalized`]) before it can influence anything. A hallucinated
//! occupation or team fails containment and the field is treated as absent. This is the
//! ep6/D-T45 discipline (grammar pins shape; prose carries meaning; a worked example
//! carries what prose cannot) plus PLAN-one-rail's T2 rule for this arm, fixed before it
//! was built: *ask for a verbatim observation and threshold on a count — never ask for
//! confidence.* The model copies; code decides.
//!
//! | | |
//! |---|---|
//! | **Seat** | `Role::Investigator` — `ministral-3:3b` on archbox (all seats, since 2026-08-20) |
//! | **Reads** | one Wikipedia REST page summary (title + description + extract) |
//! | **Writes** | nothing — [`super::gate::decide_prose`] and the handler own every write |

use crate::ollama::GenerateOptions;
use crate::util::truncate;
use serde::Deserialize;

/// Contract version for the prose arm — recorded on `acquisition_runs` rows this path
/// produces (`model_version` is the runner's answer; this names the PROMPT contract).
pub const INVESTIGATOR_PROSE_CONTRACT_VERSION: &str = "ip1";

pub const INVESTIGATOR_PROSE_SYSTEM_PROMPT: &str = r#"Read one Wikipedia page summary and copy out what it says. A fact-checker verifies every field by exact substring match against the page text, so every field must be COPIED VERBATIM — a field that is not a contiguous run of characters from the page fails and is discarded. Use only the page shown; never your own knowledge of the person.

A news article mentioned a name the newsroom does not recognise (the SOUGHT NAME below). This page may describe the same person under a different name. You copy what the page says; code decides what it means.

subject_kind — what the page is about: a person, a club, or anything else.

sought_name_evidence — the shortest run of page text showing the sought name and the page's subject are the same person — typically the full or legal name in the page's opening words. Empty string if the page never connects the sought name to its subject.

occupation_phrase — the page's own words for what the subject is or does — "American college basketball player", "Spanish football manager". Empty string if the page states none.

team_names — every team the page ties the subject to, each copied exactly as the page writes it. Empty array when the page names none.

Example, for sought name "Airious Bailey" on a page opening "Airious \"Ace\" Bailey Jr. (born August 28, 2006) is an American college basketball player for the Tennessee Volunteers.":
{"subject_kind":"person","sought_name_evidence":"Airious \"Ace\" Bailey Jr.","occupation_phrase":"American college basketball player","team_names":["Tennessee Volunteers"]}

Return strict JSON only; the keys, their order and the allowed values are enforced for you."#;

/// FIELD ORDER IS THE CONTRACT (the ar4/ep1 lesson): a RAW literal POSTed byte-for-byte,
/// because a schema that travels as a `serde_json::Value` reaches Ollama alphabetized.
/// `subject_kind` first — the cheap triage — then pure extraction. Enums and bounds here
/// are FREE (the schema compiles to a grammar and never enters the context window).
pub const INVESTIGATOR_PROSE_SCHEMA_RAW: &str = r#"{
    "type": "object",
    "properties": {
        "subject_kind": { "type": "string", "enum": ["person", "club", "other"] },
        "sought_name_evidence": { "type": "string", "maxLength": 160 },
        "occupation_phrase": { "type": "string", "maxLength": 90 },
        "team_names": {
            "type": "array",
            "maxItems": 6,
            "items": { "type": "string", "maxLength": 60 }
        }
    },
    "required": ["subject_kind", "sought_name_evidence", "occupation_phrase", "team_names"]
}"#;

/// The model budget for one prose read. The summary extract is lede-sized (rarely over
/// 1,500 chars), so 4096 ctx is roomy; 300 predict covers the four short fields.
pub fn prose_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(INVESTIGATOR_PROSE_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.1),
        num_predict: 300,
        num_ctx: crate::route::LOCAL_STAGE_NUM_CTX,
        json_mode: false,
        format_schema: Some(
            serde_json::from_str(INVESTIGATOR_PROSE_SCHEMA_RAW)
                .expect("INVESTIGATOR_PROSE_SCHEMA_RAW is valid JSON (unit-tested)"),
        ),
        format_schema_raw: Some(INVESTIGATOR_PROSE_SCHEMA_RAW.to_string()),
    }
}

/// The parsed prose read — extraction only; every judgment is derived in
/// [`super::gate::decide_prose`] (T2).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProseRead {
    #[serde(default)]
    pub subject_kind: String,
    #[serde(default)]
    pub sought_name_evidence: String,
    #[serde(default)]
    pub occupation_phrase: String,
    #[serde(default)]
    pub team_names: Vec<String>,
}

pub struct ProseReadParser;

impl crate::harness::Parser<ProseRead> for ProseReadParser {
    fn parse(&self, raw: &str) -> anyhow::Result<Option<ProseRead>> {
        let Some(slice) = json_object_slice(raw) else {
            return Ok(None);
        };
        let mut read: ProseRead = serde_json::from_str(slice)
            .map_err(|e| anyhow::anyhow!("parse prose read (raw={:?}): {e}", truncate(raw, 200)))?;
        read.subject_kind = normalize_space(&read.subject_kind).to_lowercase();
        read.sought_name_evidence = normalize_space(&read.sought_name_evidence);
        read.occupation_phrase = normalize_space(&read.occupation_phrase);
        read.team_names = read
            .team_names
            .into_iter()
            .map(|t| normalize_space(&t))
            .filter(|t| !t.is_empty())
            .take(6)
            .collect();
        Ok(Some(read))
    }
}

/// The page text the model is shown, ONE definition — the gate's containment checks run
/// against exactly this same concatenation, so "verbatim from the page" and "contained in
/// the page" can never drift apart.
pub fn page_text(title: &str, description: &str, extract: &str) -> String {
    format!("{title}\n{description}\n{extract}")
}

/// Budget for the extract slice of the user prompt. Ledes are short; this is a guard, not
/// a squeeze (4096 ctx − ~700-token system − 300 predict leaves ~2,900 tokens).
const PROSE_MAX_EXTRACT_CHARS: usize = 4_000;

pub fn build_prose_prompt(
    sought_name: &str,
    descriptor: Option<&str>,
    sport: &str,
    title: &str,
    description: &str,
    extract: &str,
) -> String {
    let mut p = String::new();
    p.push_str(&format!("Sought name: {sought_name}\n"));
    if let Some(d) = descriptor.filter(|d| !d.trim().is_empty()) {
        p.push_str(&format!("How the news article described them: {d}\n"));
    }
    p.push_str(&format!("Sport in question: {sport}\n"));
    p.push_str("\nWikipedia page summary:\n");
    p.push_str(&truncate(
        &normalize_space(&page_text(title, description, extract)),
        PROSE_MAX_EXTRACT_CHARS,
    ));
    p.push_str("\n\nReturn the JSON object now.");
    p
}

fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Same brace-matching slice as the Editor's parser — grammar-constrained output should be
/// bare JSON, but a runner that wraps it must not break the parse.
fn json_object_slice(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if *b == b'\\' {
                esc = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        match *b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start.map(|s| &raw[s..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Parser;

    #[test]
    fn schema_is_valid_json_and_order_true_in_the_raw_literal() {
        let v: serde_json::Value = serde_json::from_str(INVESTIGATOR_PROSE_SCHEMA_RAW).unwrap();
        assert_eq!(v["required"].as_array().unwrap().len(), 4);
        // The raw literal is what Ollama receives; assert the byte order directly.
        let k = |s: &str| INVESTIGATOR_PROSE_SCHEMA_RAW.find(s).unwrap();
        assert!(k("subject_kind") < k("sought_name_evidence"));
        assert!(k("sought_name_evidence") < k("occupation_phrase"));
        assert!(k("occupation_phrase") < k("team_names"));
    }

    #[test]
    fn parser_normalizes_and_drops_empty_teams() {
        let raw = r#"{"subject_kind":"Person","sought_name_evidence":"  Airious \"Ace\"  Bailey Jr. ","occupation_phrase":"American  college basketball player","team_names":["Tennessee   Volunteers",""]}"#;
        let read = ProseReadParser.parse(raw).unwrap().unwrap();
        assert_eq!(read.subject_kind, "person");
        assert_eq!(read.sought_name_evidence, "Airious \"Ace\" Bailey Jr.");
        assert_eq!(read.occupation_phrase, "American college basketball player");
        assert_eq!(read.team_names, vec!["Tennessee Volunteers"]);
    }

    #[test]
    fn prompt_carries_the_descriptor_only_when_present() {
        let with = build_prose_prompt("Airious Bailey", Some("Tennessee freshman"), "NBA",
            "Ace Bailey", "American basketball player", "Airious \"Ace\" Bailey Jr. ...");
        assert!(with.contains("How the news article described them: Tennessee freshman"));
        let without = build_prose_prompt("Airious Bailey", None, "NBA", "t", "d", "e");
        assert!(!without.contains("How the news article described"));
    }
}
