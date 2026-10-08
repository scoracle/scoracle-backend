//! Independent source qualification and native expression preparation.
//! Model-independent source contract, validation and expression preparation.
pub mod adapter;
pub mod delivery;
pub(crate) mod identity;
pub mod manifest;
pub mod prompt;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT: &str = "classifier-qualified-claims-v1";
const SCHEMA: &str = include_str!("../../fixtures/classifier/vector-schema-v1.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub article_id: i64,
    pub body: String,
    pub source: String,
    pub published_at: Option<String>,
    pub query_entities: Vec<Value>,
    #[serde(flatten)]
    pub provenance: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub quote: String,
}

/// Models copy text and identify its occurrence; Rust computes UTF-8 byte offsets.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub quote: String,
    pub occurrence: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claim<S> {
    pub evidence: S,
    pub target_relation: String,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub target_evidence: Option<Vec<S>>,
    pub kind: String,
    pub time_scope: String,
    pub candidate_dimensions: Vec<String>,
    pub qualifiers: BTreeMap<String, Option<Vec<S>>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub complete_source_review: bool,
    pub extraction_usable: bool,
    pub claims: Vec<Claim<Quote>>,
    #[serde(default)]
    pub measurements: Option<MeasurementProposal>,
}

/// Presence values are raw [0,1] model measurements, never calibrated labels.
/// Ordinal values use the schema's integer anchors. Null always means unknown.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, bound(deserialize = "S: Deserialize<'de>"))]
pub struct Measurement<S> {
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub value: Option<f64>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub evidence: Option<Vec<S>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementProposal {
    pub presence: BTreeMap<String, BTreeMap<String, Measurement<Quote>>>,
    pub ordinals: BTreeMap<String, Measurement<Quote>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Measurements {
    pub schema_version: String,
    pub article_id: i64,
    pub body_sha256: String,
    pub target: Value,
    pub source_extent: (usize, usize),
    pub calibration_status: String,
    pub presence: BTreeMap<String, BTreeMap<String, Measurement<Span>>>,
    pub ordinals: BTreeMap<String, Measurement<Span>>,
}

/// Fill the complete fixed envelope; a sparse model result never creates absence.
pub fn measurements(
    source: &Source,
    target: &Value,
    proposal: Option<MeasurementProposal>,
) -> Result<Measurements> {
    check_target(source, target)?;
    let schema: Value = serde_json::from_str(SCHEMA)?;
    let unknown = || Measurement {
        value: None,
        evidence: None,
    };
    let mut envelope = Measurements {
        schema_version: schema["version"]
            .as_str()
            .context("measurement schema")?
            .into(),
        article_id: source.article_id,
        body_sha256: hash(&source.body),
        target: target.clone(),
        source_extent: (0, source.body.len()),
        calibration_status: "unassessed".into(),
        presence: schema["vectors"]
            .as_object()
            .context("presence schema")?
            .iter()
            .map(|(family, spec)| {
                Ok((
                    family.clone(),
                    spec["labels"]
                        .as_object()
                        .context("presence labels")?
                        .keys()
                        .map(|name| (name.clone(), unknown()))
                        .collect(),
                ))
            })
            .collect::<Result<_>>()?,
        ordinals: schema["ordinal_vectors"]
            .as_object()
            .context("ordinal schema")?
            .keys()
            .map(|name| (name.clone(), unknown()))
            .collect(),
    };
    let bind_measurement = |measurement: Measurement<Quote>,
                            minimum: f64,
                            maximum: f64,
                            ordinal: bool|
     -> Result<Measurement<Span>> {
        if let Some(value) = measurement.value {
            ensure!(
                value.is_finite()
                    && value >= minimum
                    && value <= maximum
                    && (!ordinal || value.fract() == 0.0),
                "measurement outside schema range"
            );
            ensure!(
                (!ordinal && value == 0.0)
                    || measurement
                        .evidence
                        .as_ref()
                        .is_some_and(|spans| !spans.is_empty()),
                "positive presence or ordinal measurement requires source evidence"
            );
        } else {
            ensure!(
                measurement.evidence.is_none(),
                "unknown measurement cannot carry an asserted evidence selection"
            );
        }
        ensure!(
            measurement
                .evidence
                .as_ref()
                .is_none_or(|spans| !spans.is_empty()),
            "empty measurement evidence"
        );
        Ok(Measurement {
            value: measurement.value,
            evidence: bind_list(&source.body, measurement.evidence)?,
        })
    };
    if let Some(proposal) = proposal {
        for (family, dimensions) in proposal.presence {
            let known = envelope
                .presence
                .get_mut(&family)
                .context("unknown measurement family")?;
            for (name, measurement) in dimensions {
                let slot = known.get_mut(&name).context("unknown presence dimension")?;
                *slot = bind_measurement(measurement, 0.0, 1.0, false)?;
            }
        }
        for (name, measurement) in proposal.ordinals {
            let slot = envelope
                .ordinals
                .get_mut(&name)
                .context("unknown ordinal dimension")?;
            *slot = bind_measurement(
                measurement,
                schema["ordinal_vectors"][&name]["minimum"]
                    .as_f64()
                    .context("ordinal minimum")?,
                schema["ordinal_vectors"][&name]["maximum"]
                    .as_f64()
                    .context("ordinal maximum")?,
                true,
            )?;
        }
    }
    Ok(envelope)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub contract: String,
    pub article_id: i64,
    pub body_sha256: String,
    pub target: Value,
    pub complete_source_review: bool,
    pub extraction_usable: bool,
    pub review_status: String,
    pub reviewer: String,
    pub adjudicator: Option<String>,
    pub claims: Vec<Claim<Span>>,
}

fn hash(body: &str) -> String {
    hex::encode(Sha256::digest(body.as_bytes()))
}

fn check_target(source: &Source, target: &Value) -> Result<()> {
    ensure!(!source.body.trim().is_empty(), "missing retained source");
    ensure!(
        source.query_entities.contains(target)
            && target["name"]
                .as_str()
                .is_some_and(|name| !name.trim().is_empty()),
        "explicit candidate target required"
    );
    Ok(())
}

pub use prompt::{request, VERSION as PROMPT_VERSION};

fn bind(body: &str, quote: Quote) -> Result<Span> {
    ensure!(!quote.quote.trim().is_empty(), "empty source quote");
    let start = body
        .match_indices(&quote.quote)
        .nth(quote.occurrence)
        .map(|(start, _)| start)
        .context("quote occurrence absent from retained source")?;
    Ok(Span {
        start,
        end: start + quote.quote.len(),
        quote: quote.quote,
    })
}

fn bind_list(body: &str, quotes: Option<Vec<Quote>>) -> Result<Option<Vec<Span>>> {
    quotes
        .map(|quotes| quotes.into_iter().map(|quote| bind(body, quote)).collect())
        .transpose()
}

/// Decode one complete reply. Semantic correctness still requires independent review.
pub fn qualify(source: &Source, target: &Value, raw: &str) -> Result<Record> {
    check_target(source, target)?;
    let proposal: Proposal = serde_json::from_str(raw).context("qualification reply contract")?;
    measurements(source, target, proposal.measurements)?;
    let mut claims = Vec::new();
    for claim in proposal.claims {
        claims.push(Claim {
            evidence: bind(&source.body, claim.evidence)?,
            target_evidence: bind_list(&source.body, claim.target_evidence)?,
            qualifiers: claim
                .qualifiers
                .into_iter()
                .map(|(key, quotes)| Ok((key, bind_list(&source.body, quotes)?)))
                .collect::<Result<_>>()?,
            target_relation: claim.target_relation,
            kind: claim.kind,
            time_scope: claim.time_scope,
            candidate_dimensions: claim.candidate_dimensions,
        });
    }
    let record = Record {
        contract: CONTRACT.into(),
        article_id: source.article_id,
        body_sha256: hash(&source.body),
        target: target.clone(),
        complete_source_review: proposal.complete_source_review,
        extraction_usable: proposal.extraction_usable,
        review_status: "ai_provisional".into(),
        reviewer: "codex-ai-provisional".into(),
        adjudicator: None,
        claims,
    };
    validate(source, &record)?;
    Ok(record)
}

fn span_valid(body: &str, span: &Span) -> Result<()> {
    ensure!(
        span.start < span.end
            && !span.quote.trim().is_empty()
            && body.get(span.start..span.end) == Some(span.quote.as_str()),
        "not an exact model-visible source span"
    );
    Ok(())
}

/// Source binding proves literal integrity, not the meaning of model relationships.
pub fn validate(source: &Source, record: &Record) -> Result<()> {
    check_target(source, &record.target)?;
    ensure!(
        record.contract == CONTRACT
            && record.article_id == source.article_id
            && record.body_sha256 == hash(&source.body)
            && record.complete_source_review
            && record.extraction_usable,
        "complete source-bound qualification with usable extraction required"
    );
    ensure!(
        !record.reviewer.trim().is_empty(),
        "named reviewer required"
    );
    match record.review_status.as_str() {
        "ai_provisional" => ensure!(
            record.reviewer == "codex-ai-provisional" && record.adjudicator.is_none(),
            "provisional review cannot be adjudicated"
        ),
        "reviewed" => ensure!(record.adjudicator.is_none(), "premature adjudicator"),
        "adjudicated" => ensure!(
            record
                .adjudicator
                .as_ref()
                .is_some_and(|name| !name.trim().is_empty() && name != &record.reviewer),
            "independent adjudicator required"
        ),
        _ => anyhow::bail!("incomplete or unknown claim review"),
    }
    let schema: Value = serde_json::from_str(SCHEMA)?;
    let keys: BTreeSet<&str> = schema["qualifiers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap())
        .collect();
    let labels = schema["vectors"]["emotion"]["labels"].as_object().unwrap();
    let mut seen = BTreeSet::new();
    for claim in &record.claims {
        span_valid(&source.body, &claim.evidence)?;
        ensure!(
            seen.insert((claim.evidence.start, claim.evidence.end)),
            "duplicate claim span"
        );
        ensure!(
            ["direct_subject", "other_subject", "unknown"]
                .contains(&claim.target_relation.as_str())
                && [
                    "emotion",
                    "emotion_denial",
                    "conditional_emotion",
                    "report_denial",
                    "withdrawn",
                    "information"
                ]
                .contains(&claim.kind.as_str())
                && ["current", "historical", "future", "unknown"]
                    .contains(&claim.time_scope.as_str())
                && claim
                    .qualifiers
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                    == keys,
            "invalid claim relation, kind, time or qualifiers"
        );
        ensure!(
            claim
                .candidate_dimensions
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == claim.candidate_dimensions.len()
                && claim
                    .candidate_dimensions
                    .iter()
                    .all(|label| labels.contains_key(label)),
            "invalid emotion label identities"
        );
        for spans in std::iter::once(&claim.target_evidence)
            .chain(claim.qualifiers.values())
            .flatten()
        {
            ensure!(
                !spans.is_empty(),
                "qualification must be unknown or nonempty exact spans"
            );
            for span in spans {
                span_valid(&source.body, span)?;
            }
        }
        if claim.target_relation == "direct_subject" {
            ensure!(
                claim.target_evidence.is_some(),
                "target relationship requires source evidence"
            );
        }
        if ["emotion", "emotion_denial"].contains(&claim.kind.as_str()) {
            ensure!(
                claim.qualifiers["speaker"].is_some() && claim.qualifiers["subject"].is_some(),
                "emotional claim requires speaker and subject"
            );
        }
        let required = match claim.kind.as_str() {
            "emotion_denial" | "report_denial" => Some("negation"),
            "conditional_emotion" => Some("uncertainty"),
            "withdrawn" => Some("source_disagreement"),
            _ => None,
        };
        if let Some(key) = required {
            ensure!(
                claim.qualifiers[key].is_some(),
                "claim requires {key} evidence"
            );
        }
        if claim.time_scope != "unknown" {
            ensure!(
                claim.qualifiers["reported_event_time"].is_some(),
                "known claim time requires source evidence"
            );
        }
    }
    Ok(())
}

fn paragraphs(body: &str, support: &[&Span]) -> Vec<Span> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    for line in body.split_inclusive('\n').chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if support
                .iter()
                .any(|span| start < span.end && span.start < cursor)
            {
                result.push(Span {
                    start,
                    end: cursor,
                    quote: body[start..cursor].into(),
                });
            }
            start = cursor + line.len();
        }
        cursor += line.len();
    }
    result
}

