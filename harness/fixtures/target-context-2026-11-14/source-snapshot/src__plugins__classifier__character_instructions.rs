//! The assignment and four criteria for a headline–target decision.
use crate::tools::{meta, prompt};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub(crate) const CRITERIA: &str =
    include_str!("../../../fixtures/classifier/classification-criteria-v13-relations.json");

const INSTRUCTIONS: prompt::Instructions = prompt::Instructions {
    task: "Answer the question using the headline below. Decide whether what it reports concerns the specified target; a name appearing in the headline is not enough.",
    rules: "",
};

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
    let mut criteria: Value = serde_json::from_str(CRITERIA)?;
    let questions = criteria["questions"].as_object_mut().context("questions")?;
    for question in questions.values_mut() {
        question["instructions"] = json!(question["instructions"]
            .as_str()
            .context("instructions")?
            .replace("{subject}", &subject));
    }
    let state = prompt::assemble_brief(
        &INSTRUCTIONS,
        target,
        super::fresh::context(headline, publisher),
    );
    // d3 renders string state verbatim before the native question.
    Ok(json!({"state":state,"questions":questions}))
}
