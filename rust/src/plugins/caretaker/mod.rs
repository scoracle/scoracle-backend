//! Existing canonical-data maintenance machinery. No skills are activated yet.
use crate::harness::plugin::{Plugin, PluginManifest};
pub mod skills;

pub const ID: &str = "scoracle.caretaker";

pub const MANIFEST: PluginManifest = PluginManifest {
    id: ID,
    skills: &[
        &skills::investigator::manifest::MANIFEST,
        &skills::fixture_boxscore::manifest::MANIFEST,
    ],
};

pub fn build() -> Plugin {
    Plugin {
        manifest: &MANIFEST,
        skills: Vec::new(),
    }
}
