# Rust Cognition Harness — Step 3 rollback aid removal (C5)

**Date:** 2026-06-29
**Plan:** vault `Plan - Rust Cognition Harness build.md` → post-Step-3 cleanup item C5
**Status:** DONE — the Step-3 rollback aid is removed now that the cutover has bedded in.

## Context

The Step-3 cutover (2026-06-28) moved every LLM stage + the rating batch to Rust and
retired Go's `cmd/statcommentary`. To hedge a fresh regression, the Go binary and its
cron wrapper were deliberately left in place as a one-flag rollback path (Go derive worker
re-armed + crontab backup restored). With Step-3 stable across the bed-in window, that
aid is no longer needed.

## What changed

### Deleted

- `scripts/hosting/cron-statcommentary.sh` — the retired Go stats-rail batch cron wrapper.
- `go/bin/statcommentary` — the retired Go rating binary (already not rebuilt by `release.sh`).

### Updated docs

- `scripts/hosting/README.md` — removed the `cron-statcommentary.sh` row from the file table;
  `cron-rust-statcommentary.sh` remains the live path.
- `scripts/hosting/crontab.example` — no direct legacy row existed (already pointed at the Rust
  wrapper), so no change needed.
- `RUNBOOK.md` — removed the "retired `go/bin/statcommentary` is the rollback aid" note from the
  five-binaries section, and removed the entire §3 "Step-3 cognition rollback" recipe. The
  generic commit-based rollback recipe stays.
- `rust/README.md` — rephrased the one-flag rollback note to clarify that Go queue draining can
  still be re-armed, but the Go stats-rail batch is no longer present.
- `scripts/hosting/release.sh` — header comment now says the Go `statcommentary` binary is no
  longer present, instead of describing it as a kept rollback aid.
- `scripts/hosting/install.sh` — comment listing active cron wrappers now names
  `cron-rust-statcommentary` instead of the deleted `cron-statcommentary`.

## Verification

```bash
bash -n scripts/hosting/release.sh && bash -n scripts/hosting/install.sh   # syntax
# Rust gate:
cd rust
cargo build --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --lib   # 78 passed, 0 failed, 1 ignored
```

All green. The pre-existing non-mine WIP in the working tree (`go/cmd/api/main.go`,
`go/internal/api/server.go`, `server_test.go`, `cloudflared-config.example.yml`,
`go/internal/api/opencodeproxy/`, `sql/migrations/099_team_rosters.sql`) was left untouched.

## Carry

- `099_team_rosters.sql` remains untracked and untouched (not ours).
- F-046 remains open (DB password in git history; coordinate before any force-push).
- B3 — widen `work::Item.entity_id` i32 → i64 — still deferred (multi-file touch, lower-value;
  article ids fit comfortably in i32 today).

## File layout delta

```
scripts/hosting/cron-statcommentary.sh    DELETED (C5)
go/bin/statcommentary                      DELETED (C5, gitignored binary)
RUNBOOK.md                                 Updated: removed Step-3 rollback recipe + rollback-aid note
rust/README.md                             Updated: one-flag rollback note rephrased
scripts/hosting/README.md                  Updated: removed legacy cron row
scripts/hosting/release.sh                 Updated: header comment
scripts/hosting/install.sh                 Updated: comment
progress_docs/2026-06-29_c5-remove-step3-rollback-aid.md   NEW (this doc)
```
