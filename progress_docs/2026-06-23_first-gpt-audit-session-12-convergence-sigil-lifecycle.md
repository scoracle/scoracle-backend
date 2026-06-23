# Session 12 — Repair convergence and the event-driven Sigil lifecycle

**Date:** 2026-06-23 · **Machine:** archbox (prod DB / Ollama / cron / systemd)
**Plan:** `planning_docs/FIRST-GPT-AUDIT.md` Session 12
**Baseline:** `origin/main @ db2d094` (synced before editing)
**Code commit:** `331f76706f68` (DEPLOYED LIVE) · **Type:** code-only — **NO migration** (next free stays **106**; the parallel session took 105 — F-031)
**Parallel session:** the Sonnet Rust-scrubber / vibe-parity session shares the tree; its uncommitted `rust/*` + `.gitignore` edits and untracked `rust/src/{lib,vibe}.rs`, `rust/src/bin/`, `go/internal/ml/vibe_parity_test.go`, `sql/migrations/{099,105}*.sql` were left untouched (stashed only for the clean build stamp, then restored).

## Goal

Make Sigil availability depend on the current **Rating + Vibe + Momentum** inputs — not followers,
push config, or a nightly schedule. Convergence is event-driven and debounced by an input hash;
the nightly run becomes bounded reconciliation/backfill only; and the whole product is season-correct.

## Product decision (confirmed with Scott)

**Sigil season semantics = HISTORICAL SUPPORTED** (F-026). `/sigil` and `/leaderboard/sigil` take an
optional `?season=N`:
- **no param ⇒ the live view** = the current season **plus** legacy NULL-season rows (the pre-S12
  event-driven default), so synthesizing/backfilling an *older* season can never become the current
  crown.
- **`?season=N` ⇒ that season exactly** (no NULL, no 72h freshness window) — a season snapshot.

Both reads now emit a `season` field. The cross-season bug this kills, live: NBA `player/4`'s
most-recently-*generated* row was a `season=2024` row (score 35); the old "latest `generated_at`" read
served it as the live crown. Post-fix the live view returns the latest **2025** row (score 68) and
`?season=2024` returns the 2024 crown (35).

## Decisions

- **Real-time convergence is inherently current-season; pipeline_work stays season-agnostic.** No
  season column was added to `pipeline_work` (the queue PK is `(stage,entity_type,entity_id,sport)`).
  Instead `SigilGenerator.resolveSeason` stamps `sports.current_season` when `SigilRequest.Season` is
  nil, so every real-time/manual generation targets the current season and only an explicit-season
  **backfill** writes historical rows. The pillar loaders then load season-exact.
- **One convergence input hash already exists** (narrative titles + rating divined_peak/notability +
  momentum latest_sentiment/composite/vibe_prompt). S12 makes the rating + composite-momentum pillars
  season-exact, so "trigger when any of the three changes, debounce by the hash" holds without a
  separate Momentum generation (F-011 stays partial — Momentum is still read-derived; the news/vibe
  half is not season-scoped, F-029).
- **Kept the lenient pillar gate.** A scored Sigil is produced when ≥1 pillar is present; a marker only
  when ALL three are empty (unchanged). Requiring all three (esp. news) would gut coverage and
  contradict the system prompt ("one weak signal does not override the others"). The marker path is the
  truthful no-data state.
- **Kept the 72h freshness window** on the live per-entity `/sigil` (F-027, deferred): it's a residual
  timing-assumption that markers now make redundant, but removing it during a live deploy while legacy
  NULL-season rows still exist was deemed riskier than the documented follow-up.
- **No migration.** `sigil_synthesis.season` already exists; serving + generation are season-scoped in
  code. Avoids the F-031 collision (parallel session took 105) and keeps S12 a low-risk `release.sh`.

## Accomplishments

### `go/internal/ml/sigil.go` — convergence engine
- `SigilRequest.DryRun` added; **both** `persist` calls guarded by `!req.DryRun` (real dry-run).
- `Generate` resolves + stamps the season (`resolveSeason`: nil ⇒ current_season) and loads the rating
  + momentum pillars season-exact.
