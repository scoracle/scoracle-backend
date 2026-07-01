# 2026-06-14 — SESSION HANDOFF: News→local model pipeline (staged build)

Capstone for a long build session. Read this + memory `news-model-pipeline` +
`model-two-rail-narratives` + vault `Plan - News pipeline integration.md` to resume. Per-task detail
in the sibling `2026-06-14_*.md` progress docs.

## TL;DR
The full **scrub → transfers → narratives → vibe** pipeline is built; tasks 1–3 are LIVE in prod, tasks
4/6/9/11 are committed + verified but await ONE batched binary deploy. Migrations applied through 085.

## Shipped this session (commits, all pushed to origin/main)
- **Foundation deploy** (no new commit — applied 081/082 + rebuilt) — Twitter live-fetch suspended
  (~11s→0.4ms), `news_summaries` + `pipeline_stats` tables live.
- `1469053` task 2 — scrub **vetted-flag** semantics (083: `vetted`+`scrubbed_at`; scrub UPDATEs, never deletes).
- `7202797` task 3 — **async scrub** as a `news_scrub` maintenance ticker (30m/15; auto-vet primaries +
  local model-disambiguate secondaries). **LIVE** — coverage building (106k→~45k unscrubbed).
- `60790b2` task 4 — **flip consumers** onto `(vetted IS TRUE OR scrubbed_at IS NULL)` (news/vibe/transfer
  Go + db.go news_leaderboard + migration 084 transfer SQL). Non-regressive.
- `7babf28`+`bf6fbf8` task 6 — **narratives stage**: 085 restructures news_summaries → one-row-per-narrative;
  `ml/news_narratives.go` groups the vetted corpus into N storylines w/ per-narrative impact; tolerant parser.
  Retired the superseded single-summary `news_analysis.go`+`cmd/newsanalyze`.
- `75ae685` task 9 — **vibe as the final stage** (prompt v4): determines one overall sentiment from the
  latest narratives + transfer heat (no migration; reuses vibe_scores).
- `6db3139` task 11 — **transfers stage-2 read hygiene**: `generated_at > now-14d` + `heat > 0` on the 3
  transfer reads (stale/pre-scrub pairs age out, zero-signal stragglers drop).

## PROD STATE (precise — verify on resume)
- **Migrations applied: through 085** (081,082,083,084,085). `schema_migrations` confirms.
- **Live binary = the task-3 build** (`systemctl --user` PID from 2026-06-14 ~13:34). It runs: Twitter-off,
  the scrub ticker. It does NOT yet run: task-4 Go vetted filters, narratives generator, v4 vibe, task-11
  read hygiene — those are in the committed source, awaiting the rebuild.
- **First real narratives + a v4 vibe exist for Chelsea (team 18)** from verification runs (valid data).
- The API does NOT read `news_summaries` yet (no read endpoint) — so 085's column rename is safe vs the
  running + next binary.

## LOCKED DESIGN — staged local model pipeline (each stage enriches the next; sentiment LAST)
```
raw → SCRUB (clean links) → TRANSFERS (vet off scrubbed corpus) → NARRATIVES (grounded on transfers) → VIBE (last)
```
- **Narratives** = MULTIPLE per entity, one write-up each, one-row-per-narrative in news_summaries, with a
  **per-narrative impact** that ranks the leaderboard (hottest narratives, not entities).
- **Transfers** = stage 2 evidence layer; its structured rumors GROUND the narratives.
- **Vibe** = final stage; ONE overall sentiment determined from narratives + heat.
- **Product framing:** News card shows ALL trending narratives (incl. transfer sagas) — the rich product;
  a content-type scope drills to JUST transfers — the structured evidence (one-card All/Transfers scopes).
- Sourcing-from-scrubbed is wired (084 + task 4); the local model `is_rumor` vet already clears same-name noise.

## WHAT'S LEFT, IN ORDER
1. **Task 12 — ground narratives on vetted transfers.** In `ml/news_narratives.go`, load the entity's
   vetted transfer rumors (mirror vibe's `loadTransferHeat`) and pass them into the prompt as FACTS
   ("Rogers — incoming, advanced_talks, heat 85"), so transfer narratives are accurate + don't re-list
   what the Transfers drill-down already structures. Re-verify on Chelsea.
2. **Task 10 — orchestrate the cadence.** cron 3×/day + vibe_trigger run the stages IN ORDER per entity:
   scrub (async, live) → transfers → narratives → vibe. Tune narrative latency (~100s/entity). Populate
   pipeline_stats on the daily ticker.
3. **BATCHED BINARY DEPLOY** (takes tasks 4/6/9/11/12/10 live): see runbook below.
4. **Task 5 — open fuzzy gates + retire the 033 proximity gate TOGETHER** (now safe — scrub is the
   precision pass; measured: don't loosen the matcher before the scrub exists).
5. **Task 7 — frontend** (scoracle-frontend): News card renders N narratives + All/Transfers scope +
   hottest-narratives leaderboard; vibe card unchanged.
6. **Task 8 — stats-rail fast-follow** (local model stat-profile summaries + Special card; reuses the primitives).

## BATCHED DEPLOY RUNBOOK (when approved)
1. Migrations 084/085 are ALREADY applied — nothing to migrate (re-confirm via `sql/migrate.sh`, idempotent).
2. `go -C go build -o bin/scoracle-api ./cmd/api`.
3. **Manual restart** — `systemctl --user restart scoracle-api.service`. The `scoracle-api.path` watcher is
   INERT (stale path), so a rebuild does NOT auto-restart. (See memory `backend-api-restart-mechanics`.)
4. Smoke: `/health` 200 (not degraded), boot log shows news_scrub + workers; a data endpoint returns data.
5. Regenerate transfers off the vetted corpus: `cmd/transfer -mode corpus` (heavy local model; staggered).
6. Generate first narratives/vibe for top entities (or let the task-10 cadence do it).

## GOTCHAS / STANDING RULES
- **Commit freely; get explicit permission before pushing OR deploying.** Restart prod ONLY via
  `systemctl --user restart scoracle-api.service` — NEVER pkill by pattern (prod shares the bin path).
- Apply migrations BEFORE the restart (db.New prepares statements at boot; bidirectional drift degrades).
- `.env`/`.env.local` live at the REPO ROOT; CLIs need the env exported or run from root.
- **`planning_docs/SCORACLE_RATING_ENGINE.md` has an unrelated, pre-existing local edit — left untouched
  all session; do not commit it.**
- Narrative generation ~100s/entity (NumPredict 4000, 25-article corpus) — a cadence tuning point.
- DEFERRED: transfer local model vet→determine promotion (Roadmap 1a); narrative genericization prompt-tuning
  (local model sometimes says "a super striker" when the source headline was clickbait-vague).

## VERIFICATION STATE
All commits: `go build ./... ` + `go vet` + `gofmt -l` clean. Live dry-runs/persists verified each stage
on Chelsea (scrub disambiguation; 6 narratives w/ per-narrative impact; vibe 73; transfer read 21→15).
