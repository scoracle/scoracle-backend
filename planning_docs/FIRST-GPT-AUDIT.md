# First GPT Backend Audit — Launch Hardening Plan

**Audit date:** 2026-06-21  
**Audited branch:** `main`  
**Audited commit:** `1e08b493958e1a0882f326838dec03f7e7346c3b`  
**Scope:** Backend only — ingestion/seeding, both data rails (stats and news), Gemma 4 processing, PostgreSQL derivation, Go endpoint serving, and backend operations. Frontend work is explicitly excluded.

## Purpose

This document converts the first full backend audit into a sequence of focused implementation sessions. The work should be performed on the machine that has access to the production database, Ollama/Gemma 4, installed cron jobs, and live systemd services.

Each numbered improvement is intended to be handled in its own dedicated session. Some sessions have dependencies, noted below. Avoid combining unrelated fixes merely because they touch the same language or directory.

## Findings ledger

Every session surfaces things outside its own scope — surprises, cross-session dependencies,
deliberate deferrals, operational gotchas, and "fix this in Session N" notes. **At the end of
every session, record those in `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md`** (one entry each), so
the knowledge compounds across sessions instead of living only in per-session progress docs or
operator memory. That ledger is append-only: when a later session acts on a finding, update its
**Status** rather than deleting it. What belongs there is anything a *future* session, the launch
gate, or an operator should know — **not** the work the current session actually did (that goes
in the session's `progress_docs/` entry).

## Product authority and invariants

The wiki **Product Narrative** is authoritative when this audit and the product model differ. Backend
hardening must preserve these invariants:

- Scoracle is a curated derivation engine built around **compile → scrub → reveal**, not a passthrough
  aggregator.
- The statistical and emotional rails are equal sources. The statistical rail ends in **Rating**;
  the emotional rail ends in **Vibe**.
- **Momentum** is the combination of the Rating trajectory and the Vibe trajectory over time. It
  does not belong to either rail alone.
- **Sigil** is a separate convergence product synthesized from **Rating + Vibe + Momentum**. It is
  not the final stage of the news rail.
- Sigil generation is event-driven and debounced. Scheduled execution may repair or backfill missed
  work, but must not become the normal source of Sigil generations.
- Transfers is a scope within News. A backend `/transfers` contract may support that scope, but it
  must not be modeled as an independent card, tab, headline score, or leaderboard dimension.
- Vibe is an internal/end-product signal with no standalone card. It feeds Meta, Momentum, and Sigil.
- Derived outputs are append-only and time-stamped. Marker rows may change the current projection,
  but must never delete, overwrite, or invalidate historical derivations.
- Public contracts remain product-oriented and presentation-free: `/meta`, `/stats`, `/rating`,
  `/news` (including its Transfers scope), `/roster`, `/momentum`, and `/sigil`.

The guiding principle is:

> Prefer explicit, durable state over timing assumptions, in-memory watermarks, best-effort notifications, or clever recovery behavior.

## Current architecture

### Stats rail

```text
Provider schedules
  → fixtures and provider ID maps
  → pending fixture selection
  → provider box scores
  → event_box_scores / event_team_stats
  → finalize_fixture()
  → season aggregates, percentiles, ratings, event scores
  → Gemma 4 stat commentary
  → stat_summaries / Rating generations
  → prepared PostgreSQL JSON queries
  → Go /stats and /rating endpoints
```

### News rail

```text
Google News RSS
  → news_articles / news_article_entities
  → Gemma 4 entity scrub
  → vetted links
  → transfer analysis
  → narratives
  → Vibe generations
  → prepared PostgreSQL JSON queries
  → Go /news endpoint and its Transfers scope
```

### Convergence

```text
Rating generations ───────────────┐
                                  ├→ Rating trajectory ─┐
Vibe generations ─────────────────┤                     ├→ Momentum
                                  └→ Vibe trajectory ───┘

Rating + Vibe + Momentum
  → event-driven, debounced Gemma 4 synthesis
  → append-only Sigil generation
  → prepared PostgreSQL JSON queries
  → Go /momentum and /sigil endpoints
```

The component boundaries are generally good. The primary weakness is that transitions between components are often represented by timestamps, process-local state, cron timing, or transient `LISTEN/NOTIFY` messages instead of durable work records.

---

# Recommended execution order

## Launch-blocking sequence

1. Establish baseline and production safety checks.
2. Fix service paths, release mechanics, and health checks.
3. Add NBA/NFL ingestion scheduling.
4. Strengthen fixture finality and completeness validation.
5. Repair BDL error and rate-limit propagation.
6. Make deferred season recomputation durable.
7. Introduce durable news-pipeline work state.
8. Make compile → scrub → derive → reveal an ordered pipeline.
9. Repair real-time news trigger semantics.
10. Make transfer validation fail closed.
11. Standardize latest-generation marker semantics.
12. Repair convergence and the event-driven Sigil lifecycle.
13. Make batch jobs report failure and prevent overlap.
14. Harden Ollama/Gemma lifecycle management.
15. Harden backup, restore, and migration operations.
16. Add focused automated tests and CI.
17. Reconcile backend documentation and runbooks.

Sessions 7–9 should be designed together but implemented separately. Sessions 10–12 depend on having confidence in the pipeline state model. Operations work can begin earlier, but the final launch review should happen only after all launch blockers are complete.

---

# Session 1 — Establish a production baseline

## Problem

Subsequent changes will touch ingestion state, generated products, migrations, cron, and live services. A known baseline is required before changing behavior.

## Work

- Confirm `main` is synchronized with `origin/main`.
- Record the deployed commit for:
  - `scoracle-api`
  - `pipeline`
  - `statcommentary`
  - `vibesynth`
  - Python seeder package
- Record installed systemd unit contents, not just repository templates.
- Record the live crontab.
- Record all applied migrations from `public.schema_migrations`.
- Capture row counts and freshness summaries for:
  - `fixtures`
  - `event_box_scores`
  - `event_team_stats`
  - `player_stats`
  - `team_stats`
  - `news_articles`
  - `news_article_entities`
  - `transfer_rumors`
  - `news_summaries`
  - `vibe_scores`
  - `stat_summaries`
  - `sigil_synthesis`
- Capture current pipeline backlog:
  - Pending and retry-exhausted fixtures.
  - Unscrubbed news links.
  - Rated entities without stat commentary.
  - Rated entities without a current Sigil.
- Take and verify a database backup before the first migration.

## Verification

- Baseline is saved in a dated progress document.
- Live binaries can be mapped to one source commit.
- The live systemd and cron configuration are preserved for rollback.
- Backup file exists and passes a corrected restore drill.

## Done when

There is enough information to distinguish a pre-existing data defect from a regression introduced during this plan.

---

# Session 2 — Correct service paths, release mechanics, and health checks

## Problems

- Repository systemd templates still use the stale pre-consolidation path `/home/sheneveld/scoracle-backend`.
- Cron wrappers use `/home/sheneveld/scoracle/scoracle-backend`.
- Installing the repository templates on a replacement machine can recreate a known-broken deployment.
- The API starts without a database and `/health` still reports HTTP 200.
- Railway probes `/health`, so a database-less API can be considered healthy.
- The API handles `os.Interrupt` but not the normal container termination signal, `SIGTERM`.
- Only the API binary is built by Docker; cron binaries rely on separate manual builds.

## Work

- Replace stale systemd paths with the canonical repository path.
- Decide whether deployment paths should remain hardcoded or be generated from one `REPO_ROOT`.
- Make the installer include all active hosting wrappers:
  - `cron-pipeline.sh`
  - `cron-statcommentary.sh`
  - `cron-vibesynth.sh`
  - existing seeder, backup, restore, and tier scripts
- Align repository service restart policy with the proven live policy.
- In production, either:
  - fail API startup when Postgres is unavailable; or
  - retain degraded startup but make readiness fail.
- Point Railway readiness to `/health/db`, or make `/health` include database readiness.
- Add `SIGTERM` to graceful shutdown handling.
- Create one release command/script that:
  - builds `scoracle-api`
  - builds `pipeline`
  - builds `statcommentary`
  - builds `vibesynth`
  - stamps or reports the Git commit
  - installs/restarts the correct units
  - verifies health after restart
- Ensure a partial build cannot leave sibling cron binaries on different commits.

## Keep it simple

Do not introduce a deployment platform solely to solve this. A checked-in release script plus corrected systemd/cron templates is sufficient.

## Verification

- Install repository units into a temporary location and inspect rendered paths.
- Stop Postgres or use an invalid test URL; readiness must fail.
- Send `SIGTERM`; confirm graceful shutdown.
- Build all binaries from one commit and verify their hashes/timestamps.
- Restart the API and verify `/health/db` plus representative data endpoints.

## Done when

A clean machine can reproduce the currently intended backend deployment without manual path surgery.

---

# Session 3 — Add live NBA and NFL ingestion

## Problem

NBA and NFL fixture processing are intentionally absent from cron while waiting for BallDontLie webhooks, but no webhook ingestion exists. Live NBA and NFL data will become stale without manual intervention.

Football jobs also hardcode season `2025`, creating a rollover hazard.

## Work

- Choose polling as the initial durable mechanism.
- Add NBA and NFL fixture refresh jobs.
- Add NBA and NFL event-processing jobs at a cadence appropriate to provider finalization.
- Avoid processing every historical fixture on every tick.
- Add date/window options if needed so schedule refreshes stay bounded.
- Replace hardcoded football season values with:
  - a database lookup from `sports.current_season`; or
  - a wrapper that resolves the active season before invoking the CLI.
- Document expected provider-call volume and rate-limit behavior.
- Keep future webhook support optional rather than making polling depend on it.

## Suggested initial cadence

- Fixture/schedule refresh: daily, plus a tighter game-day window if necessary.
- Completed-event drain: every 15–30 minutes during active seasons.
- Football may remain less frequent if SportMonks final data is known to lag.

Validate these intervals against actual provider quotas before installation.

## Verification

- Load a bounded date range for NBA and NFL.
- Process one known completed fixture per sport.
- Re-run the same jobs and confirm idempotency.
- Confirm current-season ratings and endpoint timestamps advance.
- Confirm a provider rate limit stops the run without exhausting fixture retries.

## Done when

All advertised sports update without a human manually invoking the seeder.

---

# Session 4 — Enforce fixture finality and completeness

## Problems

- Loaded fixtures use `seed_delay_hours=0`.
- Pending selection is based on start time and local status, not authoritative provider-final state.
- A fixture is accepted when either player rows or team rows are present.
- Partial data can delete existing rows, replace them with an incomplete response, and mark the fixture seeded.

## Work

- Define an explicit completeness contract per sport.
- Capture provider status/finality when loading or fetching a fixture.
- Require provider final/completed status before finalization.
- Require both expected teams in `event_team_stats`.
- Require final scores for completed fixtures.
- Require a meaningful player-row count where player box scores are expected.
- Add sport-specific minimums only when they represent genuine provider guarantees.
- Reject suspiciously smaller replacement payloads unless explicitly forced.
- Use non-zero delay defaults appropriate to each provider.
- Consider a `--force` repair path for legitimate exceptional fixtures.
- Record the reason a fixture is considered incomplete separately from transport failures.

## Suggested acceptance predicate

```text
provider_final
AND expected_home_team_present
AND expected_away_team_present
AND both_scores_present
AND player_rows_meet_sport_expectation
```

## Verification

- Feed an empty response: transaction rolls back and fixture remains pending/retryable.
- Feed team-only data: fixture is not finalized.
- Feed one-team data: fixture is not finalized.
- Feed complete data: fixture finalizes and aggregates update.
- Re-seed a complete fixture: replacement is atomic.

## Done when

`status='seeded'` means “complete enough to serve,” not merely “the provider returned something.”

---

# Session 5 — Repair BDL exception and rate-limit propagation

## Problems

- `BDLClient` raises `RateLimitExhausted` correctly.
- NBA/NFL handlers catch broad `Exception` and return empty results.
- The outer process therefore treats rate limits as fixture failures and increments `seed_attempts`.
- Schedule loading can swallow all endpoint errors and report zero fixtures.
- Score fallback uses truthiness, so a legitimate zero can be replaced by `None`.

## Work

- Add `except RateLimitExhausted: raise` before broad handlers.
- Narrow exception handling where possible.
- When fallback API paths all fail, raise the last error instead of returning an empty schedule.
- Distinguish:
  - provider returned no data;
  - provider returned a protocol error;
  - transport failed;
  - rate limit was reached.
- Replace score `or` fallback with explicit `is not None` logic.
- Add tests for 429 behavior and zero scores.

## Verification

- Simulated 429 exits the whole process with resume state preserved.
- `seed_attempts` does not change on rate-limit pause.
- Simulated HTTP 500 is reported clearly.
- Empty successful provider response is distinguishable from a failed request.
- Zero scores remain zero.

## Done when

Provider failures retain their real meaning all the way to the operator.

---

# Session 6 — Make deferred season recomputation durable

## Problem

Historical fixtures can be marked seeded while the required `(sport, season)` recomputation exists only in an in-memory Python set. If the process exits before the final recompute, no durable record says the season remains dirty.

## Work

- Add a small table, for example:

```sql
CREATE TABLE season_recompute_needed (
    sport       text NOT NULL,
    season      integer NOT NULL,
    requested_at timestamptz NOT NULL DEFAULT now(),
    last_error  text,
    attempts    integer NOT NULL DEFAULT 0,
    PRIMARY KEY (sport, season)
);
```

- Upsert the dirty record in the same transaction that finalizes a deferred fixture.
- Run `recompute_season` from durable dirty rows.
- Delete the row only after:
  - recomputation succeeds; and
  - rating-history snapshot succeeds.
- Make `event recompute` clear the corresponding dirty row.
- Add a command to drain all dirty seasons.

## Verification

- Finalize a historical fixture in deferred mode.
- Kill the process before end-of-run recomputation.
- Confirm the dirty season remains visible.
- Run the drain command and confirm it clears only after success.

## Done when

Process death cannot strand a seeded-but-unrecomputed season invisibly.

---

# Session 7 — Introduce durable news-pipeline work state

## Problems

- The news pipeline uses an in-process `runStart` watermark.
- Existing links are inserted with `ON CONFLICT DO NOTHING`, so a new run may not rediscover work.
- A crash can lose the set of affected entities.
- Time-only debounce can skip changed inputs.

## Work

- Add a minimal durable work table. Prefer one generic table over several specialized queues:

```sql
CREATE TABLE pipeline_work (
    stage        text NOT NULL,
    entity_type  text NOT NULL,
    entity_id    integer NOT NULL,
    sport        text NOT NULL,
    status       text NOT NULL DEFAULT 'pending',
    attempts     integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    last_error   text,
    input_version text,
    PRIMARY KEY (stage, entity_type, entity_id, sport)
);
```

- Keep statuses minimal: `pending`, `running`, `failed`.
- Use `FOR UPDATE SKIP LOCKED` to claim work.
- Requeue stale `running` rows after a timeout.
- Upsert affected entities when:
  - an article/entity link is inserted;
  - a link becomes vetted;
  - a transfer verdict changes;
  - a Rating generation changes;
  - a Vibe generation changes;
  - either trajectory changes and therefore changes Momentum;
  - any Rating, Vibe, or Momentum input changes and therefore requires Sigil convergence.
- Store an input hash/version where useful instead of relying only on elapsed time.

## Keep it simple

Do not add Redis, Kafka, or a new queue service. PostgreSQL already owns the relevant state and is sufficient for this workload.

## Verification

- Insert duplicate work: one row remains.
- Crash after claiming: stale work becomes retryable.
- Two workers claim distinct rows.
- A changed input reopens previously completed work.

## Done when

The database can answer: “What backend derivation work is pending, running, or failed?”

---

# Session 8 — Make compile → scrub → derive → reveal an ordered pipeline

## Problems

- Daily RSS compilation is followed immediately by derivation.
- Scrubbing is asynchronous and may not have processed the new links.
- Fresh content often cannot influence the same pipeline run.
- The pipeline comments describe a stronger ordering guarantee than the code provides.

## Dependency

Complete Session 7 first.

## Work

- Have RSS persistence return or record inserted/affected article IDs.
- Enqueue those articles for scrub.
- Run the scrub stage as part of the daily pipeline for that exact batch.
- After an article is fully scrubbed, enqueue affected entities for:
  - transfer analysis;
  - narratives;
  - Vibe.
- Process those stages in declared order.
- When a Vibe generation changes, enqueue trajectory/Momentum recomputation.
- When Rating or Vibe changes, recompute the affected trajectory pair and append a Momentum
  generation when its input version changes.
- Enqueue Sigil convergence only after the current Rating, Vibe, and Momentum inputs are available.
- Generate Sigil from those three versioned inputs, never directly from “news pipeline complete.”
- Retain the maintenance scrub ticker only for old backlog and failed-item repair.
- Remove `runStart` as the correctness boundary.
- Make stage transitions explicit in durable work state.
- Prefer input-hash checks over “generated within N hours.”

## Target flow

```text
RSS fetch
  → transactional article/link persistence
  → scrub article
  → enqueue vetted affected entities
  → transfers
  → narratives
  → Vibe
  → recompute Rating/Vibe trajectories
  → Momentum
  → converge Rating + Vibe + Momentum
  → Sigil
  → reveal through product endpoints
```

## Verification

- Insert a brand-new article and run the pipeline once.
- Confirm it is scrubbed and reflected in derived outputs in that same run.
- Kill the pipeline after scrub, restart it, and confirm derivation resumes.
- Re-run unchanged input and confirm expensive Gemma work is skipped.

## Done when

One successful pipeline run means all accepted new inputs have reached their intended derived products.

---

# Session 9 — Repair real-time trigger semantics

> **✅ COMPLETE — deployed live 2026-06-22 (archbox).** Code `cc23b68`; F-016 release fix `795a9dd`.
> Migration **103** `enqueue_derive_on_vetted` applied (per-file). The `vetted=TRUE` transition now
> ENQUEUES `pipeline_work` and `pg_notify`s only as a wake-up; the in-API `internal/derive` worker
> drains the queue on wake / startup / a safety-net timeout; the two in-process LISTEN workers
> (news-volume + transfer) were deleted. Verified end-to-end live (all four stages drained; rejected
> and stale links enqueue nothing; a burst collapses to ≤1 in-flight per stage). Progress doc:
> `progress_docs/2026-06-22_first-gpt-audit-session-9-realtime-trigger-semantics.md`. Findings surfaced:
> **F-015** live schema drift / migration-ledger ≠ live schema (→ S15/17), **F-016** path-watcher
> release flap (resolved), **F-017** `composite_shift`→sigil still inline (→ S12), **F-018**
> restart-mid-drain strands the worker's lease (→ S13/14), **F-019** narrator empty-array parsed as a
> failure → dead-letters (→ S10/11).

## Problems

- News-volume notifications fire on raw link insert.
- Scrub updates do not retrigger insert-only notifications.
- `LISTEN/NOTIFY` is transient and cannot itself guarantee processing.
- The news-volume worker can run before any links are vetted.
- Real-time narrative/Vibe work has no concurrency governor equivalent to the transfer worker.

## Dependency

Sessions 7 and 8 should define the work model first.

## Work

- Change triggers so they enqueue durable work rather than directly representing completed eligibility.
- Trigger derivation from a transition to `vetted=TRUE`, or from article-level “scrub complete.”
- Use `NOTIFY` only as a low-latency wake-up signal.
- On wake-up, drain durable pending work.
- Ensure startup drains pending work even if a notification was missed.
- Add bounded concurrency for news-volume Gemma work.
- Add per-entity in-flight protection.
- Ensure multi-replica API deployment cannot duplicate the same work.

## Verification

- Stop the listener, insert and scrub qualifying articles, restart listener: work still runs.
- Insert raw links that are later rejected: no narrative/Vibe work runs for rejected entities.
- Generate a burst for one entity: at most one in-flight job exists.

## Done when

Notifications improve latency but are never required for correctness.

---

# Session 10 — Make transfer validation fail closed

> **✅ COMPLETE — deployed live 2026-06-22 (archbox).** Code `1486b7b`; migration **104**
> `104_transfer_fail_closed` applied (per-file) + recorded. A Gemma timeout, unparseable output, or a
> verdict with no `is_rumor` field now persists `is_rumor=NULL` (UNKNOWN) instead of the old provisional
> `is_rumor=TRUE` — UNKNOWN is never served (every read requires `is_rumor IS TRUE`) and is re-enqueued
> through the existing `pipeline_work(transfers)` stage (`drainTransfers` fails the item on `res.Unknown>0`
> ⇒ queue backoff retry; no new mechanism). `is_rumor IS TRUE` now gates narrative/Vibe heat grounding
> (`loadTransferHeat`) and `pipeline_stats.transfer_rumors_active` (the `/transfers` read contract already
> had it). `compute_transfer_heat` now requires BOTH links `vetted IS TRUE` (dropped the unscrubbed-link
> allowance — empirically 56/1950 active pairs lose heat that was unvetted, monotonic, 0 gained). Removed
> simplification-C tweet vestigials: dropped the unused fail-OPEN `seed_transfer_rumors`, the
> `compute_transfer_heat` `tweet_ids` OUT param, and `input_tweet_ids` on `transfer_rumors`+`vibe_scores`
> (+ `loadPairTweets`/`tweetItem`/tweet consts in Go). Verified live: new binary writes only model-stamped
> TRUE/FALSE (0 fail-open), `/transfers` + leaderboard 200, a newer FALSE/NULL supersedes an older TRUE in
> grounding. Deploy order INVERTED (F-022): released the new binary first (tolerates both schemas), then
> migrated. Progress doc: `progress_docs/2026-06-22_first-gpt-audit-session-10-transfer-fail-closed.md`.
> Findings surfaced: **F-020** (historical fail-open rows left append-only; 3 teams re-vet-enqueued →
> launch gate), **F-021** (team-grained retry re-runs the whole team → optimization), **F-022** (drop-column
> deploy order). **F-019** confirmed still open (narratives empty-array → Session 11; the only dead-letters).

## Problems

- Gemma timeout or parse failure writes `is_rumor=TRUE`.
- The News Transfers-scope read path interprets `TRUE` as vetted.
- Internal transfer heat used by narratives and Vibe does not filter `is_rumor`.
- Cleared rumors can continue influencing downstream prose and scores.
- `compute_transfer_heat` still permits unscrubbed links.
- Vestigial tweet fields and loaders remain after Twitter table removal.

## Work

- Define transfer verdict states clearly:
  - `TRUE`: Gemma-vetted rumor.
  - `FALSE`: Gemma-cleared.
  - `NULL`: unknown/unprocessed/model failure.
- On Gemma error or parse failure, persist `NULL`, not `TRUE`.
- Retry unknown rows through durable work.
- Require `is_rumor IS TRUE` in:
  - the `/transfers` contract used by the News Transfers scope;
  - narrative grounding;
  - Vibe inputs;
  - pipeline statistics.
- Require both article/entity links to be `vetted IS TRUE` in `compute_transfer_heat`.
- Remove remaining tweet loaders, constants, output parameters, and columns if no compatibility consumer remains.
- Consider distinguishing deterministic heat from validated rumor status in naming and API contracts.

## Verification

- Force Ollama timeout: no public rumor appears.
- Force invalid JSON: row remains unknown and retryable.
- Write a newer `is_rumor=FALSE` row: older true result no longer influences narratives or Vibe.
- Unscrubbed links do not contribute to heat.

## Done when

Only a successful positive Gemma verdict can become a served or downstream-consumed rumor.

---

# Session 11 — Standardize append-only marker semantics

> **✅ COMPLETE — deployed live 2026-06-23 (archbox).** Code `fcff1d9`. **Read-path + narrator only —
> NO migration** (next free number stays 105). One canonical latest-generation rule now governs every
> product read: resolve the entity's latest generation REGARDLESS of nullability, then return its content
> (or empty/null if that generation is a marker) — killing the "filter nulls before picking the latest"
> bug where a newer no-data marker failed to clear stale content. Fixed in `go/internal/db/db.go`:
> **`entity_news`** (inner `max(generated_at)` was body-filtered → unfiltered, matching
> `ml/vibe.go loadLatestNarratives`), **`entity_vibes`** (the per-entity `/sigil` read — `vibe_cur` now
> takes the latest synthesis then drops it if marker/stale; `vibe_hist` sparkline unchanged — markers
> change current, never history), **`/rating` commentary** (latest `stat_summaries` gen within the season
> scope, then body-gate), **`narratives_leaderboard`** (resolve each entity's latest gen via a `latest_gen`
> CTE, then keep only its content → a newer marker drops the entity off the board), **`sigil_leaderboard`**
> and **`vibes_leaderboard`** (`latest_raw` DISTINCT-ON unfiltered → `latest` filters markers). Prior
> generations are untouched (append-only); a marker only changes the current projection.
> **F-019** fixed in `go/internal/ml/news_narratives.go`: `parseNarratives` now reports *parseability*, so a
> cleanly-closed (even empty) `{"narratives": []}` is a successful no-data outcome → existing no-corpus
> marker path → the queue item Completes; only a genuinely malformed/truncated response still errors
> (retry) — `generation_failed` never masquerades as no-data. Locked by `news_narratives_test.go`. All
> prepared statements re-validated against the LIVE schema (a throwaway `db.New` boot) before the restart;
> all 11 dead-lettered/failed `{"narratives": []}` rows requeued post-deploy and Completed as markers.
> Deploy: `release.sh` (PID-specific rebuild + restart, masks the `scoracle-api.path` watcher — F-016),
> then requeued stranded `running` rows (F-018). Progress doc:
> `progress_docs/2026-06-23_first-gpt-audit-session-11-marker-semantics.md`. Findings: **F-019** RESOLVED;
> **F-023** (Sigil generation-side pillar/debounce loaders still use latest-non-marker → Session 12),
> **F-024** (explicit `marker_reason` column deliberately deferred).

## Problem

Generators write marker rows for “no narratives,” “no stats,” and “no pillars,” but several read paths select the latest non-null successful row. A newer marker therefore fails to clear old content.

## Work

- Define one canonical latest-generation rule:
  1. Find the latest generation regardless of nullability.
  2. If it is a marker, return empty/current-null.
  3. Otherwise return rows from that generation.
- Apply it to:
  - `/news`
  - news leaderboard
  - `/rating` commentary
  - `/sigil`
  - Sigil leaderboard
  - any Vibe read where a null marker should clear prior data
- Decide whether markers expire old history or only current state.
- Preserve all prior generations permanently. A marker changes only the current projection; it does
  not erase, supersede historically, or make prior derivations unavailable to archive/time-series
  queries.
- Add an explicit marker reason if useful:
  - `no_corpus`
  - `no_stats`
  - `no_pillars`
  - `generation_failed` should not masquerade as no data
- Avoid filtering null rows before selecting the latest generation.

## Suggested SQL pattern

```sql
WITH latest_generation AS (
    SELECT max(generated_at) AS generated_at
    FROM product_table
    WHERE entity keys...
),
current_rows AS (
    SELECT ...
    FROM product_table
    WHERE entity keys...
      AND generated_at = (SELECT generated_at FROM latest_generation)
)
SELECT ...
```

## Verification

- Create a successful generation.
- Append a marker.
- Confirm current endpoint becomes empty/null.
- Append a later successful generation.
- Confirm content reappears.
- Confirm leaderboards follow the same truth.

## Done when

All endpoint products agree on what the latest generation means.

---

# Session 12 — Repair convergence and the event-driven Sigil lifecycle

> **✅ COMPLETE — deployed live 2026-06-23 (archbox).** Code `331f76706f68`. **Code-only — NO migration**
> (next free stays **106**; the parallel session took 105 — F-031). Sigil convergence is now event-driven,
> debounced by the three-pillar input hash, and **season-correct (historical supported — Scott's call,
> F-026):** `/sigil` + `/leaderboard/sigil` take an optional `?season` (no param ⇒ the live view = current
> season + legacy NULL rows, so an older season can never become the current crown; `?season=N` ⇒ that
> season exactly). Every generation STAMPS a concrete season (`resolveSeason`: nil ⇒ `sports.current_season`),
> so the real-time queue is current-season and only explicit-season **backfill** writes history.
> **F-017** fixed: the percentile listener ENQUEUES durable `pipeline_work(sigil)` on a composite shift,
> **before** the follower early-return — zero-follower entities still converge (simplification A); inline
> Gemma off the NOTIFY + `RecentlySynthesized` removed. **F-023** fixed: the generation-side pillar/debounce
> loaders (`sigil.go` `loadRatingPillar`/`lastSynthesisHash`/`lastScore`, `rating.go`
> `lastCommentaryHash`/`ReStampPeakKeys`) now apply the S11 canonical latest-generation rule + season scope.
> Real **`DryRun`** added to `SigilRequest` (a single dry-run no longer persists). **`vibesynth -mode nightly`**
> converted to bounded reconciliation (enumerate current-season missing/stale → enqueue `pipeline_work(sigil)`;
> no inline synth, no Ollama, no scheduled duplicates); `backfill` is per-season + only-missing; cron docs
> rewritten, nightly line kept (F-002). All prepared statements re-validated against the live schema (F-025);
> deployed via `release.sh` (F-016); 11 restart-stranded `vibe` rows requeued (F-018). Progress doc:
> `progress_docs/2026-06-23_first-gpt-audit-session-12-convergence-sigil-lifecycle.md`. Findings:
> **F-002/F-010/F-017/F-023** RESOLVED, **F-011** clarified; **F-026** (season decision), **F-027** (72h-window
> follow-up), **F-028** (NULL-season transition), **F-029** (historical news/vibe not season-scoped), **F-030**
> (NFL/FOOTBALL current-season coverage gap → launch-gate), **F-031** (migration 105 taken) added.

## Problems

- Composite-shift synthesis occurs after follower and FCM-specific early returns.
- Entities without followers do not receive this generation path.
- `vibesynth -mode nightly` enumerates every season despite claiming current-season behavior.
- `/sigil` ignores season and serves the latest generated scored row.
- Processing older seasons after current seasons can make old output current.
- Sigil input hash, previous score, and recent debounce are not season-scoped.
- Single-mode “dry run” still persists.
- Existing nightly behavior is being treated as a generation path even though the product model
  requires event-driven, debounced convergence.
- Current orchestration does not make Rating + Vibe + Momentum explicit, versioned prerequisites.

## Work

- Move Sigil generation outside notification/follower logic.
- Treat notification delivery and product generation as separate concerns.
- Define one convergence input record/hash from the current season-aware Rating, Vibe, and Momentum
  generations.
- Trigger convergence when any of those three inputs changes.
- Debounce duplicate events by convergence input hash, not by elapsed time alone.
- Require all three valid inputs before producing a scored Sigil; append a truthful marker when the
  product contract calls for a current no-data state.
- Decide product semantics:
  - If Sigil is current-season only, enforce that everywhere.
  - If historical Sigils are supported, require season in endpoint selection and hashes.
- Scope:
  - last input hash;
  - previous score;
  - recent-debounce checks;
  - endpoint current selection
  by season.
- Add a real `DryRun` field to `SigilRequest`, matching stat commentary.
- Ensure marker and successful Sigil reads obey Session 11.
- Convert nightly execution into a bounded reconciliation/backfill job only:
  - enumerate current-season entities;
  - detect missing or stale convergence hashes;
  - enqueue the same event-driven convergence work;
  - do not synthesize an unchanged Sigil merely because a schedule fired.
- Reconcile current-season coverage before launch.

## Verification

- Entity with zero followers receives Sigil generation.
- FCM disabled does not disable Sigil.
- Rating, Vibe, or Momentum input change enqueues one debounced convergence generation.
- Unchanged convergence inputs do not append a scheduled duplicate.
- Reconciliation targets only current-season missing/stale rows.
- Historical synthesis cannot replace current-season endpoint output.
- Single dry-run produces no database write.
- Current-season rated entities reach an agreed coverage threshold.

## Done when

Sigil availability depends on current Rating, Vibe, and Momentum inputs—not followers, push
configuration, or a nightly generation schedule.

---

# Session 13 — Make jobs observable, non-overlapping, and correctly failing

> **✅ COMPLETE — deployed live 2026-06-23 (archbox).** Code `c35e1ba`. Migration **106**
> `106_pipeline_runs` applied per-file (F-006; next free = 107). Batch jobs are now observable,
> non-overlapping, and correctly failing: an operator can tell whether last night's work completed from
> `SELECT * FROM pipeline_runs_latest` instead of grepping logs. **`pipeline_runs`** (additive) records
> one row per `cmd/pipeline|statcommentary|vibesynth` run (commit, outcome, attempted/succeeded/skipped/
> failed counts, summarized error). **`internal/jobrun`** adds the per-job advisory lock
> (`pg_try_advisory_lock(hashtext('scoracle.job.'+job))`) — F-012 RESOLVED: a second run (or a manual run
> racing the cron) records a `skipped` row and exits 0; the in-API worker deliberately does NOT take it
> (SKIP LOCKED already disjoints cron-vs-worker). **Exit codes:** `0` success/overlap-skip · `3` partial
> (retryable item failures) · `1` enumeration/whole-stage failure OR dead-lettered work remains (F-033).
> **F-018 RESOLVED:** the derive drain settles its leased rows on a context detached from the drain — a
> graceful shutdown hands the leased-but-unprocessed batch back to `pending` (new `work.Requeue`) instead
> of stranding it `running` for the 30m stale lease; `cmd/api` waits (≤8s) for the worker to settle before
> the pool closes. **Dead-letter report:** `go run ./cmd/work dead-letters` lists `pipeline_work` rows
> parked past the retry cap AND fixtures at `seed_attempts >= cap` — it immediately surfaced 2 pre-fix
> empty-array narratives stragglers (F-032). Verified: build/vet/gofmt clean; `work` integration tests
> (incl. `Requeue`/`DeadLetters`) + advisory-lock cross-session exclusion + `pipeline_runs` round-trip pass
> on a throwaway PG; F-025 prepared-statement boot OK on live; deployed via `release.sh` (F-016); 1 orphan
> requeued post-deploy (F-018, the last time — the fix is now live). Progress doc:
> `progress_docs/2026-06-23_first-gpt-audit-session-13-observable-jobs.md`. Findings: **F-012**, **F-018**
> RESOLVED; **F-031** updated (next free = 107); **F-032** (pre-fix narratives dead-letters → operator
> requeue), **F-033** (pipeline exit-1 keys off global dead-letter state — by design) added.

## Problems

- Pipeline, stat-commentary, and Sigil jobs count failures but frequently exit zero.
- Cron cannot distinguish partial failure from success.
- There is no advisory lock preventing overlap.
- There is no durable pipeline-run record.
- Retry-exhausted fixtures have no explicit dead-letter report.

## Work

- Add one `pipeline_runs` table or structured run log with:
  - job name;
  - start/end;
  - source commit;
  - success/failure;
  - attempted/succeeded/skipped/failed counts;
  - summarized error.
- Use a PostgreSQL advisory lock per job.
- Return non-zero when:
  - enumeration fails;
  - a required stage fails entirely;
  - failures exceed an agreed threshold;
  - work remains failed after retries.
- Decide whether individual entity failures should produce exit 1 or a distinct partial-success code.
- Add a report/query for fixtures at the retry cap.
- Add log alerts or a simple daily operator query; avoid adding a complex observability stack unless needed.

## Verification

- Start two pipeline instances: second exits without overlapping.
- Force one stage-wide failure: cron-visible non-zero exit.
- Force one entity failure: run record accurately shows partial outcome.
- Query retry-exhausted fixtures and failed pipeline work.

## Done when

An operator can tell whether last night’s backend work actually completed without reading thousands of log lines.

---

# Session 14 — Harden Ollama/Gemma 4 lifecycle and capacity

> **✅ COMPLETE — deployed live 2026-06-24 (archbox).** Code `cf4f26069df6`. **Code + `.env.local`
> only — NO migration** (next free stays **107**). Ollama downtime now delays enrichment without
> losing work or changing truth semantics. **F-014 RESOLVED:** `cmd/api` builds the Gemma generators
> UNCONDITIONALLY and always starts the derive worker (gated only on `DERIVE_WORKER_ENABLED`) — the
> one-time boot ping is gone. `derive.DrainAll` reachability-PRE-GATES each cycle and DEFERS when
> Ollama is down (`Result.Deferred`; claims nothing, burns no retries); a mid-drain connection error
> requeues the leased batch via the new `ml.IsUnavailable` classifier (no attempt burned). Pending
> `pipeline_work` drains on the next cycle once Ollama returns — **no API restart**. The maintenance
> scrub ticker got the same pre-gate (cheap SQL auto-vet still runs; Gemma phase skipped while down);
> `cmd/pipeline`'s boot ping is now NON-FATAL (sweep keeps ingesting raw; run records `partial`, not
> `exit 1`). **Shared GPU governor:** a process-wide semaphore in `internal/ml`
> (`SetGemmaConcurrency`, default 1, `OLLAMA_MAX_CONCURRENT`) acquired around every `Generate` —
> derive worker + maintenance scrub + cron Gemma serialize on the single 8GB card. **Operation-specific
> timeouts:** `OLLAMA_TIMEOUT_SECONDS` is now the LONG-op budget (narratives, NumPredict 4000) + HTTP
> backstop; new `OLLAMA_SHORT_TIMEOUT_SECONDS` (120s) bounds scrub/vibe/sigil/transfer; `keep_alive`
> (`OLLAMA_KEEP_ALIVE`, 30m) keeps gemma4:e4b resident (measured warm `load_ms ≈ 350`, vs the 100s+
> cold load that blew the old flat 600s stopgap). **Metrics:** the client logs one timed line per call
> (`op`, `wall_ms`, `eval_count`, outcome). Verified: build/vet/gofmt/test clean; new tests for the
> classifier, the gate, and DrainAll-defers; live boot loaded the new config with no degraded mode,
> F-018 settle confirmed on the old shutdown (`requeued=7`), serving stayed responsive (`/health/db`
> 0.6ms) under heavy drain, zero dead-letters. Deployed via `release.sh` (F-016). Progress doc:
> `progress_docs/2026-06-24_first-gpt-audit-session-14-ollama-lifecycle-capacity.md`. Findings:
> **F-014** RESOLVED; **F-034** (simplification A deferred — F-014 removed its main motivation),
> **F-035** (explicit cross-process governor `OLLAMA_NUM_PARALLEL=1` on the ollama service — ops
> follow-up), **F-036** (durable per-call Gemma metric deferred — log-only for now), **F-037**
> (transfers per-pair timeout still team-scoped → pairs with F-021).

