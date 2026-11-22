//! Assemble the Classifier task, target, headline and native scoring questions.
use super::character_instructions::{INSTRUCTIONS, QUESTIONS};
use crate::tools::{meta, prompt};
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::{json, Value};

pub fn request(headline: &str, publisher: &str, sport: &str, meta: &Value) -> Result<Value> {
    ensure!(
        !headline.trim().is_empty() && !headline.contains('\0'),
        "missing or invalid RSS headline"
    );
    ensure!(!publisher.contains('\0'), "invalid RSS publisher");
    let entity: meta::EntityMeta = serde_json::from_value(meta["identity"].clone())?;
    ensure!(
        entity.sport == sport && entity.entity_id > 0 && !entity.name.trim().is_empty(),
        "invalid Classifier subject metadata"
    );
    let (target, subject) = meta::decision_context(&entity, meta)?;
    let questions: serde_json::Map<String, Value> = QUESTIONS
        .iter()
        .map(|(name, instructions)| {
            (
                (*name).to_owned(),
                json!({"type":"noul", "instructions":instructions.replace("{subject}", &subject)}),
            )
        })
        .collect();
    #[derive(Serialize)]
    struct Task<'a> {
        assignment: &'a str,
        rules: Vec<&'a str>,
    }
    #[derive(Serialize)]
    struct Brief<'a, T, E> {
        task: Task<'a>,
        target_context: &'a str,
        target: T,
        evidence_context: &'a str,
        evidence: E,
    }
    let evidence = super::fresh::context(headline, publisher);
    let state = serde_json::to_string_pretty(&Brief {
        target_context: prompt::TARGET_CONTEXT,
        target: target.data,
        task: Task {
            assignment: INSTRUCTIONS.job(),
            rules: INSTRUCTIONS.rules.lines().collect(),
        },
        evidence_context: evidence.guide.relationship,
        evidence: evidence.data,
    })?;
    // d3 renders string state verbatim before the native question.
    Ok(json!({"state":state,"questions":questions}))
}
