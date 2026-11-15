//! Composition roster for the first-party plugin fleet.

use crate::harness::plugin::{PluginManifest, SkillManifest};
use crate::harness::route::RouteKey;

/// Concurrent Kev article requests. Four native slots share each request's question prefix.
pub const ARCHBOX_SLOTS: (&str, usize) = ("decision-d1", 4);
/// Shared slots for models hosted on the Mac. Slot-group membership must follow routing.
pub const MAC_SLOTS: (&str, usize) = ("characters-qwen", 4);

/// Domain plugin owners, including retained but inactive Caretaker machinery.
pub const ALL: [&PluginManifest; 3] = [
    &crate::plugins::classifier::MANIFEST,
    &crate::plugins::character::MANIFEST,
    &crate::plugins::caretaker::MANIFEST,
];

/// Current deployable skills. Caretaker remains unavailable through stage configuration.
pub fn skills() -> impl Iterator<Item = &'static SkillManifest> {
    crate::plugins::classifier::MANIFEST
        .skills
        .iter()
        .chain(crate::plugins::character::MANIFEST.skills.iter())
        .copied()
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