## Problems

- API startup decides whether Gemma-backed workers exist.
- If Ollama is down at startup, workers remain disabled until API restart.
- Default generation timeout is 60 seconds, while narrative generation may exceed it.
- The API, maintenance scrub, real-time workers, and cron jobs share one GPU.
- News-volume generation lacks the transfer worker’s concurrency protection.

## Work

- Separate worker readiness from a one-time API boot ping.
- Prefer workers that attempt work and record retryable failure when Ollama is unavailable.
- Add a shared concurrency governor for all Gemma work on the machine.
- Confirm the intended Ollama timeout from production measurements.
- Use operation-specific timeouts if narrative generation genuinely needs longer than scrub or Vibe.
- Add per-call timing and timeout metrics to run records.
- Define behavior during Ollama outage:
  - raw data continues ingesting;
  - durable work accumulates;
  - no unverified output is published;
  - work drains after recovery.
- Consider separating workers from the API process so API restarts do not govern ML availability.

## Verification

- Start API while Ollama is down, then restore Ollama without restarting API; pending work eventually runs.
- Saturate the queue; concurrent model calls stay within the configured limit.
- Confirm request serving remains responsive during heavy generation.

## Done when

Ollama downtime delays enrichment but neither loses work nor changes truth semantics.

---

# Session 15 — Harden backups, restores, and migrations

