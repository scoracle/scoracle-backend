//! scoracle-cognition — the **Rust Cognition Harness** (the LLM-derivation / cognition layer).
//!
//! The layer that *empowers* the local models. (Renamed from `scoracle-scrubber` — the
//! original clean-the-data framing.) A durable `pipeline_work`
//! queue consumer plus an Ollama client, wired to a LISTEN/NOTIFY drain loop, with
//! per-stage derivation handlers. This library crate holds the reusable modules; the
//! long-running service binary is `src/main.rs`, the offline temp-0 parity harness is
//! `src/bin/parity.rs`, and the offline A/B model eval harness is `src/bin/eval.rs` — all
//! built on top of it.
//!
//! Phase 0 shipped the foundation (queue + Ollama clients, worker loop); Phase 1 added
//! the first handler — [`vibe`] — proven byte-for-byte against Go at temperature 0. The
//! direction is **library-first**: the capability library now exists — [`route`] (the model
//! seam) and [`harness`] (the `Harness` context + the `extract` / persist / debounce / and
//! the shaped Resolve · Embed · Normalize primitives) — and `vibe` is re-expressed as its
//! first composition (`route + extract + persist`). Canonical doc:
//! `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md`.
//!
//! The product model is two primary rails plus one convergence product:
//!
//! - Stats/PEAK rail: provider box scores become derived stats, PEAK context, and the
//!   model-written scouting report / Rating surface.
//! - News/Cognition rail: RSS articles are vetted, bucketed, summarized into narratives
//!   and transfer context, then distilled into Vibe sentiment + prompt.
//! - Momentum is the convergence of PEAK trajectory and Vibe trajectory. It is surfaced
//!   to clients and also becomes Sigil context.
//! - Sigil is the final synthesis prompt: PEAK scouting report + Vibe prompt + Momentum.
//!   It is event-driven and debounced, not a scheduled rewrite of every entity.
//!
//! Product tables are append-only. No-data marker rows are part of that model: they clear
//! stale current projections without deleting history, and they still carry the configured
//! model and prompt versions rather than `NULL` provenance.

pub mod bucket;
pub mod buildinfo;
pub mod config;
pub mod corpus;
pub mod db;
pub mod embed;
pub mod eval_tasks;
pub mod harness;
pub mod narratives;
pub mod ollama;
pub mod rating;
pub mod resolve;
pub mod route;
pub mod scrub;
pub mod sigil;
pub mod stage;
pub mod trajectory;
pub mod transfer;
pub mod util;
pub mod vibe;
pub mod work;
pub mod worker;
