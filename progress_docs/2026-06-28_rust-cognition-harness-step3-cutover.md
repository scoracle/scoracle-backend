# Rust Cognition Harness — Step 3 full cutover

**Date:** 2026-06-28
**Status:** LIVE — Go LLM derivation is disabled; Rust owns the `pipeline_work` LLM stages.

## What changed

- `scoracle-cognition.service` now runs:
  `COGNITION_STAGES=scrub,transfers,narratives,vibe,sigil`.
- `.env.local` live switches were set on archbox:
  - `DERIVE_WORKER_ENABLED=false`
  - `NEWS_SCRUB_VIA_QUEUE=true`
  - `COGNITION_STAGES=scrub,transfers,narratives,vibe,sigil`
- The API was restarted and logged `Real-time derive worker disabled`.
- The Rust worker was redeployed and restarted from `rust/bin/scoracle-cognition`.
- Cron was updated:
  - `cron-pipeline.sh -mode ingest` keeps the Go RSS funnel only.
  - `cron-rust-statcommentary.sh -mode nightly -limit 400` replaces Go `statcommentary`.
  - `cron-vibesynth.sh -mode nightly` remains because nightly/reconcile only enqueues durable
    `sigil` work; it does not call the model.

## Code added

- `rust/src/bin/statcommentary.rs` — Rust rating batch (`single`, `nightly`, `backfill`) over the
  L12 rating core; rating remains a batch, not a queue stage.
- `scripts/hosting/cron-rust-statcommentary.sh` — cron wrapper for the Rust rating batch.
- `go/cmd/pipeline -mode ingest` — RSS sweep only, no Go LLM derive chain.

## Verification

- `cargo build --manifest-path rust/Cargo.toml`
- `cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path rust/Cargo.toml --lib`
- `./rust/target/debug/statcommentary -mode single -entity-type player -entity-id 56677822 -sport NBA -skip-unchanged`
  hash-skipped unchanged Wembanyama.
- `./rust/target/debug/statcommentary -mode single -entity-type team -entity-id 20 -sport NBA`
  generated the first live-capable team-rating prose path in Rust (dry-run).
- `./go/bin/pipeline -mode ingest -sport NBA -rss-limit 1 -rss-pause-ms 0`
  ran RSS ingest only and did not invoke the Go derive stages.
- Manual smoke enqueue of `pipeline_work(vibe, team/20, NBA)` completed under Rust and produced a
  fresh `vibe_scores` row at `2026-06-28 17:57:43-04` (`sentiment=35`, `prompt_version=v7`,
  `model_version=mistral:7b`).
- Host process table after cutover showed only:
  - `go/bin/scoracle-api`
  - `rust/bin/scoracle-cognition`

## Current observations

The NBA ingest smoke created real fresh work. Rust began draining it through the chain:
scrub/narratives cleared, vibe was actively draining, and downstream sigil work was pending. One
transfer row and one sigil row had failed and will retry via normal backoff.

## Rollback

1. Set `DERIVE_WORKER_ENABLED=true` in `.env.local`.
2. `systemctl --user restart scoracle-api.service`
3. `systemctl --user stop scoracle-cognition.service`
4. Restore the previous crontab from `/home/sheneveld/.cache/crontab/crontab.bak` if the Go batch
   crons must resume.

## Carry

- `099_team_rosters.sql` remains untracked and untouched.
- F-046 remains open; coordinate before any history rewrite.
- `scripts/hosting/release.sh` still needs a Step-3 cleanup so the standard release flow deploys the
  Rust worker and Rust `statcommentary` binary deliberately instead of only the Go binaries.
