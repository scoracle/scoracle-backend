//! Composition roster for the first-party plugin fleet.

pub use crate::plugins::analyst::manifest::MANIFEST as ANALYST;
pub use crate::plugins::fixture_boxscore::manifest::MANIFEST as FIXTURE_BOXSCORE;
pub use crate::plugins::graph::manifest::MANIFEST as GRAPH;
pub use crate::plugins::harvester::manifest::MANIFEST as HARVESTER;
pub use crate::plugins::influencer::manifest::MANIFEST as INFLUENCER;
pub use crate::plugins::insider::manifest::MANIFEST as INSIDER;
pub use crate::plugins::investigator::manifest::MANIFEST as INVESTIGATOR;
pub use crate::plugins::journalist::manifest::MANIFEST as JOURNALIST;
pub use crate::plugins::oracle::manifest::MANIFEST as ORACLE;
pub use crate::plugins::scout::manifest::MANIFEST as SCOUT;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::PluginManifest;

/// Shared slots for models hosted on Archbox. Keep this aligned with the host's
/// `OLLAMA_NUM_PARALLEL` and configured backend concurrency.
pub const ARCHBOX_SLOTS: (&str, usize) = ("archbox-3b", 4);
/// Shared slots for models hosted on the Mac. Slot-group membership must follow routing.
pub const MAC_SLOTS: (&str, usize) = ("mac-3b", 4);

/// The deployable first-party worker fleet: the six reader-facing characters,
/// then the internal seats. Registration order in `main.rs` follows the queue's
/// dependency order instead; this list is the identity roster.
pub const ALL: [&PluginManifest; 10] = [
    &JOURNALIST,
    &INFLUENCER,
    &SCOUT,
    &INSIDER,
    &ANALYST,
    &ORACLE,
    &HARVESTER,
    &INVESTIGATOR,
    &FIXTURE_BOXSCORE,
    &GRAPH,
];

/// Configured inference routes contributed by the statically linked plugin roster.
pub fn inference_routes() -> Vec<RouteKey> {
    let mut routes = Vec::new();
    for manifest in ALL {
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
