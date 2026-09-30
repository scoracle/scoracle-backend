//! Harvester prepares predicates and governs admission; System 1 only scores.
use crate::plugins::{influencer, insider, journalist, scout};
use crate::studio::plugin::PluginManifest;

pub struct Predicate {
    pub key: &'static str,
    /// {target} is the plugin-supplied identity, never model-selected.
    pub statement: &'static str,
    pub threshold: f64,
}

pub struct CharacterRoute {
    pub predicates: &'static [Predicate],
    pub destination: &'static PluginManifest,
    pub delivery_name: &'static str,
}

pub const CHARACTER_ROUTES: &[CharacterRoute] = &[
    CharacterRoute {
        predicates: &[Predicate { key: "narrative", statement: "The source reports a specific new event or match result involving {target}.", threshold: 0.5 }],
        destination: &journalist::manifest::MANIFEST, delivery_name: "journalist",
    },
    CharacterRoute {
        predicates: &[Predicate { key: "emotional_charge", statement: "This text describes someone's actual feelings or emotional reaction about {target}.", threshold: 0.5 }],
        destination: &influencer::manifest::MANIFEST, delivery_name: "influencer",
    },
    CharacterRoute {
        predicates: &[
            Predicate { key: "player_move", statement: "This text reports transfer news or transfer rumours about {target}.", threshold: 0.5 },
            Predicate { key: "contract", statement: "This text reports a contract decision involving {target}.", threshold: 0.5 },
            Predicate { key: "staffing", statement: "This text reports a coaching or staff change at {target}.", threshold: 0.5 },
        ],
        destination: &insider::manifest::MANIFEST, delivery_name: "insider",
    },
    CharacterRoute {
        predicates: &[
            Predicate { key: "performance", statement: "This text reports a game or match result or player performance statistics for {target}.", threshold: 0.7 },
            Predicate { key: "fitness", statement: "This text reports an injury, suspension or player availability update for {target}.", threshold: 0.5 },
        ],
        destination: &scout::manifest::MANIFEST, delivery_name: "scout",
    },
];

/// Provisional, evaluated on development fixtures; not a calibrated probability.
pub const HEADLINE_POLICY: &str = "explicit-headline-read-p025-v2";
pub const HEADLINE_READ_THRESHOLD: f64 = 0.25;
pub const CHARACTER_ROUTE_POLICY: &str = "source-window-predicates-v2";
pub const MAX_THEME_WINDOWS: usize = 8;

pub fn admits_headline(probability: f64) -> bool {
    probability >= HEADLINE_READ_THRESHOLD
}
