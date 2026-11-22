//! Composition roster for the first-party plugin fleet.

use crate::harness::route::RouteKey;
use crate::harness::skill::{PluginManifest, SkillManifest};

/// Concurrent Kev article requests. Four native slots share each request's question prefix.
pub const ARCHBOX_SLOTS: (&str, usize) = ("decision-d1", 4);
/// Shared slots for models hosted on the Mac. Slot-group membership must follow routing.
pub const MAC_SLOTS: (&str, usize) = ("characters-qwen", 4);

/// Domain plugin owners, for Capture and Characters.
pub const ALL: [&PluginManifest; 2] = [
    &crate::harness::plugins::capture::MANIFEST,
    &crate::harness::plugins::characters::MANIFEST,
];

/// Current deployable skills. Investigator and Fixture Boxscore remain offline.
pub fn skills() -> impl Iterator<Item = &'static SkillManifest> {
    std::iter::once(&crate::harness::plugins::capture::skills::classifier::manifest::MANIFEST)
        .chain(
            crate::harness::plugins::characters::MANIFEST
                .skills
                .iter()
                .copied(),
        )
}

/// Configured inference routes contributed by the statically linked plugin roster.
pub fn inference_routes() -> Vec<RouteKey> {
    let mut routes = Vec::new();
    for manifest in skills() {
        for &route in manifest.inference_routes {
            if !routes.contains(&route) {
                routes.push(route);
            }
        }
    }
    routes
}

#[cfg(test)]
mod tests;
