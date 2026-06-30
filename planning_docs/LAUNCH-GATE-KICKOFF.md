# LAUNCH-GATE-KICKOFF.md — pre-launch milestone runbook

Opens the last pre-launch milestone after the FIRST-GPT-AUDIT (S1–S17 COMPLETE, S17 = `a2038a1`).
**This is not an audit session.** It sequences the remaining launch-blocking carryover findings and
the Final launch gate (one deliberate end-to-end proof per sport) + the launch decision. Multi-session
— keep each session bounded and update the findings ledger as you go.

Canonical sources: `planning_docs/FIRST-GPT-AUDIT.md` (`# Final launch gate` section) +
`planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` (carryovers) + `RUNBOOK.md`. Tap-to-copy kickoff prompt:
see the appendix.

## Status

| Phase | State |
|---|---|
| Phase 0 — clear launch-blocking carryovers | ⏳ S1: F-043 ✅ + F-045 ✅ done; F-030 diagnosed+measured (grind gated on Scott); F-035/F-040/F-046 Scott-gated |
| Phase 1 — Stats proof (× nba/nfl/football) | ⏳ |
| Phase 2 — News proof (× nba/nfl/football) | ⏳ |
| Phase 3 — Convergence proof (× nba/nfl/football) — after F-030 | ⏳ |
| Phase 4 — Operations proof — after F-045/F-035/F-040 | ⏳ |
| Phase 5 — Launch decision | ⏳ |

## Setup / ground truth

- The repo IS `scoracle-backend` (its own `.git`) — `cd scoracle-backend` for all git ops.
- archbox = prod (prod DB, Ollama/Gemma, cron, systemd). Three sports: `nba`, `nfl`, `football`;
  `current_season` = 2025 for all.
- Live per-entity products: `/stats /rating /momentum /sigil /news /transfers /meta` (+
  `team/{id}/results`, `team/{id}/roster`). **Route truth = `go/internal/api/server.go`, never
  prose/memory (F-044 lesson).**
- **First step per CLAUDE.md:** `git fetch && git status`, confirm synced with `origin/main` — a
  parallel Rust session shares this tree and pushes here, so `git fetch` before any commit, stage ONLY
  your own files (never `git add -A`), and leave `sql/migrations/099_team_rosters.sql` + `rust/*`
  untracked.

## Read first

- `planning_docs/FIRST-GPT-AUDIT.md` → `# Final launch gate` (the 4 proofs + Launch decision criteria).
- `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` (the carryover findings below).
- `RUNBOOK.md` — release/rollback §2–3, backup/restore §4, jobs §6, pipeline §7, incident §10,
  carryovers §12.
- `PASSWORD-LEAK-REPAIR.md` for F-046.

---

## Phase 0 — clear the launch-blocking carryovers (gate prerequisites)

