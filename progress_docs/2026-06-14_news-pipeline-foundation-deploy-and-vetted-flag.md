# 2026-06-14 — News→local model pipeline: foundation DEPLOYED + scrub vetted-flag

Picks up the 2026-06-13 foundation session. Two things this session: (1) **deployed**
the batched foundation to prod (the first prod change of this project), and (2) built the
**vetted-flag** scrub semantics (integration Plan Phase 2, step 1). Companion plans:
`~/scoracleWiki/wiki/Plan - News to local model Summaries.md` (what) +
`Plan - News pipeline integration.md` (how/next).

## Goal
Get the pipeline foundation live in prod (kill the ~11s live-Twitter latency, stand up the
new tables) and convert the scrub from destructive (DELETE dropped links) to non-destructive
(record a `vetted` verdict) so consumers can be flipped onto the clean set safely.

## What was done

### 1. Deployed the foundation (task 1) — prod
- Applied migrations `081_news_summaries` + `082_pipeline_stats` via `sql/migrate.sh`
  (additive `CREATE TABLE`; verified recorded in `schema_migrations` + both tables exist).
- Rebuilt `go/bin/scoracle-api` (cmd/api) and **manually** restarted
  (`systemctl --user restart scoracle-api.service`). **The `scoracle-api.path` watcher is
  INERT** — it watches the stale pre-consolidation path `/home/sheneveld/scoracle-backend/go/bin/`
  (does not exist), so a rebuild does NOT auto-restart (confirmed: service stayed on the old
  PID after rebuild). Memory `backend-api-restart-mechanics` was correct + current.
- Pre-flight (no degraded-mode risk): the new binary's only api change vs the running one is
  the `TwitterEnabled` gate (no DB); `db.go` prepared-statement changes were the already-applied
  `080` `rating_breakdown` reads; neither references the new tables. Validates both directions.

### 2. Scrub vetted-flag semantics (task 2) — built + committed, NOT yet applied
- `083_news_entity_vetting.sql`: `news_article_entities.vetted BOOLEAN` + `scrubbed_at
  TIMESTAMPTZ` (both nullable; NULL vetted = unscrubbed) + partial index
  `idx_news_entities_unscrubbed (article_id) WHERE scrubbed_at IS NULL` for the async worker
  backlog query. Verdict semantics: TRUE kept · FALSE local model-dropped · NULL unscrubbed.
- `ml/news_scrub.go` `applyVerdicts`: DELETE → `UPDATE ... SET vetted = <verdict>,
  scrubbed_at = NOW()` for **every** candidate (kept + dropped), so the article is auditable
  and the worker knows it's scrubbed. Primary link (conf 1.0) still always vetted=true.
- `cmd/newsscrub`: help/mode text updated (non-destructive, "recording vetted verdicts").

## Decisions
- **`vetted` flag, NOT delete** — non-destructive + auditable; keeps recall to re-judge; lets
  us compare fuzzy-vs-vetted before consumers trust it; roll back by ignoring the flag.
- **Manual restart is the deploy step** (path-watcher inert) — do not rely on auto-restart.

## Verification
- `go build ./...` + `go vet` clean. Scrub dry-run on Chelsea unchanged + correct (drops the
  two wrong "Pedro"/"João Pedro", keeps real `28931574`).
- Prod post-deploy: `/health` 200 healthy (not degraded); `/football/twitter/feed` now a fast
  503 (~0.4ms, was ~11s); `/football/health` real data (20,535 players, 21ms); vibe + transfer
  workers + listeners all up in the boot log.

## State / next
- Foundation is LIVE in prod. `083` is committed but **NOT applied** (gated on Scott).
- Next (gated): apply `083`, run a persist scrub on a sample to verify the flag write, then
  task 3 (wire the scrub async over unscrubbed candidate-rich articles), task 4 (flip
  consumers onto `vetted IS TRUE OR scrubbed_at IS NULL`), task 5 (open gates + retire 033
  together), task 6 (generators into cron/vibe_trigger), task 7 (frontend). Then the parked
  stats-rail fast-follow (`Plan - local model stat-profile summaries.md`).
