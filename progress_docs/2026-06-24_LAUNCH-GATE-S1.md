# LAUNCH-GATE Session 1 — Phase 0 carryovers (F-043, F-045 done; F-030 diagnosed)

**Date:** 2026-06-24 · **Machine:** archbox (prod) · **Driver:** `planning_docs/LAUNCH-GATE-KICKOFF.md`
**Not an audit session** — this opens the pre-launch milestone (FIRST-GPT-AUDIT S1–S17 complete).

## Goals

Pick up at the first unchecked phase of the launch-gate runbook = **Phase 0, clear the launch-blocking
carryovers** (F-030, F-045, F-035, F-040, F-043; F-046 on its own track). Work in bounded chunks; land what
is safely autonomous, hand the rest to Scott with executable runbooks.

## What shipped this session (committed, my files only)

| Finding | Before | After |
|---|---|---|
| **F-045** Swagger advertises removed `/twitter/*` + `/api/v1/news/*` | Open | **Resolved-pending-redeploy** — `go/docs/*` regenerated |
| **F-043** docker-compose `seed/` has no Dockerfile | Open (minor) | **Resolved** — `seed/Dockerfile` + `.dockerignore` added |
| **F-030** NFL/FOOTBALL zero season-2025 Sigils | Watch (vague) | **Watch — diagnosed + measured + Scott runbook** (grind not run) |

### F-045 — regenerate Swagger ✅ (redeploy pending)
- Ran `swag init -g cmd/api/main.go -o docs --parseDependency --parseInternal` in `go/` (swag v1.16.4; go.mod
  pins the library v1.16.6 — generated output compatible, `docs.go` builds clean).
- `twitter` + `/api/v1/news` mentions in the spec: **6 → 0**. The only `news` paths left are the live
  `/{sport}/{entityType}/{id}/news` + `/{sport}/leaderboard/news` (correct). Net **−736 lines** across
  `docs.go` / `swagger.json` / `swagger.yaml`.
