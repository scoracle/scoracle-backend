//! Scoracle Studio (the in-house harness) and its production plugins.
//!
//! [`harness`] registers plugins, coordinates durable work, and supplies inference transports.
//! [`plugins`] own preparation, validation and publication policy.
//! [`tools`] supply shared source acquisition, identity and form mechanics.
//! [`evaluation`] uses the same production contracts.
//!
//! The fleet has **ten plugins, six of them accountable characters the seeker meets**:
//!
//! - The Scout (`rating`) and The Analyst (`momentum`) read the stats material.
//! - The Journalist (`narratives`), The Insider (`transfers`) and The Influencer (`vibe`) read
//!   the news material.
//! - The Oracle (`sigil`) reads the other five cards and renders the verdict.
//! - Four internal capabilities: Harvester, Investigator, Graph, and deterministic Boxscore retrieval.
//!
//! Those groupings describe which material a seat reads. Routing uses plugin-owned route keys.
//!
//! Momentum is now a queue stage: deterministic `momentum_scores` stays the numeric backbone, while
//! `momentum_summaries` stores the generated direction/blurb product consumed by Sigil. The eval
//! harness still exposes fixture-first `momentum` cases so analytical model candidates can be
//! measured before any dedicated route split.
//!
//! Product tables are append-only. No-data marker rows are part of that model: they clear
//! stale current projections without deleting history, and they still carry the configured
//! model and prompt versions rather than `NULL` provenance.

/// Offline evaluation and editorial review tools.
pub mod evaluation;
/// Plugin registration, durable execution, IO and model infrastructure.
pub mod harness;

/// First-party cognition, tools, and domain adapters.
pub mod plugins;

/// Shared form, prose integrity and publisher-source tools.
pub mod tools;

/// Pure text, rounding, and fingerprint helpers.
pub mod util;
