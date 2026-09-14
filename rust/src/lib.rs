//! scoracle-cognition — the **Rust Cognition Harness** (the LLM-derivation / cognition layer).
//!
//! A durable `pipeline_work`
//! queue consumer plus an Ollama client, wired to a LISTEN/NOTIFY drain loop, with
//! per-stage derivation handlers. This library crate holds the reusable modules; the
//! long-running service binary is `src/main.rs`, and the offline A/B model eval harness is
//! `src/bin/eval.rs`; both are built on top of this library crate.
//!
//! The layer is **library-first**: [`runtime::route`] owns model routing, [`runtime::harness`] owns the
//! `Harness` context plus shared `extract` / persist / debounce primitives, and the stage
//! modules compose those capabilities over SQL-backed inputs. Canonical doc:
//! `scoracle-wiki/wiki/Architecture/Rust Cognition Harness.md`.
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