- Canonical latest-generation rule + season scope (F-023) on `loadRatingPillar` (drop `body IS NOT NULL`
  pre-filter; a no-stats marker → pillar suppressed, no fallback to an older body), `lastSynthesisHash`
  (marker's NULL hash → "" ⇒ never wrongly skip), `lastScore` (marker ⇒ baseline 0).
- `RecentlySynthesized` deleted (its only caller, the inline composite_shift path, is gone).

### `go/internal/ml/rating.go` — F-023 (stats rail)
- `lastCommentaryHash` + `ReStampPeakKeys` drop the `body IS NOT NULL` pre-filter → latest generation
  regardless of nullability (marker ⇒ "" / no-op), keeping season scope.

### `go/internal/listener/listener.go` — F-017 + simplification A
- `handlePercentileChange` ENQUEUES `pipeline_work(sigil)` on a ≥10 composite delta (input_version
  `composite:<season>:<pctile>`), **before** the follower early-return — zero-follower entities still
  converge. No inline Gemma off the transient NOTIFY. `Start`/`listenLoop`/`handlePercentileChange` no
  longer take `*ml.SigilGenerator` (`cmd/api/main.go` updated; the API still builds one for the drainer).

### `go/internal/db/db.go` + `handler/data.go` — season-scoped reads
- `entity_vibes` (`/sigil`) + `sigil_leaderboard` gain `$4 season`, resolve `sports.current_season`, and
  apply the live-view vs. exact-season rule above while preserving the S11 canonical marker rule. Both
  emit `season`. Handlers parse `?season` (+ Swagger). `dataCacheKey` already keys on the query string,
  so `?season=` does not collide in cache.

### `go/cmd/vibesynth/main.go` + cron — nightly → reconciliation
- `single`: `DryRun = !persist` (was: warned but still persisted).
- `nightly`/`reconcile`: enumerate current-season rated entities whose Sigil is missing/stale (an input
  generation newer than the Sigil) and **enqueue** `pipeline_work(sigil)` — no inline synth, **no Ollama**.
- `backfill`: per-season, only-missing (`enumRatedMissing`); direct-generate (Ollama), stamps season.
- `cron-vibesynth.sh` + `crontab.example` rewritten for the reconciliation/backstop role; nightly line
  kept (F-002), now `-mode nightly -limit 500` (dropped `-throttle-ms`, ignored in reconcile).

### `go/internal/derive/derive.go`
- `drainSigil` comment updated: the real-time queue is inherently current-season (Generate resolves +
  stamps it); no logic change needed.

## Pre-deploy validation

- `gofmt -l` clean · `go build ./...` · `go vet ./...` · `go test ./internal/ml/... ./internal/work/...` pass.
- **All prepared statements re-validated against the LIVE schema** via a throwaway `cmd/validate-stmts`
  calling `db.New` (the exact `AfterConnect → registerPreparedStatements → Ping` boot path, no
  worker/listener/drainer) → `OK`; removed after (F-025).
- Functional SQL proofs on live data BEFORE deploy: NBA `player/4` live view → 2025 (not the newer 2024
  row); `?season=2024`/`2025` exact; NBA crown board live (304) vs `season=2024` (65) vs `2025` (278)
  distinct; reconcile/backfill enumerators run + size the backlog (F-030).

## Deploy + live verification

Committed `331f76706f68` (clean stamp — the parallel session's 5 tracked edits were `git stash`'d for
the build, then `git stash pop`'d back cleanly, no conflict). Deployed via `scripts/hosting/release.sh`
(built all 4 binaries @ `331f767`, reinstalled units, masked the `scoracle-api.path` watcher — F-016,
restarted the API). `/health/db` → `healthy; serving commit 331f76706f68`. Log: "Gemma workers enabled",
"Real-time derive worker started", "Percentile listener connected" — **no degraded / prepared-statement /
panic errors**.

- **Season-scoped reads (live):** `/nba/player/4/sigil` → `season:2025 score 68`; `?season=2024` →
  `season:2024 score 35`; `/nba/leaderboard/sigil` → season 2025 (Knicks, Giannis, Turner); `?season=2024`
  → season 2024 (Davis, Beal, SGA). Distinct per-season boards; old season never current.
- **Real DryRun (live):** `vibesynth -entity-id 4 -sport NBA` (no `-persist`) → printed Score 68, rows
  before=after=4 (**no write**).
- **Reconciliation (live):** `vibesynth -mode nightly -sport NBA -limit 3` → `candidates=77 enqueued=3`
  in 0s, DB-only; the 3 enqueued `sigil` rows are NBA current-season, `input_version` NULL (distinct from
  composite_shift's `composite:*`).
- **F-018:** the restart stranded 11 `vibe` rows leased at 17:17 by the old worker (the new worker's
  17:21:51 `transfers` lease was spared); requeued exactly those (cutoff `updated_at < 17:21:51`).
- **Event path (live):** vibe drains enqueue `sigil` via the S8 hand-off (238 pending accumulated); the
  new worker drains them with season-stamped current-season output, progressively replacing legacy
  NULL-season rows (F-028) and filling the NFL/FOOTBALL coverage gap (F-030).

## Quick reference

- **`/sigil` / `/leaderboard/sigil` season:** no param = live (current season + legacy NULL); `?season=N`
  = exact (no NULL, no 72h window). Both emit `season`.
- **Generation always stamps a season:** `SigilRequest.Season` nil ⇒ `sports.current_season`. Real-time
  queue = current season; historical = explicit-season backfill only.
- **Convergence producers:** news `vibe→sigil` (derive `drainVibe`), stats `composite_shift→sigil`
  (listener enqueue), nightly reconciliation (missing/stale) — all drain through `pipeline_work(sigil)`,
  current-season, hash-gated (`SkipUnchanged`).
- **`vibesynth` modes:** `single` (DryRun unless `-persist`) · `backfill` (per-season, only-missing,
  Ollama) · `nightly`/`reconcile` (enqueue current-season missing/stale, DB-only) · `restamp` (one-time).
- **Launch-gate (F-030):** run a larger reconcile/backfill to season-stamp current-season NFL/FOOTBALL
  before launch (they have 0 season-2025-stamped rows; legacy NULL rows still serve in the meantime).
- **Findings:** F-002/F-010/F-017/F-023 RESOLVED; F-011 clarified; **F-026** (season decision), **F-027**
  (72h window follow-up), **F-028** (NULL transition), **F-029** (historical news limitation), **F-030**
  (coverage gap / launch-gate), **F-031** (105 taken → next free 106) added.
