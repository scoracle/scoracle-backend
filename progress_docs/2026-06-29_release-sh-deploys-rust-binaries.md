# Rust Cognition Harness — Step 3 carry #3: `release.sh` deploys the Rust binaries

**Date:** 2026-06-29
**Plan:** vault `Plan - Rust Cognition Harness build.md` → "The Cutover Plan" Step 3 carry list, item 3
**Status:** DONE — the standard release flow now builds + places + restarts the Rust cognition
binaries alongside the Go binaries, with the same atomic-staging + path-watcher-masking discipline.
The carry item ("`release.sh` still needs a Step-3 cleanup so the standard release flow deploys the
Rust worker and Rust `statcommentary` binary deliberately instead of only the Go binaries") is closed.

## Context

The Step-3 cutover (2026-06-28, progress_docs/2026-06-28_rust-cognition-harness-step3-cutover.md)
flipped every LLM stage to Rust in production — but `release.sh` still built only the 4 Go binaries
(api / pipeline / statcommentary / vibesynth). Ad-hoc `cargo build` + `cp rust/target/debug/…` was
the only way to front an updated cognition daemon or rating batch onto archbox, with no atomic
placement discipline and no integrated daemon restart. Today closes that gap.

## What changed

### `scripts/hosting/release.sh` — the standard release flow is now 3 Go + 2 Rust

- **Drops the Go `statcommentary` build.** The Step-3 cutover replaced it with the Rust rating
  batch (`cron-rust-statcommentary.sh` execs `./rust/bin/statcommentary`). The retired
  `go/bin/statcommentary` is **left in place** as a rollback aid (the L13 handoff's crontab-restore
  path execs it); `release.sh` simply stops rebuilding it.
- **Adds `cargo build --bin scoracle-cognition --bin statcommentary`** to the build step. Only the
  two live Rust binaries — NOT all `Cargo.toml` bins (the offline parity / eval / resolve_experiment
  harnesses would waste a release cycle compiling).
- **Build-only invariant preserved and extended:** every binary (3 Go + 2 Rust) is built BEFORE any
  binary is moved. A failed `cargo build` aborts (`set -e`) before a single binary is placed, so the
  cron binaries + the daemon can never end up on a different commit than the API.
- **Stage-and-atomic-rename:** cargo writes to `rust/target/debug/<bin>`; release stages them into
  the same `mktemp` staging dir the Go bins use (same parent filesystem = repo root → atomic `mv`
  into `rust/bin/` and `go/bin/`).
- **Path-watcher masking extended to cognition.** `scoracle-cognition.path` watches `rust/bin/`
  (the L10 design — narrow dir, not the noisy `target/debug/`). Placing 2 binaries would otherwise
  fire the oneshot restart helper twice, flapping the daemon mid-drain and abandoning leased
  `pipeline_work` rows. `release.sh` now masks BOTH `scoracle-api.path` AND `scoracle-cognition.path`
  across placement (full release only), with the cleanup trap re-arming whichever it stopped. As
  with the Go side: ad-hoc `cargo build + cp rust/target/debug/X rust/bin/` still auto-restarts via
  the re-armed watcher — only the integrated release step is gated.
- **Daemon restart + sanity check on full release.** After the API restart + `/health/db` probe,
  release does an explicit `systemctl --user restart scoracle-cognition.service` and a 30 s
  `is-active` poll. The daemon has no HTTP probe; readiness is the systemd unit + the journal
  (`journalctl --user -u scoracle-cognition`). A failed cognition boot is NON-fatal for the API
  verification (the API is healthy + serving from precomputed tables), but it DOES fail the release
  script so a "API up, cognition silently down" state can't pass clean.
- **`RELEASE_BIN_DIR` now redirects BOTH tracks.** The existing `--build-only` verification pattern
  (`RELEASE_BIN_DIR=$(mktemp -d) scripts/hosting/release.sh --build-only`) previously redirected just
  the Go side; the Rust side still wrote into the live `rust/bin/`. Now setting `RELEASE_BIN_DIR`
  sends all 5 binaries (3 Go + 2 Rust) to the same scratch dir → ZERO live placement + ZERO
  path-watcher trips. Default (unset) keeps the production targets: `go/bin/` for Go, `rust/bin/`
  for Rust.
- Go buildinfo (commit + build_time) LDFLAGS stamping is unchanged; the Rust side does not stamp
  today (no equivalent plumbing in the Rust crate — HORIZON, not blocking).

### `scripts/hosting/install.sh`

Step 2b of the post-install banner updated: the rendered systemd units are still the one-time
enablement act on a fresh machine, but the binary deployment is now done by the standard release
flow (`release.sh`). A one-time pre-release bootstrapping recipe is documented for the rare case of
enabling the units before the first `release.sh` — building + `cp`-ing both Rust binaries by hand
(mirroring the L10 daemon-only convention, extended to the rating batch).

### `scripts/hosting/README.md`

Updated the Release section + the file-listing table to reflect the 5-binary build set, the
`scoracle-cognition.{service,path,-restart.service}` units, and the Rust-vs-Go cron wrappers (the
retired `cron-statcommentary.sh` is now labeled "legacy wrapper for the retired Go stats-rail batch;
rollback aid" and `cron-rust-statcommentary.sh` is listed as the live path).

### `RUNBOOK.md`

- Updated prod-host description + the system-map ASCII diagram: the Go box's "derive worker" path
  is now `DERIVE_WORKER_ENABLED=false (Step-3)`, and a Rust `scoracle-cognition.service` box is
  drawn showing the durable `pipeline_work` drain (scrub → transfers → narratives → vibe → sigil).
- Replaced the "Four deployed binaries" table with a 5-row table (3 Go + 2 Rust) and explained the
  retire/reuse semantics of the legacy `go/bin/statcommentary` (in place as rollback aid, NOT
  rebuilt by `release.sh`).
- Updated §2 (Release) wording + the `release.sh` invocation comment to "all 5 (3 Go + 2 Rust)".
- Added a **Step-3 cognition rollback** recipe to §3 covering the one-flag
  `DERIVE_WORKER_ENABLED=true` flip + `systemctl --user stop scoracle-cognition.service` + crontab
  restore path, with the legacy `go/bin/statcommentary` explicitly named as the rollback aid that
  release preserves.

## Verification

```bash
bash -n scripts/hosting/release.sh && bash -n scripts/hosting/install.sh   # syntax
scripts/hosting/release.sh --help                                          # header block prints cleanly
# Inspection dry-run (no live changes — all binaries land in a scratch dir):
RELEASE_BIN_DIR=$(mktemp -d) scripts/hosting/release.sh --build-only
#   → 3 Go binaries built + placed into the scratch dir
#   → 2 Rust binaries built + placed into the SAME scratch dir (rust/bin untouched)
#   → 0 path-watcher restarts (scratch dir is not watched)
# Production --build-only places live binaries:
scripts/hosting/release.sh --build-only
#   → Go bins into go/bin/, Rust bins into rust/bin/, no service changes
```

All paths green on this machine. (No live `release.sh` was run — that's the user's call on archbox.)

## Carry

- `099_team_rosters.sql` remains untracked and untouched (not ours).
- F-046 remains open (DB password in git history; coordinate before any force-push).
- The Rust side has no commit/build-time stamp (`BUILDINFO`-equivalent LDFLAGS is Go-only). A
  future `scoracle-cognition --version` could pin a build SHA; deferred — out of scope for the
  release-flow carry.
- The legacy `cron-statcommentary.sh` wrapper + the retired `go/bin/statcommentary` binary are left
  in the tree as the documented Step-3 cognition rollback aid. Deleting them is a deliberate act the
  user can take once Step 3 is fully bedded in; `release.sh` no longer rebuilds the Go binary.

## File layout delta

```
scripts/hosting/release.sh                Reworked: 3 Go + 2 Rust build/placement/restart (path-masking both sides)
scripts/hosting/install.sh               Step 2b banner reworded (release.sh deploys both binaries; one-time bootstrap recipe documented)
scripts/hosting/README.md                Release section + table updated (5 binaries, cognition units, Rust+Go cron wrappers)
RUNBOOK.md                               Prod description + system map + 1 system-map table + §2 Release + §3 Step-3 cognition rollback recipe
```