//! Decision-model work and its source-bound delivery contract.
use crate::harness::skill::{Plugin, PluginManifest};
use anyhow::Result;
use sqlx::PgPool;
use std::{collections::HashSet, sync::Arc};
pub mod skills;
use skills::{classifier, fixture_boxscore, investigator};

pub const ID: &str = "scoracle.capture";

pub const MANIFEST: PluginManifest = PluginManifest {
    id: ID,
    skills: &[
        &classifier::manifest::MANIFEST,
        &fixture_boxscore::manifest::MANIFEST,
        &investigator::manifest::MANIFEST,
    ],
};

pub fn build(pool: PgPool, enabled: &HashSet<String>) -> Result<Plugin> {
    let mut skills: Vec<Arc<dyn crate::harness::skill::Skill>> = Vec::new();
    if enabled.contains(classifier::manifest::TASK.as_str()) {
        skills.push(Arc::new(classifier::adapter::ClassifierHandler::new(
            pool,
            classifier::decision::Decision::from_env()?,
        )));
    }
    Ok(Plugin {
        manifest: &MANIFEST,
        skills,
    })
}