## Problems

- Restore drill ignores `pg_restore` failure with `|| true`.
- The drill checks the dropped `tweets` table.
- Two failed queries returning `n/a` can be labeled `ok`.
- Default backups are on the same physical storage as Postgres.
- Migration application and migration-ledger recording are separate transactions/processes.
- A fresh environment depends on cloning a healthy existing database.

## Work

- Remove `|| true` from the restore operation.
- Fail the drill on any missing critical table or failed count query.
- Replace stale table checks with current critical tables.
- Compare:
  - schema migration versions;
  - table existence;
  - row counts;
  - selected constraints/indexes/functions.
- Verify restored API prepared statements can register against the restored database.
- Add an off-host or off-disk backup destination.
- Test restoration from that independent copy.
- Make migration recording atomic where practical:
  - include the ledger insert inside each migration transaction; or
  - use a runner that applies migration SQL and ledger update in one transaction.
- Generate and version a current schema snapshot so the repository has a recovery path independent of production.

## Verification

- Corrupt a test dump: restore drill fails.
- Remove a critical table: restore drill fails.
- Restore a valid dump: API can connect and prepare statements.
- Confirm an off-host copy exists and is decryptable/readable.
- Interrupt a test migration between SQL and recording; state remains unambiguous.

## Done when

“We have backups” has been replaced by “we can restore a backend that boots.”

