//! scoracle-scrubber — the Rust scrubbing (LLM-derivation) layer.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop, with per-stage derivation handlers. This library crate
//! holds the reusable modules; the long-running service binary is `src/main.rs` and
//! the offline parity harness is `src/bin/parity.rs`, both built on top of it.
//!
//! Phase 0 shipped the foundation (queue + Ollama clients, worker loop) with ZERO
//! handlers. Phase 1 adds the first stage handler — [`vibe`] — and proves it matches
//! the Go vibe stage byte-for-byte at temperature 0. See `rust/README.md` and the vault
//! plan `scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md`.

pub mod config;
pub mod db;
pub mod ollama;
pub mod stage;
pub mod util;
pub mod vibe;
pub mod work;
pub mod worker;