- `go build ./cmd/api` passes → the embedded spec is valid.
- **Pending:** the spec ships embedded in the binary, so the live `/docs/` UI only updates on the next
  `release.sh`. Not deployed autonomously (a prod API restart while the parallel Rust session + nightly crons
  are active is Scott's call).

### F-043 — seed/Dockerfile ✅
- Added `seed/Dockerfile` (kept the build target rather than deleting — CLAUDE.md documents
  `docker compose run --rm seed event process …`). `python:3.12-slim`, `pip install .` of the three packages
  declared in `pyproject.toml` (`scoracle_seed*` / `services*` / `shared*`), non-root user,
  `ENTRYPOINT ["scoracle-seed"]`. `psycopg[binary]` bundles libpq → no build toolchain needed. Added
  `seed/.dockerignore`.
- **Not build-tested** (the Docker daemon is not running on archbox) — verified structurally: all three are
  real `__init__.py` packages and the CLI imports `services.*`. CI (F-042) still builds only `go/`.

## F-030 — the launch-gate Sigil backfill: diagnosed, measured, NOT run (deliberate)

### Live baseline (the real gap)
By `public.sigil_synthesis` season-stamp, current-season **rated** entities missing a `season=2025` Sigil:

| sport | rated 2025 | have 2025 sigil | **missing** |
|---|---|---|---|
| NBA | 283 | ~278 | **5** (effectively done) |
| NFL | 1072 | 0 | **1072** |
| FOOTBALL | 2147 | 0 | **2147** |

All NFL/FOOTBALL crowns today are legacy **NULL-season** rows (NFL 441, FOOTBALL 450), still serving via the
F-028 transition allowance. **Total grind = 3219** first-time syntheses.

### Why the existing machinery won't close it before launch
1. **Enqueue cap.** `cron-vibesynth.sh -mode nightly -limit 150` (O2-sized for steady-state maintenance, when
   the backlog was ~95% empty). The 2026-06-24 05:00 run detected **3307 candidates, enqueued 150** (the cap).
   At 150/night → ~21 nights.
2. **Drain order starves sigil.** The always-on derive worker (`derive.DrainAll`) drains
   **transfers → narratives → vibe → sigil, sigil LAST**, each stage fully before the next. With the transfers
   stage perpetually retrying model-failure pairs (fail-closed, F-020) and a 277-item vibe backlog ahead of it,
   sigil is starved: newest `season=2025` Sigil was **~12h stale** (NBA 2026-06-23 23:15) while vibe/news/stat
   were minutes-fresh. Enqueuing more sigil work does not help while sigil is last in line.

### Throughput measured
- Single dry-run synthesis, NFL `team/1` (New England Patriots): valid **Score 42**, coherent blurb, pillars
  present, season-stampable. **local model wall = 102s** — but under **3-way GPU contention**: the parallel Rust
  `parity` process, the `statcommentary` nightly cron (still running ~8h), and the API derive worker all share
  the single 8GB card.
- 3219 × ~100s ≈ **~90 GPU-hrs**. In a quiet window with the F-035 governor pinned, per-call should fall to
  ~30–60s → **~40–90 GPU-hrs ≈ 1.5–4 nights**.

### Why I did NOT run the grind
The resolution is a dedicated `vibesynth -mode backfill` (direct generation; holds the shared `vibesynth`
jobrun lock; season-stamps each row; bypasses the starved queue). It is a **multi-night GPU job** that contends
with live serving + the parallel Rust session, and the **F-035 cross-process governor is still unset**
(`OLLAMA_NUM_PARALLEL`/`OLLAMA_MAX_LOADED_MODELS` absent → thrash/OOM risk on the 8GB card when ≥2 local model
processes hit Ollama). At session start the GPU was already running the Rust `parity` job. Kicking off a fourth
local model driver autonomously would interfere with the parallel session and risk the card — **Scott's operational
call.** The synthesis path itself is proven for these sports (the probe).

### Runbook for Scott — the F-030 grind
1. **F-035 first** (pin the cross-process governor — see below). The grind runs ≥2 local model processes; without it
   they can collide on the 8GB card.
2. **Quiet the GPU:** pause/await the parallel Rust `parity` session (coordinate); confirm no
   `statcommentary` / `cmd/pipeline` / nightly `vibesynth` (05:00) run is mid-flight (`ps`, `pipeline_runs`).
3. **Run the backfill** (NFL first — smaller; the lock blocks the nightly reconcile so no collision):
   ```bash
   cd /home/sheneveld/scoracle/scoracle-backend
   nohup ./go/bin/vibesynth -mode backfill -sport NFL      -throttle-ms 250 >> logs/vibesynth-backfill-nfl.log 2>&1 &
   # when NFL is done:
   nohup ./go/bin/vibesynth -mode backfill -sport FOOTBALL -throttle-ms 250 >> logs/vibesynth-backfill-football.log 2>&1 &
   ```
   (`-limit N` to chunk it across nights; re-running only fills remaining gaps — `enumRatedMissing` drops
   entities once they have a season-stamped row.)
4. **Verify** until `missing_2025_sigil → 0` for NFL + FOOTBALL:
   ```sql
   WITH rated AS (
     SELECT sport,'player' et, player_id id FROM player_stats WHERE season=2025 AND rating_composite_score IS NOT NULL GROUP BY sport,player_id
     UNION ALL
     SELECT sport,'team', team_id FROM team_stats WHERE season=2025 AND rating_composite_score IS NOT NULL GROUP BY sport,team_id)
   SELECT r.sport,
     count(*) FILTER (WHERE s.entity_id IS NULL) AS missing_2025_sigil
   FROM rated r
   LEFT JOIN public.sigil_synthesis s
     ON s.sport=r.sport AND s.entity_type=r.et AND s.entity_id=r.id AND s.season=2025
   GROUP BY r.sport ORDER BY r.sport;
   ```
   Then the Convergence proof (Phase 3) + the "Sigil broadly populated / season-correct" launch criterion can pass.

## Scott-gated blockers (documented, not actionable autonomously)
- **F-035** — Ollama systemd drop-in (root): `Environment=OLLAMA_NUM_PARALLEL=1` +
  `Environment=OLLAMA_MAX_LOADED_MODELS=1`, then `sudo systemctl daemon-reload && sudo systemctl restart ollama`
  in a quiet `pipeline_work` window (the API derive worker defers-and-recovers, F-014). **Do before the F-030 grind.**
- **F-040** — pick the off-SITE backup target (cloud bucket via rclone / NAS); set `OFFHOST_BACKUP_DIR` to it.
  Mechanism + off-disk mirror already live (S15). Infra call.
- **F-046** 🔴 — credential rotation + git-history purge across 3 repos. Its own track:
  `PASSWORD-LEAK-REPAIR.md` + `progress_docs/2026-06-24_F-046-credential-leak-remediation.md`. Rotate FIRST.

## Landmines respected
- Never pattern-killed anything (F-001); no prod deploy / API restart; no migration (none needed this session —
  next free = **107**). `git fetch` first; staged only my own files — left `sql/migrations/099_team_rosters.sql`
  + `rust/*` untracked for the parallel Rust session. Read the LIVE schema, not migration files (F-015).

## Quick reference — state at session end
- F-043 ✅, F-045 ✅ (redeploy pending). F-030 diagnosed (3219 missing; ~40–90 GPU-hrs; grind gated on Scott).
- F-035 / F-040 / F-046 — Scott-gated, documented.
- Phases 1–4 (the per-sport Stats/News/Convergence/Operations proofs) not started — Phase 0 not fully cleared.
  Convergence proof is blocked on the F-030 grind; Operations proof on F-045 redeploy + F-035 + F-040.
