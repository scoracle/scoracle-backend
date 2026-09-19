//! The three inputs to a character's reading: voice, form and entity memories.
//!
//! `characters` defines the perspective; `form` defines the canvas; `memories`
//! supplies sourced context. Junctions gather new evidence and execute the call.
//! Live writers load these memories before hashing their inputs; offline probes
//! can inspect the same package and compose it with separately supplied evidence.

pub mod characters;
pub mod form;
pub mod guards;
pub mod memories;

use anyhow::Result;
use memories::{Mission, Package};

/// Provider-independent system and user inputs. Runtime options and persistence
/// remain with the junction and harness.
pub struct CardPrompt {
    pub system: String,
    pub prompt: String,
}

pub fn compose_card(memories: &Package, new_evidence: &str) -> Result<CardPrompt> {
    let system = match memories.mission {
        Mission::Scout => &crate::studio::scout::RATING_SYSTEM_PROMPT,
        Mission::Analyst => &characters::analyst::MOMENTUM_SYSTEM_PROMPT,
        Mission::Journalist => &crate::studio::journalist::NARRATIVES_SYSTEM_PROMPT,
        Mission::Influencer => &crate::studio::influencer::VIBE_SYSTEM_PROMPT,
        Mission::Insider => &crate::studio::insider::INSIDER_SCORE_SYSTEM_PROMPT,
        Mission::Oracle => &crate::studio::oracle::ORACLE_SYSTEM_PROMPT,
        _ => anyhow::bail!("internal extraction missions use their junction's task contract"),
    };
    let mut prompt = memories.render_for_model()?;
    if !new_evidence.trim().is_empty() {
        prompt.push_str("\nNew evidence for this reading:\n");
        prompt.push_str(new_evidence);
        prompt.push('\n');
    }
    Ok(CardPrompt {
        system: system.to_string(),
        prompt,
    })
}