/// Provisional emotional world for evaluation, with unresolved decisions explicit.
/// No full-spectrum no-action decision or production scheduling is made here.
pub fn emotional_world(source: &Source, record: &Record) -> Result<Value> {
    validate(source, record)?;
    let emotional =
        |claim: &&Claim<Span>| ["emotion", "emotion_denial"].contains(&claim.kind.as_str());
    let selected: Vec<_> = record
        .claims
        .iter()
        .filter(emotional)
        .filter(|claim| claim.target_relation == "direct_subject" && claim.time_scope != "unknown")
        .collect();
    let unresolved = record.claims.iter().filter(emotional).any(|claim| {
        claim.target_relation == "unknown"
            || (claim.target_relation == "direct_subject" && claim.time_scope == "unknown")
    });
    let support: Vec<_> = selected
        .iter()
        .flat_map(|claim| {
            std::iter::once(&claim.evidence)
                .chain(claim.target_evidence.iter().flatten())
                .chain(claim.qualifiers.values().flatten().flatten())
        })
        .collect();
    Ok(json!({
        "selection_status": if unresolved { "unresolved" } else if selected.is_empty() { "no_eligible_emotional_claims" } else { "candidate_evidence" },
        "production_eligible": false,
        "expression_world": if unresolved { Value::Null } else { json!({
            "TARGET": record.target, "RELEVANT HISTORY": [],
            "FRESH EVIDENCE": [{"publisher": source.source, "published_at": source.published_at}],
            "SOURCE CONTEXT": paragraphs(&source.body, &support).iter().map(|span| &span.quote).collect::<Vec<_>>(),
            "QUALIFIED CLAIMS": selected.iter().map(|claim| {
                let mut result = serde_json::Map::new();
                result.insert("publisher_text".into(), json!(claim.evidence.quote));
                result.insert("time_scope".into(), json!(claim.time_scope));
                for (key, spans) in &claim.qualifiers {
                    if let Some(spans) = spans {
                        result.insert(key.clone(), json!(spans.iter().map(|span| &span.quote).collect::<Vec<_>>()));
                    }
                }
                Value::Object(result)
            }).collect::<Vec<_>>()
        }) }
    }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod plumbing_tests;