- [ ] **F-030 (LAUNCH-GATE):** NFL (1072) + FOOTBALL (2147) current-season entities have ZERO
      season-2025-stamped Sigils. **S1 (2026-06-24): diagnosed + measured, grind NOT run — gated on Scott.**
      Live baseline = 3219 missing (NFL 1072/1072, FOOTBALL 2147/2147; NBA effectively done at ~278/283).
      The nightly cron caps enqueue at `-limit 150` and the worker drains sigil LAST (starved behind the
      transfers churn + vibe backlog), so neither path closes it in time. ~100s/synthesis under contention →
      ~40–90 GPU-hrs (~1.5–4 nights) of a dedicated `vibesynth -mode backfill`. Synthesis path PROVEN
      (NFL `team/1` probe → Score 42). Resolution is a multi-night GPU grind in a quiet window (pause the
      Rust parity session; set F-035 first) — **Scott's operational call.** Runbook: `progress_docs/
      2026-06-24_LAUNCH-GATE-S1.md` §F-030. Required before the Convergence proof + the "Sigil broadly
      populated" launch criterion. (Relates to **F-028** — keep the legacy NULL-season allowance until stamped.)
- [x] **F-045:** ✅ S1 regenerated `go/docs/*` via `swag init` (stale route mentions removed; builds clean).
      **Redeploy via `release.sh` still pending** (spec is embedded; `/docs/` updates on next deploy — Scott).
      Satisfies the "docs describe the same system" criterion for the Swagger surface once redeployed.
- [ ] **F-035:** set the Ollama systemd drop-in `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1`
      (needs sudo — guide Scott via `! <cmd>`). The Go GPU governor is in-process only. **Do this BEFORE the
      F-030 grind** — it pins the cross-process serialization the multi-process Gemma grind relies on.
- [ ] **F-040:** pick the off-SITE backup target (cloud/NAS) — mechanism is ready (`OFFHOST_BACKUP_DIR`;
      off-disk mirror already live). Scott's infra call; required for the Operations-proof off-host restore.
- [x] **F-043:** ✅ S1 added `seed/Dockerfile` (+ `.dockerignore`) — `docker compose build seed` now has a
      target. Verified structurally (no Docker daemon on archbox to build-test); CI still builds `go/` only.
- [ ] **F-046 🔴:** the credential-leak repair runs on its own track via `PASSWORD-LEAK-REPAIR.md`
      (rotation + history purge, gated on Scott). A launch blocker — coordinate, but don't fold it into
      a proof session.

> Not blockers (cosmetic / optional / watch — skip unless trivially in the way): F-003, F-005, F-021,
> F-024, F-027, F-029, F-034, F-036, F-037, F-048.

---

## Phases 1–4 — Final launch gate (run per sport: `nba`, `nfl`, `football`)

### Phase 1 — Stats proof
1. Load a fixture.
2. Confirm it is not eligible before finality.
3. Process complete final box scores.
4. Confirm event rows, season aggregates, ratings, and commentary.
5. Confirm `/stats` and `/rating`.
6. Re-run and verify idempotency.

### Phase 2 — News proof
1. Compile a new RSS article.
2. Confirm exact links are scrubbed.
3. Confirm rejected links do not reach consumers.
4. Confirm accepted links enqueue durable work.
5. Confirm transfer verdict, narratives, and Vibe.
6. Confirm `/news` and its Transfers scope.
7. Append a no-data marker and confirm old current content clears.
8. Simulate process death between stages and confirm recovery.

### Phase 3 — Convergence proof (after F-030)
1. Change a current Rating or Vibe input.
2. Confirm both trajectories are recomputed into Momentum.
3. Confirm Rating + Vibe + Momentum enqueue one debounced Sigil convergence.
4. Confirm `/momentum` exposes both trajectories and `/sigil` exposes the holistic synthesis.
5. Re-run reconciliation with unchanged inputs and confirm no duplicate Sigil is appended.
6. Confirm prior Momentum and Sigil generations remain available as append-only history.

### Phase 4 — Operations proof (after F-045/F-035/F-040)
1. Deploy all binaries from one commit.
2. Verify database-aware readiness.
3. Restart Postgres and Ollama independently.
4. Confirm raw ingestion and durable work recover.
5. Restore the latest off-host backup into a throwaway database.
6. Boot the API against the restored database.
7. Confirm cron jobs return meaningful statuses.

---

## Phase 5 — Launch decision

Launch only when:
- [ ] all three sports update automatically;
- [ ] no pipeline stage depends on an ephemeral notification for correctness;
- [ ] unverified content cannot be served as verified;
- [ ] marker rows clear stale current products;
- [ ] Momentum combines both rail trajectories;
- [ ] Sigil is season-correct, broadly populated, and generated from Rating + Vibe + Momentum;
- [ ] scheduled work only reconciles missing/stale Sigils and never creates unchanged duplicates;
- [ ] health checks reflect actual serving readiness;
- [ ] a verified off-host restore can boot the backend;
- [ ] the deployed binaries, schema, service files, cron, and documentation all describe the same system;
- [ ] F-046 closed (credentials rotated; history purged).

## Landmines / conventions

- Never pattern-kill backend procs (F-001) — `systemctl --user restart scoracle-api.service`.
- Migrate-before-restart EXCEPT drop-column = binary-first (F-022). **Next free migration = 107.**
- Validate prepared stmts via a throwaway `db.New` boot (or `cmd/validate-stmts`) BEFORE any prod
  restart (F-025).
- Deploy = `release.sh` (all 4 binaries from one commit; brief path-watcher flap, F-016).
- Read the LIVE schema, not the migration files (F-015).
- `migrate.sh` is unsafe while the parallel session has unrecorded migrations on disk — use per-file
  psql (F-006/F-031).

## At session end

Update each touched finding's Status in `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` + this doc's Status
table; write a `progress_docs/` entry; commit + push your own files (`git fetch` first, stage only
yours); update the `[[first-gpt-audit-execution]]` memory + the relevant MEMORY.md line with proof
results + what's left.

---

## Appendix — tap-to-copy kickoff prompt

Paste this to start a milestone session:

> **Pre-launch milestone — Final launch gate + carryover blockers.** The FIRST-GPT-AUDIT is COMPLETE
> (S1–S17, S17 = `a2038a1`); this is NOT an audit session. Drive `planning_docs/LAUNCH-GATE-KICKOFF.md`:
> `cd scoracle-backend`, then `git fetch && git status` (a parallel Rust session shares this tree — stage
> only your own files, leave `099` + `rust/*` untracked). Read the runbook + its referenced sources
> (`FIRST-GPT-AUDIT.md` `# Final launch gate`, `FIRST-GPT-AUDIT-FINDINGS.md`, `RUNBOOK.md`,
> `PASSWORD-LEAK-REPAIR.md`), pick up at the first unchecked phase in its Status table, and work in
> bounded chunks. Phase 0 = clear launch blockers (F-030 Sigil backfill, F-045 swagger, F-035 ollama
> governor, F-040 off-site backup, F-043 dockerfile; F-046 via its own runbook); Phases 1–4 = run the
> Stats/News/Convergence/Operations proofs per sport (nba/nfl/football); Phase 5 = the launch decision.
> Landmines: never pattern-kill (F-001 — `systemctl --user restart scoracle-api.service`);
> migrate-before-restart except drop-column = binary-first (F-022); next free migration = 107; validate
> prepared stmts before any prod restart (F-025); read the LIVE schema not migration files (F-015). At
> session end: update finding statuses + the runbook Status table, write a `progress_docs/` entry, commit
> + push your own files (`git fetch` first), update the `[[first-gpt-audit-execution]]` memory + MEMORY.md.