---

# Session 16 — Add focused tests and CI

## Problems

Current automated coverage is sparse around the highest-risk state transitions. There is no committed CI workflow.

## Work

Add tests around behavior, not implementation trivia.

### Python seeder tests

- Rate-limit propagation.
- Zero-score preservation.
- Empty and partial fixture rejection.
- Atomic replacement behavior.
- Deferred recompute dirty-state behavior.
- Retry-cap behavior.

### Go pipeline tests

- RSS persistence error propagation.
- Scrub verdict transactionality.
- Durable work claiming/retry.
- Marker generation semantics.
- Transfer fail-closed behavior.
- Cleared rumors excluded from downstream heat.
- Sigil dry-run.
- Event-driven convergence from Rating + Vibe + Momentum.
- Current-season Sigil reconciliation without duplicate scheduled generations.
- Season-scoped hashes and previous scores.

### SQL/integration tests

- Latest marker clears old endpoint content.
- Prepared statements register against a migrated test database.
- Fixture finalization only marks complete data seeded.
- Work queue claiming is concurrency-safe.

### CI

- Python compile and tests.
- `go test ./...`
- `go vet ./...`
- shell syntax and ShellCheck.
- Docker build.
- Migration/static schema checks.

## Verification

- Reproduce each previously identified bug with a failing test before fixing it where practical.
- CI runs on every main-bound change.

