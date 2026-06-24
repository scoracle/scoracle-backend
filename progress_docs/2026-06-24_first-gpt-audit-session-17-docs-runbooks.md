# FIRST-GPT-AUDIT — Session 17: Reconcile backend documentation and runbooks

**Date:** 2026-06-24 · **Machine:** archbox · **Type:** docs + code comments + new runbook —
**NO migration, NO deploy, NO API restart, no `release.sh`.** Next free migration stays **107**.
**This is the final audit session (S17 of 17) — S1–S17 complete.**

## Goal

Make the repository documentation trustworthy during an incident or a machine rebuild. Backend docs
described removed routes and retired integrations; operational instructions disagreed about live
paths and restart behavior. Reconcile `CLAUDE.md` / `README.md` / `ENDPOINTS.md` (+ code comments)
to the **actual live system**, and write runbooks.

## The crux — verify against the live system, not the prose (F-044)

The audit plan (2026-06-21) said "remove `/special` `/trends` per-entity `/vibes`." The S17 kickoff
brief countered they SHIPPED LIVE (two-rail model) and must be KEPT. **Both were stale.** Read live
against `go/internal/api/server.go` — and confirmed byte-identical in the **deployed** S14 binary
`cf4f260` — the **O14 convergence rename already landed**:

- `/special` → folded into **`/rating`** (the "divined" stat read + `stat_summaries` commentary)
- `/trends` → **`/momentum`** (Rating × Vibe trajectory)
- per-entity `/vibes` → **`/sigil`** (the crown synthesis)
- bundled profile route `/{sport}/{entityType}/{id}` → **removed** (O16)

So the live per-entity products are: `/stats`, `/rating`, `/momentum`, `/sigil`, `/news`,
`/transfers`, `/meta` (+ `team/{id}/results`, `team/{id}/roster`). The audit was right that the old
names should leave the docs, but for the wrong reason (renamed, not deleted); the two-rail memory was
a snapshot the convergence rename superseded. **There was no deployed-vs-tree routing gap** — S15/S16
didn't touch routing, and the rename predates S14.

## Decisions

- **Source of truth = code, always.** Every documented route checked against `server.go`; cron
  against `crontab.example`; binaries against `release.sh`; env against `config.go`. Where the audit
  or a memory disagreed with the code, the code won and the disagreement became a finding.
- **New `RUNBOOK.md` at the repo root** (not a rewrite of the historical `SELF_HOSTING_OPS.md`) — the
  single incident-facing doc the audit was missing, cross-linked from README + the hosting README.
- **Code-comment fixes kept minimal** — only two clearly-misleading comments (`db.go` stmt header,
  `cmd/api` Swagger `@description`). Scattered "tweet"/"sentiment" prose left for a future code pass
  (F-048) to keep the diff docs-focused and avoid churn in files the parallel Rust session is in.
  `divined_sigil` references in `ml/sigil.go`/`cmd/vibesynth` are **intentional** (legacy key the
  convergence migration converts to `divined_peak`) — not stale.
