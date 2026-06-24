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

pub mod config;
pub mod db;
pub mod harness;
pub mod ollama;
pub mod route;
pub mod stage;
pub mod util;
pub mod vibe;
pub mod work;
pub mod worker;
