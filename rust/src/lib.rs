//! Scoracle Studio (the in-house harness) and its production application adapters.
//!
//! [`studio`] creates validated products from prepared material through an injected model.
//! [`runtime`] owns routing, transports, queue scheduling, and the transitional database
//! context. [`composition`] prepares sourced memories and holds other character briefs;
//! [`evidence`] retrieves shared material. The Analyst's creation has moved from
//! [`junctions`] to Studio. Existing evaluation tooling remains in [`evaluation`].
//!
//! The product model is **nine lenses, six of them accountable characters the seeker meets**:
//!
//! - The Scout (`rating`) and The Analyst (`momentum`) read the stats material.
//! - The Journalist (`narratives`), The Insider (`transfers`) and The Influencer (`vibe`) read
//!   the news material.
//! - The Oracle (`sigil`) reads the other five cards and renders the verdict.
//! - Three internal seats never surface as a character: The Editor, The Investigator, and graph.
//!
//! Those groupings describe which material a seat reads. Routing is independently per `Role`.
//!
//! Momentum is now a queue stage: deterministic `momentum_scores` stays the numeric backbone, while
//! `momentum_summaries` stores the generated direction/blurb product consumed by Sigil. The eval
//! harness still exposes fixture-first `momentum` cases so analytical model candidates can be
//! measured before any dedicated route split.
//!
//! Product tables are append-only. No-data marker rows are part of that model: they clear
//! stale current projections without deleting history, and they still carry the configured
//! model and prompt versions rather than `NULL` provenance.

/// The nine model-calling seats, one directory each. See [`junctions`] for the roster.
pub mod junctions;

/// Voice, form and entity memories composed into model inputs.
pub mod composition;

/// Offline evaluation and editorial review tools.
pub mod evaluation;
/// Shared evidence sources and deterministic story primitives.
pub mod evidence;
/// Execution, IO, configuration and provider infrastructure.
pub mod runtime;

/// Studio is the in-house harness; runtime owns its application adapters.
pub mod studio;
