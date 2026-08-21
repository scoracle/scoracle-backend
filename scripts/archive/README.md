# scripts/archive — closed one-shot instruments

Retired here for legibility, not deleted: each was a phase-gate or bootstrap tool whose
decision window has closed. All are read-only or superseded; none is wired to cron or
systemd. Kept because the SQL documents how their phases were measured.

| Script | Was | Closed by |
|---|---|---|
| `rail-6.7-bands.sh` | PLAN-one-rail 6.7 — 72h organic-storyline reading bands | window closed 2026-08-08 |
| `rail-cutover-check.sh` | PLAN-one-rail 8.1 — the five §2 cutover clauses, 7-green-days gate | rail closeout 2026-08-15 (`progress_docs/2026-08-15_rail-swap-closeout.md`) |
| `rollout-relational-layer.sh` | migrations 154-170 bundler for a fresh environment | ledger moved past 219; fresh boxes follow RUNBOOK §11 + `sql/migrate.sh`. Its code pointer (`rust/src/graph.rs`) predates the junctions reorg |
