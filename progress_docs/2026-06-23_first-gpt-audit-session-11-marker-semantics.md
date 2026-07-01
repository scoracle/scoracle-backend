# Session 11 — Standardize append-only marker semantics

**Date:** 2026-06-23 · **Machine:** archbox (prod DB / Ollama / cron / systemd)
**Plan:** `planning_docs/FIRST-GPT-AUDIT.md` Session 11
**Baseline:** `origin/main @ ad6f196` (synced before editing; parallel Sonnet session shares the tree —
`sql/migrations/099_team_rosters.sql` left untracked + unapplied as instructed)
**Type:** read-path + narrator only — **NO migration** (next free migration number stays **105**)

## Goal

One canonical "latest generation" rule across every product read, so a newer marker row (no-data) clears
stale current content — and fix the narrator so an empty local model result becomes a successful marker instead
of a hard failure (F-019).

## The bug pattern (killed)

Several reads filtered out null/marker rows **before** selecting the latest generation:

```sql
-- WRONG: a newer no-data marker can never become "the latest", so old content lingers
WHERE body IS NOT NULL
  AND generated_at = (SELECT max(generated_at) FROM t WHERE ... AND body IS NOT NULL)
```

The canonical rule (already used correctly by `ml/vibe.go loadLatestNarratives` and
`ml/sigil.go loadNarrativePillar`):

```sql
-- RIGHT: find the latest generation regardless of nullability, THEN gate on content.
-- If the latest generation is a marker, the content gate yields 0 rows → empty/null.
WHERE body IS NOT NULL
  AND generated_at = (SELECT max(generated_at) FROM t WHERE ...)   -- inner max UNFILTERED
```

Markers change only the **current projection** — prior generations are preserved (append-only); history
sparklines keep only real points.

## Decisions

- **Read-path-only, no schema change.** The failure-vs-no-data distinction is already encoded in control
  flow (a real failure returns an `error` → queue retry/dead-letter; a no-data outcome writes a NULL-body
  marker row → Completes). An explicit `marker_reason` column (`no_corpus`/`no_stats`/`no_pillars`) was
  the audit's *optional* suggestion — deferred (**F-024**) to keep S11 a pure `release.sh` (no migration,
  lower risk with the parallel session + F-015 schema-drift). Per-product reason is implicit in which
  table the marker lives in.
- **Scope = serving reads only.** Generation-side pillar/debounce loaders (`loadRatingPillar`,
  `lastSynthesisHash`, `lastScore`, `lastCommentaryHash`, `ReStampPeakKeys`) were left for **Session 12**
  (the Sigil/convergence lifecycle) and recorded as **F-023** — they still use latest-non-marker, which is
  inconsistent but low-impact today (`stat_summaries` has 0 markers live).
- **F-019 fix = parse *semantics*, not a new branch.** `parseNarratives`'s bool now means "was the
  response a parseable narratives document," independent of count. The existing
  `len(narratives) == 0 → marker` path (already present for the no-grounded case) does the rest, so an
  empty array flows to a marker with no new persistence code.

## Accomplishments

### `go/internal/db/db.go` — canonical rule applied to 6 reads
| Statement | Endpoint | Fix |
|---|---|---|
| `entity_news` | `/news` (per entity) | inner `max(generated_at)` de-filtered (was `AND body IS NOT NULL`) |
| `entity_vibes` | `/sigil` (per entity) | `vibe_cur` selects the latest synthesis (unfiltered max), returns it only if `score IS NOT NULL` and <72h; `vibe_hist` unchanged |
| `/rating` commentary | `/rating` | latest `stat_summaries` gen **within the season scope** (unfiltered max), then `body IS NOT NULL` gate |
| `narratives_leaderboard` | news board | new `latest_gen` CTE resolves each entity's latest gen first; `latest` keeps only that gen's content (+7d window on the gen) → a newer marker drops the entity off the board |
| `sigil_leaderboard` | crown board | `latest_raw` (DISTINCT ON, unfiltered) → `latest` filters `score/blurb IS NOT NULL` |
| `vibes_leaderboard` | vibes board | `latest_raw` (DISTINCT ON, 48h window, unfiltered) → `latest` filters `sentiment IS NOT NULL` |

`loadNarrativePillar` (sigil generation input) already used the correct pattern → untouched.

