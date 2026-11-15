//! Decision-model work and its source-bound delivery contract.
use crate::harness::plugin::{Plugin, PluginManifest};
use anyhow::Result;
use sqlx::PgPool;
use std::{collections::HashSet, sync::Arc};
pub mod skills;
use skills::classify;

pub const ID: &str = "scoracle.classifier";

pub const MANIFEST: PluginManifest = PluginManifest {
    id: ID,
    skills: &[&classify::manifest::MANIFEST],
};

pub fn build(pool: PgPool, enabled: &HashSet<String>) -> Result<Plugin> {
    let mut skills: Vec<Arc<dyn crate::harness::plugin::Skill>> = Vec::new();
    if enabled.contains(classify::manifest::TASK.as_str()) {
        skills.push(Arc::new(classify::adapter::ClassifierHandler::new(
            pool,
            classify::decision::Decision::from_env()?,
        )));
    }
    Ok(Plugin {
        manifest: &MANIFEST,
        skills,
    })
}