## Done when

The launch-critical invariants are executable checks rather than comments and operator memory.

---

# Session 17 — Reconcile backend documentation and runbooks

## Problems

Backend documentation still describes removed routes and retired integrations. Operational instructions disagree about live paths and restart behavior.

## Work

- Update `CLAUDE.md` route conventions.
- Update `README.md` API surface and environment variables.
- Update `ENDPOINTS.md` date and current contracts.
- Remove references to:
  - bundled profile route;
  - retired live RSS routes;
  - Twitter routes/configuration;
  - `/special`;
  - `/trends`;
  - per-entity `/vibes` where `/rating`, `/momentum`, or `/sigil` are current.
- Document the actual compile → scrub → derive pipeline.
- Document durable work tables and repair commands.
- Document release, rollback, backup, and restore procedures.
- Document which jobs are cron-driven and which are event-driven.
- Document that Sigil is event-driven and debounced, with cron limited to reconciliation/backfill.
- Document Transfers as a News scope even if `/transfers` remains a supporting backend contract.
- Ensure comments in code use current Rating/Vibe/Sigil vocabulary.

## Verification

- Compare every documented route to router registration.
- Compare every documented cron job to installed crontab.
- Compare every documented binary to the release script.
- Follow the runbook on a clean/test environment.

## Done when

