# Collation repair, and the case for a shared worker pool

**Session of 2026-08-24.** Operational, not feature work — plus one architectural finding worth
more than the maintenance was. Everything verified against production.

---

## 1. The glibc collation repair (done)

An OS update on 2026-08-23 evening moved glibc's collation version **2.43 → 2.44**. Postgres
b-tree indexes on text are stored in collation order, so every text index was built under rules
the OS no longer uses. The failure mode is **silent**: a lookup returns nothing rather than
erroring, and a unique constraint can admit a duplicate. 140 of 227 indexes were text-collated.

Waited for the churn to clear (it did, ~12:26), paused `scoracle-cognition` — the queue was empty
by then, so nothing was lost — and left `scoracle-api` up throughout.

Per database (`scoracle`, `postgres`, `template1`):

```sql
REINDEX DATABASE CONCURRENTLY <db>;   -- user indexes, no long write lock
REINDEX SYSTEM <db>;                  -- catalogs; CONCURRENTLY is not supported for them
ALTER DATABASE <db> REFRESH COLLATION VERSION;
```

**`ALTER DATABASE … REFRESH COLLATION VERSION` was not sufficient on its own.** It clears the
database-level version, but the collation OBJECTS in `pg_collation` keep their own — `en_US.utf8`
and `en_US` still read 2.43 afterwards and needed:

```sql
ALTER COLLATION pg_catalog."en_US.utf8" REFRESH VERSION;
ALTER COLLATION pg_catalog."en_US" REFRESH VERSION;
```

**Verified after:** no flagged collations; all three databases at 2.44; **no invalid indexes and
no leftover `_ccnew`** (the specific debris a failed `REINDEX CONCURRENTLY` leaves); a fresh
connection prints no WARNING. Smoke-tested the lookups that were actually at risk — team by name,
player by name, `pipeline_work` by `sport`, `schema_migrations`, `sports` — all correct.

**Side effect worth knowing:** index size fell from **3,658 MB → 1,536 MB**. Same 227 indexes;
the rebuild reclaimed ~2.1 GB of bloat. A periodic reindex is apparently worth something here
independent of collation.

Script and log left at `archbox:~/fix_collation.sh` and `~/fix_collation.log`.

---

## 2. The churn cleared on its own

For the record, because it explains the 2026-08-23 "narratives is stalled" note:

| time | narratives outstanding |
|---|---|
| 08:07 | 537 (331 pending + 206 failed) |
| 08:40 | 445 |
| 09:09 | 374 |
| 12:26 | **0** |

Measured drain ~158–167/hour. Sigil's 1,999-item backlog cleared overnight separately.

**`pending` sat at exactly 331 for hours while only `failed` dropped**, which looked like a stall
and was not: the claim orders by `available_at`, the failed rows carried 03:10 stamps and the
pending ones 03:12–03:22, so the retries were simply first in line. All rows were `player` grain,
so the stage's team-first claim ordering never came into it.

Root cause of the 03:00 failure burst — 202 items, all
`ollama request: error sending request … 192.168.1.77:11434` — was archbox briefly unable to
reach the Mac. **Never identified.** Both hosts talk fine now (7 ms). Worth naming as unexplained
rather than closed.

---

## 3. THE FINDING: a ~43-minute outage permanently destroys a queue

This is the part worth keeping.

`work::fail()` increments `attempts` **regardless of cause** — a network blip costs an item one of
its five lives exactly as if the model had produced garbage. And at the fifth:

```sql
available_at = CASE WHEN attempts + 1 >= $6 THEN NOW() + INTERVAL '100 years' ... END
```

A permanent dead-letter, invisible unless queried for. There is **no retry inside the inference
layer**, and `Router::for_role` is single-valued — one role, one backend, no fallback. The
`candidates` map exists but is explicitly *"NEVER served; read only by `bin/eval`."*

Put together: a connection-refused fails in **milliseconds**, so the worker burns items as fast as
the backoff ramp permits (30s → 2m → 10m → 30m). **Roughly 43 minutes of Mac downtime parks every
item in that stage's queue for a century.** On 08-23 the outage was short and items topped out at
attempts 2–3; an overnight sleep would have destroyed all 331.

One row is already parked this way (`momentum`, player grain) and nobody knew.

---

## 4. Scott's answer, which is better than the fixes proposed against it

Proposed in-session: don't burn attempts on infrastructure failure; a per-backend circuit breaker
(the `BudgetedFetcher` pattern, keyed on `base_url`); failover to the other host.

Scott, 2026-08-24:

> *"What if rather than these fixes, we established a system where work is available for any
> machine to grab, as long as it's grabbed in order? That way we could have one machine connected
> to the drain or 5, and they'd all be grabbing from the workable pools. It would eliminate one
> machine being idle, and we don't need to worry about one machine stopping."*

**The queue already IS that system.** The claim is
`WHERE status IN ('pending','failed') AND available_at <= NOW() ORDER BY … FOR UPDATE SKIP LOCKED`
— a textbook multi-consumer pool. Postgres guarantees no two workers take the same row, and the
stale-lease recovery loop (`'running'` → `'pending'`, 1800s lease) already exists to reclaim a
dead worker's in-flight items.

**The problem was never the queue — it is the TOPOLOGY.** Today one worker on archbox makes
*remote model calls* to the Mac. The Mac is an inference endpoint, not a worker, so a Mac hiccup
becomes archbox's problem and the ITEM takes the blame for a machine boundary failing mid-flight.

Inverting it — N workers, each on its own GPU, all pulling the shared pool — means a machine
vanishing costs only its leases (auto-recovered), nothing idles, and a third or fifth box is
config rather than code. **It dissolves the circuit breaker and the failover entirely**, because
there is no cross-machine model call left to fail. The proposed fixes were treating a symptom.

Plan: `planning_docs/PLAN-availability-and-boxscores.md` §7.

---

## 5. State at close

- Collation repair complete and verified; harness and API healthy; 9 stages registered
  (`fixture_boxscore` correctly still absent).
- Queue: 174 inert `fixture_boxscore` rows + 1 dead-lettered `momentum` row.
- Branch `availability-record-and-public-source-turn`, **not pushed**.
- Models unchanged and uniform (`ministral-3:3b` both hosts) — Scott's call after the 8B
  headroom check showed archbox's 1070 Ti has 8 GB VRAM against ~6 GB of weights at concurrency 4.
