//! scoracle-cognition — the **Rust Cognition Harness** (the LLM-derivation / cognition layer).
//!
//! The layer that *empowers* the local models. (Renamed from `scoracle-scrubber` — the
//! original clean-the-data framing.) A durable `pipeline_work`
//! queue consumer plus an Ollama client, wired to a LISTEN/NOTIFY drain loop, with
//! per-stage derivation handlers. This library crate holds the reusable modules; the
//! long-running service binary is `src/main.rs` and the offline parity harness is
//! `src/bin/parity.rs`, both built on top of it.
//!
//! Phase 0 shipped the foundation (queue + Ollama clients, worker loop); Phase 1 added
//! the first handler — [`vibe`] — proven byte-for-byte against Go at temperature 0. The
//! direction now is **library-first**: build the capability library (route · resolve ·
//! extract+validate · embed+cluster · normalize · persist) and re-express `vibe` as its
//! first composition. Canonical doc:
//! `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md`.

pub mod config;
pub mod db;
pub mod ollama;
pub mod stage;
pub mod util;
pub mod vibe;
pub mod work;
pub mod worker;