The repository documentation can be trusted during an incident or machine rebuild.

---

# Additional simplification opportunities

These are not necessarily separate launch-blocking sessions, but they should be folded into the relevant work above.

## A. Move background derivation out of the API

The API currently owns:

- HTTP serving;
- notification dispatch;
- percentile listener;
- news-volume listener;
- transfer listener;
- maintenance;
- news scrub;
- Gemma client initialization.

The simpler long-term boundary is:

```text
API: read-only serving
Worker: durable background work
Seeder: provider ingestion
```

This avoids API restarts controlling ML availability and reduces production blast radius.

## B. Remove time-only debounce where inputs can be hashed

“Generated within ten hours” does not mean “generated from current inputs.” Prefer input hashes for:

- narratives;
- Vibe;
- Sigil;
- stat commentary;
- transfer verdicts where feasible.

Time windows can remain as load governors, but should not be the sole correctness gate.

## C. Remove vestigial Twitter compatibility

Once confirmed unused, remove:

- tweet ID output from `compute_transfer_heat`;
- tweet columns in transfer and Vibe products;
- `loadPairTweets`;
- tweet constants and logging;
- obsolete comments and documentation.

This reduces mental overhead in one of the most correctness-sensitive flows.

## D. Standardize generation tables

The append-only product tables share common concepts:

