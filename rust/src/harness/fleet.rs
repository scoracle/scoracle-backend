//! Composition roster for the first-party plugin fleet.

use crate::harness::plugin::PluginManifest;
use crate::harness::route::RouteKey;
pub use crate::plugins::analyst::manifest::MANIFEST as ANALYST;
pub use crate::plugins::classifier::manifest::{
    ACQUIRE_MANIFEST as CLASSIFIER_ACQUIRE, MANIFEST as CLASSIFIER,
};
pub use crate::plugins::fixture_boxscore::manifest::MANIFEST as FIXTURE_BOXSCORE;
pub use crate::plugins::influencer::manifest::MANIFEST as INFLUENCER;
pub use crate::plugins::insider::manifest::MANIFEST as INSIDER;
pub use crate::plugins::investigator::manifest::MANIFEST as INVESTIGATOR;
pub use crate::plugins::journalist::manifest::MANIFEST as JOURNALIST;
pub use crate::plugins::oracle::manifest::MANIFEST as ORACLE;
pub use crate::plugins::scout::manifest::MANIFEST as SCOUT;

/// Concurrent Kev article requests. Eight native slots share each request's question prefix.
pub const ARCHBOX_SLOTS: (&str, usize) = ("archbox-kev", 4);
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
    &INVESTIGATOR,
    &FIXTURE_BOXSCORE,
    &CLASSIFIER_ACQUIRE,
    &CLASSIFIER,
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
