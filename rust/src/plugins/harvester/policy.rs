//! Harvester-owned admission rules. Laya supplies probabilities for the
//! predicates defined by this plugin; its winning choice is audit data.

pub const HEADLINE_POLICY: &str = "headline-read-p025-v1";
/// Shadow-only until headline-read usefulness is calibrated with reviewed cases.
pub const HEADLINE_READ_THRESHOLD: f64 = 0.25;

pub const CHARACTER_ROUTE_POLICY: &str = "theme-route-p050-v1";
/// Explicitly version the existing 0.5 theme boundary inside the plugin.
pub const CHARACTER_ROUTE_THRESHOLD: f64 = 0.50;

pub fn admits_headline(relevance_probability: f64) -> bool {
    relevance_probability >= HEADLINE_READ_THRESHOLD
}

pub fn routes_theme(relevance_probability: f64) -> bool {
    relevance_probability >= CHARACTER_ROUTE_THRESHOLD
}