- entity key;
- generation time;
- trigger;
- marker/no-data result;
- model and prompt versions;
- input provenance;
- input hash.

Adopt consistent semantics and query patterns even if the physical tables remain separate.

---

# Final launch gate

Before public launch, run one deliberate end-to-end proof for every sport.

## Stats proof

1. Load a fixture.
2. Confirm it is not eligible before finality.
3. Process complete final box scores.
4. Confirm event rows, season aggregates, ratings, and commentary.
5. Confirm `/stats` and `/rating`.
6. Re-run and verify idempotency.

## News proof

1. Compile a new RSS article.
2. Confirm exact links are scrubbed.
3. Confirm rejected links do not reach consumers.
4. Confirm accepted links enqueue durable work.
5. Confirm transfer verdict, narratives, and Vibe.
6. Confirm `/news` and its Transfers scope.
7. Append a no-data marker and confirm old current content clears.
8. Simulate process death between stages and confirm recovery.

## Convergence proof

1. Change a current Rating or Vibe input.
2. Confirm both trajectories are recomputed into Momentum.
3. Confirm Rating + Vibe + Momentum enqueue one debounced Sigil convergence.
4. Confirm `/momentum` exposes both trajectories and `/sigil` exposes the holistic synthesis.
5. Re-run reconciliation with unchanged inputs and confirm no duplicate Sigil is appended.
6. Confirm prior Momentum and Sigil generations remain available as append-only history.

## Operations proof

1. Deploy all binaries from one commit.
2. Verify database-aware readiness.
3. Restart Postgres and Ollama independently.
4. Confirm raw ingestion and durable work recover.
5. Restore the latest off-host backup into a throwaway database.
6. Boot the API against the restored database.
7. Confirm cron jobs return meaningful statuses.

## Launch decision

Launch only when:

- all three sports update automatically;
- no pipeline stage depends on an ephemeral notification for correctness;
- unverified content cannot be served as verified;
- marker rows clear stale current products;
- Momentum combines both rail trajectories;
- Sigil is season-correct, broadly populated, and generated from Rating + Vibe + Momentum;
- scheduled work only reconciles missing/stale Sigils and never creates unchanged duplicates;
- health checks reflect actual serving readiness;
- a verified off-host restore can boot the backend;
- the deployed binaries, schema, service files, cron, and documentation all describe the same system.
