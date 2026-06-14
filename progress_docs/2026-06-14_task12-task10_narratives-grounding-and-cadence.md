# 2026-06-14 — Task 12 (narratives grounding) + Task 10 (pipeline cadence)

Continuation of the News→Gemma staged pipeline build (see
`2026-06-14_SESSION-HANDOFF_news-gemma-pipeline.md`). These two tasks complete the
pre-deploy coding work; everything here is committed but **awaits the batched binary deploy**.

## Goal
- **Task 12** — ground the narratives generator on the entity's VETTED transfer rumors so transfer
  storylines name the real counterparty + true direction/stage instead of re-inferring them from
  clickbait, and don't duplicate what the Transfers drill-down structures.
- **Task 10** — orchestrate the staged cadence (sweep → transfers → narratives → vibe) as a
  once-daily chain, make the real-time spike path narrate-before-vibe, and snapshot the corpus daily.

## Decisions
- **Cadence = full chain 1×/day overnight** (Scott). One cron entry; the in-API LISTEN/NOTIFY
  workers cover intraday spikes. Simpler than a split cadence.
- **One orchestrator process** (`cmd/pipeline`), not chained stage CLIs — the cross-stage
  "touched entities this run" handoff uses an in-process `runStart` watermark.
- **Per-stage batches** (all transfers → all narratives → all vibes); the DB is the handoff. Vibe
  reads each entity's latest narratives, so a full narratives pass before the vibe pass keeps it fresh.
- **Genericization tightening shipped now** (Scott): the narrator drops vague clickbait that names
  no concrete subject rather than papering it over ("a Super Striker", "the Germans"). Reveal signal,
  cut noise — the product's core value.
- **Reuse over duplication**: extracted the transfer-heat loader/formatter into a shared `ml`
  primitive and the RSS sweep + touched-entity selection into a new `internal/corpus` package, used
  by both `cmd/vibe` and `cmd/pipeline`.

## What was done
### Task 12 (commit `1709678`)
- New `go/internal/ml/transfer_heat.go` — `loadTransferHeat` + `writeHeatLines`, shared by the vibe
  and narratives stages (vibe.go delegates to it; behavior unchanged).
- `news_narratives.go` — best-effort heat load (never blocks the narrative), facts section injected
  only when heat exists, system-prompt rules to (a) treat the facts as confirmed truth and (b) cut
  vague clickbait. Prompt `n1 → n2`.

### Task 10
- New `go/internal/corpus/corpus.go` — `Sweep`, `LoadTeams`, `LoadTouchedEntities`,
  `RecentlyGenerated`, `LookupEntityName`, `Entity`/`Team` types. `cmd/vibe` refactored to use it.
- New `go/cmd/pipeline/main.go` — `-mode corpus` runs sweep → transfers (all teams) → narratives →
  vibe over the touched-delta, per-stage debounce + throttle flags, `-limit` for smoke/ops.
- `internal/listener/news_volume_worker.go` — on a news spike: refresh narratives first (new
  `recentlyNarrated` debounce), then vibe. `cmd/api/main.go` constructs + wires the narrator.
- `internal/maintenance/maintenance.go` — new `pipeline_stats` daily ticker (`StatsInterval`, ~24h),
  pure-SQL upsert per (sport, today): article counts, fresh-narrative/vibe coverage, distinct ACTIVE
  transfer pairs (latest gen, heat>0, 14d — not raw append rows), coverage% + median staleness over
  the in-scope entity set. Runs once on startup then on interval. `config.go`:
  `PIPELINE_STATS_INTERVAL_MINUTES` (default 1440, 0 disables).
- `scripts/hosting/cron-pipeline.sh` (new) + `crontab.example` — one overnight pipeline entry
  **replaces** the separate vibe + transfer cron entries.

## Verification
- `go build ./...`, `go vet`, `gofmt -l` all clean.
- Task 12: Chelsea (team 18) dry-run — `prompt=n2`, transfer storylines name the vetted rumors
  (Rogers/Cucurella/João Pedro/Bowen); the generic squad-filler narratives are gone (6→5).
- Pipeline stats SQL validated read-only against prod (FOOTBALL: 24105 articles, 161 vibed, 603
  active transfer pairs, 56.9% coverage, 19.1h median staleness).
- Pipeline orchestrator: bounded smoke `-sport FOOTBALL -limit 1` (exit 0, 5m54s) — staged order
  confirmed end-to-end: sweep (137 teams) → transfers (rumors=4, cleared=1) → narratives (ok=1) →
  vibe (ok=1) → complete; `-limit` cap worked (145 touched → 1).

## Deploy (pending approval) — see the session handoff runbook
Migrations already at 085 (these tasks add none). Build all binaries to `go/bin/`, **manual**
`systemctl --user restart scoracle-api.service`, install the updated crontab (replacing the
vibe+transfer entries), then kick the first `cron-pipeline.sh -mode corpus`.
