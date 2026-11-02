//! Decision criteria and target binding; the decision adapter owns inference.
use anyhow::{ensure, Result};
use serde_json::Value;

pub(crate) const CRITERIA: &str =
    include_str!("../../../fixtures/classifier/classification-criteria-v10-availability.json");

pub fn request(headline: &str, publisher: &str, sport: &str, meta: &Value) -> Result<Value> {
    ensure!(
        !headline.trim().is_empty() && !headline.contains('\0'),
        "missing or invalid RSS headline"
    );
    ensure!(!publisher.contains('\0'), "invalid RSS publisher");
    let entity: crate::tools::meta::EntityMeta = serde_json::from_value(meta["identity"].clone())?;
    ensure!(
        entity.sport == sport && entity.entity_id > 0 && !entity.name.trim().is_empty(),
        "invalid Classifier subject metadata"
    );
    // Bind identifying attributes into the question; context-only gender did not separate teams.
    let subject = match meta.get("team_gender").filter(|gender| !gender.is_null()) {
        Some(gender) => {
            ensure!(
                entity.entity_type == "team",
                "team_gender requires a team subject"
            );
            let category = match gender.as_str() {
                Some("men") => "men's",
                Some("women") => "women's",
                Some("mixed") => "mixed-gender",
                _ => anyhow::bail!("team_gender must be men, women, mixed, or null"),
            };
            format!(
                "the {category} {} team named {}",
                entity.sport.to_lowercase(),
                entity.name
            )
        }
        None => entity.reference(),
    };
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    crate::tools::prompt::decision(
        &subject,
        super::fresh::context(headline, publisher, sport, meta),
        criteria["questions"].clone(),
    )
}