### `go/internal/ml/news_narratives.go` — F-019
- `parseNarratives`: a cleanly-closed array (incl. empty `{"narratives": []}`) → `ok=true`; EOF before the
  array closed with nothing salvaged → `ok=false` (retry). Doc comment + call-site comment rewritten to
  state `generation_failed` must never masquerade as no-data.
- `go/internal/ml/news_narratives_test.go` (new) — `TestParseNarrativesEmptyArrayIsNoData` locks the 8
  cases (empty/whitespace/one/two/truncated-salvage/no-key/truncated-empty/empty-response).

## Pre-deploy validation

- `gofmt` + `go vet` clean; `go test ./internal/ml/... ./internal/work/...` pass.
- **All prepared statements re-validated against the LIVE schema** via a throwaway `cmd/validate-stmts`
  that calls `db.New` (AfterConnect → `registerPreparedStatements` → `Ping`, the exact boot path) WITHOUT
  starting any worker/listener/drainer → `OK`. Removed after. (Guards against a degraded boot from a
  SQL/column error in the edits — F-015.)
- Functional proof on a live stale entity (`player/37296248` FOOTBALL, latest gen is a marker):
  OLD rule served **1** stale narrative; NEW rule returns **0**. 36 such news entities were mis-serving
  stale narratives pre-fix.

## Deploy + live verification

Code committed `fcff1d92197b` (clean tree → clean stamp; `099_team_rosters.sql` left untracked).
Deployed with `scripts/hosting/release.sh` → built all 4 binaries @ `fcff1d9`, reinstalled units,
restarted the API, `/health/db` healthy ("serving commit fcff1d92197b"). The API log shows the clean
handoff: old worker (PID 2144376) cancelled mid-drain → new worker (PID 2203928) "Real-time derive worker
started" + "connected", **no prepared-statement / degraded / panic errors**.

- **F-018 mitigation:** the restart stranded the old worker's leased batch (10 `narratives` @ 06:58, 10
  `transfers` @ 06:59 — `running`, <30m so not auto-recovered). Requeued exactly those (cutoff
  `updated_at < 07:00`), leaving the new worker's fresh 07:07 transfers lease untouched. Then requeued all
  8 failed `{"narratives": []}` rows (`attempts=0, available_at=NOW()`).
- **Canonical read rule (live):**
  - `entity_news` — stale `player/37296248` FOOTBALL (latest gen is a marker) `/news`: **1 → 0** narratives.
  - `narratives_leaderboard` (NFL) old-vs-new SQL: **372 → 362** entities (10 stale-marker entities dropped,
    **0** added) — markers now clear the board.
  - All six edited reads serve **HTTP 200 + valid JSON** post-restart (`/leaderboard/{news,sigil,vibes}`
    across NFL/NBA/FOOTBALL; per-entity `/sigil`, `/rating`, `/news`).
- **F-019 (live, full loop):**
  - Unit: `TestParseNarrativesEmptyArrayIsNoData` (8 cases) green.
  - `newsnarrate` dry-run on `player/86` NFL (Jameis Winston, a prior dead-letter): returns
    `(no usable narratives — null marker)`, **exit 0** — was the hard `parse narratives failed` error.
  - Queue path (throwaway `drainnarr`, exact `work.Claim → Generate → Complete` logic, removed after):
    3/3 requeued items **COMPLETED** (Napoli 4, Tez Johnson 1, Juventus 4 narratives) — none re-failed.
  - Post-verify queue: **0** failed narratives, **0** empty-array dead-letters; stages draining normally
    (the in-API worker is on the transfers backlog; narratives + the requeued rows drain behind it — the
    Complete-not-fail behavior is proven, so they cannot re-dead-letter on the empty-array path).

## Quick reference

- **Canonical rule:** latest generation = `generated_at = (SELECT max(generated_at) ... )` with the inner
  max **unfiltered**; gate on content AFTER. Never filter nulls before picking the latest.
- **Marker per table:** `news_summaries.body IS NULL` · `stat_summaries.body IS NULL` ·
  `sigil_synthesis.score IS NULL` · `vibe_scores.sentiment IS NULL`.
- **F-015 reminder:** `entity_vibes` (the `/sigil` per-entity read) physically reads `sigil_synthesis`;
  `vibes_leaderboard` reads `vibe_scores`. Verified against the live schema, not the migration ledger.
- **Requeue a dead-lettered/failed work row:** `work.Enqueue` reopens a `failed` row, or
  `UPDATE pipeline_work SET status='pending', attempts=0, available_at=NOW() WHERE ...`.
