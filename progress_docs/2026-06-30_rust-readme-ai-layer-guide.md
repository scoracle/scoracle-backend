# 2026-06-30 - Rust README AI layer guide

## Goal

Make `rust/README.md` a focused entry point for work on the Rust Cognition Harness, since this folder is the production AI derivation layer.

## What Changed

- Rewrote `rust/README.md` around layer role, start docs, product/data boundaries, stage map, core primitives, change workflow, verification, operations, progress docs, and handoff format.
- Kept the existing technical substance: queue stages, rating batch distinction, Harness/Route/Extract/Persist/Resolve primitives, production binaries, environment variables, offline harnesses, and known limits.
- Reframed the folder as the AI/cognition layer that writes precomputed products rather than an isolated Rust implementation note.

## Files Changed

- `rust/README.md`
- `README.md`
- `progress_docs/2026-06-30_rust-readme-ai-layer-guide.md`

## Verification

- Confirmed backend branch was synced with `origin/main` before editing.
- Cross-checked current Rust source layout, `Cargo.toml`, systemd unit, release script, backend README, development docs, product narrative, and data flow before rewriting.
- Documentation-only change; no Rust build or tests required.

## Result

Future AI-layer sessions can start in `rust/README.md` and quickly load the right product, data-flow, stage, verification, operations, and handoff context without reading the whole backend repo.