- **Swagger NOT regenerated** (F-045) — it's generated code embedded in the serving binary; `swag
  init` + redeploy is the next deploy's job, not a docs-only session's. Flagged at point of use in
  `ENDPOINTS.md`.

## Accomplishments

- **`CLAUDE.md`** — Route Conventions rewritten to the per-product live route list (bundled profile
  removed; `/news`+`/twitter` removed, not "being retired"); Environment section's "parked Twitter"
  block replaced with the real optional env vars (Ollama/derive/scrub/transfer/JWT) + Twitter-gone note.
- **`README.md`** — Architecture + service-responsibility framing de-Twittered; API Surface rewritten
  to match `server.go` (per-entity products, all leaderboards incl. `sigil`/`trending`, league-scoped,
  operational + auth; removed-routes note); Environment Variables fixed (DB-URL priority `PRIVATE >
  URL`, no `RAILWAY`; dropped `TWITTER_*`; added the real vars); RUNBOOK pointer.
- **`ENDPOINTS.md`** — date → 2026-06-24; added an **authoritative route inventory** table at the top
  (verified vs `server.go`, with a removed-routes list + the F-045 Swagger-drift warning); fixed the
  `/stats · /special` heading → `/stats · /rating`; added a `/rating` contract + a
  `/leaderboard/trending` contract; corrected stale `"page"` literals (`sparkline`→`stats`,
  `trends`→`momentum`); marked the bundled "Profile Response Example" historical; corrected the
  Backend Implementation Map (`derive` worker, `cmd/work`, removed the non-existent
  `listener/news_volume_worker.go`).
- **`RUNBOOK.md` (new, ~360 lines)** — system map; release (`release.sh`, all-4-from-one-commit) +
  migrate-before-restart boot guard + F-022 destructive-migration ordering; rollback (incl. F-001
  no-`pkill` landmine + schema-coupled caveat); backup + 5-stage bootable-backend restore drill +
  off-SITE gap; migrations; **cron-vs-event-driven jobs** (Sigil event-driven + debounced, cron
  `vibesynth` backstop only; Transfers as a News scope); compile→scrub→derive→reveal pipeline;
  durable work tables + `cmd/work` repair commands + `pipeline_runs_latest`; S16 CI gate; health +
  incident quick-reference; bare-metal rebuild checklist; launch-gate carryovers.
- **`SELF_HOSTING_OPS.md`** — scrubbed a hardcoded DB password (literal redacted) from two examples
  (→ `PGPASSWORD` from env), added a historical banner + RUNBOOK pointer (F-046).
- **Code comments** — `db.go` per-product stmt header corrected to live routing (`/rating`
  commentary, `/momentum` series); `cmd/api/main.go` Swagger `@description` de-Twittered.
- **Findings F-044…F-048** appended; Session 17 marked ✅ COMPLETE.

## Findings recorded (F-044…F-048)

| ID | Summary | Status |
|---|---|---|
| F-044 | Audit + memory both stale; docs disagreed with live router (the convergence rename) | Resolved (docs) |
| F-045 | Committed Swagger (`go/docs/`) still lists removed `/twitter/*` + `/api/v1/news/*` | Open — `swag init` + redeploy |
| F-046 | Leaked DB password + stale paths in `SELF_HOSTING_OPS.md` | Doc scrubbed; rotate password (Scott) |
| F-047 | New `RUNBOOK.md` is the durable incident/rebuild home | Resolved |
| F-048 | Scattered "tweet"/"sentiment" prose in non-serving comments | Open (cosmetic) |

## Quick reference

- **Live route truth:** `go/internal/api/server.go`. Inventory mirrored at the top of `ENDPOINTS.md`.
- **Operations:** `RUNBOOK.md` (release/rollback, backup/restore, jobs, work queue, incidents).
- **Live names:** `vibe_scores`=Vibe · `sigil_synthesis`=Sigil · `news_summaries`=narratives ·
  `stat_summaries`(`divined_peak`)=stat commentary · `transfer_rumors`=transfers.
- **Launch-gate carryovers:** F-030 (Sigil backfill NFL/FOOTBALL), F-040 (off-SITE backup target),
  F-035 (Ollama systemd drop-in), F-043 (`seed/Dockerfile`), F-045 (regenerate Swagger).

## Files touched (this session only)

```
CLAUDE.md
README.md
ENDPOINTS.md
RUNBOOK.md                                   (new)
go/internal/db/db.go                         (1 comment block)
go/cmd/api/main.go                           (1 Swagger @description line)
planning_docs/SELF_HOSTING_OPS.md            (password scrub + banner)
planning_docs/FIRST-GPT-AUDIT.md             (S17 ✅ COMPLETE blockquote)
planning_docs/FIRST-GPT-AUDIT-FINDINGS.md    (F-044…F-048)
scripts/hosting/README.md                    (RUNBOOK pointer)
progress_docs/2026-06-24_first-gpt-audit-session-17-docs-runbooks.md  (this file)
```

Left untracked (parallel Rust session — not S17's): `rust/`, `sql/migrations/099_team_rosters.sql`,
`progress_docs/2026-06-24_rust-cognition-library-plan.md`, `go/internal/ml/vibe_parity_test.go`.
