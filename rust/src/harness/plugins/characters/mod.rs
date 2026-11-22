//! Reader-facing character outputs, independently scheduled as skills.
use crate::harness::models::Models;
use crate::harness::skill::{Plugin, PluginManifest, Skill};
use anyhow::Result;
use sqlx::PgPool;
use std::{collections::HashSet, sync::Arc};

pub mod skills;
use skills::{analyst, influencer, insider, journalist, oracle, scout};

pub const ID: &str = "scoracle.character";

pub const MANIFEST: PluginManifest = PluginManifest {
    id: ID,
    skills: &[
        &journalist::manifest::MANIFEST,
        &influencer::manifest::MANIFEST,
        &scout::manifest::MANIFEST,
        &insider::manifest::MANIFEST,
        &analyst::manifest::MANIFEST,
        &oracle::manifest::MANIFEST,
    ],
};

pub fn build(pool: PgPool, models: &Models, enabled: &HashSet<String>) -> Result<Plugin> {
    let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
    // News voices precede statistics; Momentum follows rating/vibe; Sigil is terminal.
    for manifest in MANIFEST.skills {
        if !enabled.contains(manifest.task.as_str()) {
            continue;
        }
        let capabilities = models.capabilities(manifest)?;
        skills.push(match manifest.task {
            journalist::manifest::TASK => Arc::new(journalist::NarrativesHandler::new(
                pool.clone(),
                capabilities,
            )),
            influencer::manifest::TASK => {
                Arc::new(influencer::VibeHandler::new(pool.clone(), capabilities))
            }
            scout::manifest::TASK => {
                Arc::new(scout::RatingHandler::new(pool.clone(), capabilities))
            }
            insider::manifest::TASK => {
                Arc::new(insider::TransferHandler::new(pool.clone(), capabilities))
            }
            analyst::manifest::TASK => {
                Arc::new(analyst::MomentumHandler::new(pool.clone(), capabilities))
            }
            oracle::manifest::TASK => {
                Arc::new(oracle::SigilHandler::new(pool.clone(), capabilities))
            }
            _ => unreachable!("Character manifest contains only its six skills"),
        });
    }
    Ok(Plugin {
        manifest: &MANIFEST,
        skills,
    })
}
