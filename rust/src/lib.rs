//! scoracle-cognition — the **Rust Cognition Harness** (the LLM-derivation / cognition layer).
//!
//! The layer that *empowers* the local models. (Renamed from `scoracle-scrubber` — the
//! original clean-the-data framing.) A durable `pipeline_work`
//! queue consumer plus an Ollama client, wired to a LISTEN/NOTIFY drain loop, with
//! per-stage derivation handlers. This library crate holds the reusable modules; the
//! long-running service binary is `src/main.rs`, and the offline A/B model eval harness is
//! `src/bin/eval.rs`; both are built on top of this library crate.
//!
//! The layer is **library-first**: [`route`] owns model routing, [`harness`] owns the
//! `Harness` context plus shared `extract` / persist / debounce primitives, and the stage
//! modules compose those capabilities over SQL-backed inputs. Canonical doc:
//! `scoracle-wiki/wiki/Architecture/Rust Cognition Harness.md`.
//!
//! The product model is three rails with six accountable lenses:
//!
//! - Stats/analytical rail: Rating/PEAK plus Momentum trajectory.
//! - Emotional/news rail: Narratives, Transfers, and Vibe.
//! - Synthesis rail: Sigil — The Oracle reads the other five cards and renders the verdict.
//!
//! Momentum is now a queue stage: deterministic `momentum_scores` stays the numeric backbone, while
//! `momentum_summaries` stores the generated direction/blurb product consumed by Sigil. The eval
//! harness still exposes fixture-first `momentum` cases so analytical model candidates can be
//! measured before any dedicated route split.
//!
//! Product tables are append-only. No-data marker rows are part of that model: they clear
//! stale current projections without deleting history, and they still carry the configured
//! model and prompt versions rather than `NULL` provenance.

/// The seven model-calling seats, one directory each. See [`junctions`] for the roster.
pub mod junctions;

// Infrastructure — the machinery every junction composes over.
pub mod config;
pub mod db;
pub mod embed;
pub mod harness;
pub mod ledger;
pub mod ollama;
pub mod route;
pub mod stage;
pub mod work;
pub mod worker;

// Primitives — shared, junction-agnostic building blocks.
pub mod bucket;
pub mod buildinfo;
pub mod corpus;
pub mod novelty;
pub mod threads;
pub mod trajectory;
pub mod util;

// Non-junction stages and offline tooling.
pub mod boxscore_fetch;
pub mod eval_tasks;
pub mod judge;
pub mod scrub;
