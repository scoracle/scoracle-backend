# First GPT Audit — Session 4: Enforce fixture finality and completeness

**Worked:** 2026-06-21 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 4

**Depends on baseline:** Session 1 (`progress_docs/2026-06-21_first-gpt-audit-session-1-production-baseline.md`),
Session 3 seed-delay rails (`…-session-3-live-nba-nfl-ingestion.md`)

**Product authority:** wiki `Product Narrative`

## Goal

Make `status='seeded'` mean "complete enough to serve", not merely "the provider
returned something". Before this session a fixture finalized when *either* player
rows *or* team rows were present, the existing rows were deleted and replaced by
whatever the provider returned, and the fixture was marked seeded — with no
finality check and no atomicity against partial payloads.

## Decisions

1. **The completeness gate lives in the seeder, not SQL.** Accepting a provider
   payload is ingestion policy, distinct from the derived-stat/percentile logic
   CLAUDE.md keeps in Postgres. The contract is one pure function
   (`services/event/completeness.py`).
2. **Finality is tri-state and fail-safe.** A *recognised* not-final provider
   status is rejected; a recognised final status confirms completeness; an
   unknown/blank status defers to the structural checks (both teams + both final
   scores + min player rows), which an unfinished game cannot satisfy. This means
   a provider status string we haven't enumerated can never silently stall the
   live pipeline — the worst case degrades to structural-only completeness, which
   is still correct.
3. **Validate before mutating.** The gate runs *before* the
   `DELETE event_box_scores/event_team_stats`, so a rejected payload leaves the
   existing rows intact and the fixture pending/retryable. Nothing is ever
   half-seeded.
4. **Incompleteness is recorded separately from transport failures.** New
   `fixtures.last_incomplete_reason` column; `last_seed_error` stays reserved for
   transport/processing errors. Both are cleared by `mark_fixture_seeded()` on a
   successful finalize.
5. **A `--force` + `--fixture-id` repair path** handles legitimate exceptional
   fixtures without weakening the default gate.
6. Seed-delay defaults (the "non-zero delay" item) were already set in Session 3
   (NBA 4h / NFL 6h / FOOTBALL 3h); not re-touched here.

## Acceptance predicate (`evaluate_completeness`)

```
NOT provider_known_not_final
AND expected_home_team_present
AND expected_away_team_present
AND home_score_present AND away_score_present   # None = absent; a legit 0 (0-0) is present
AND len(player_rows) >= sport_min               # NBA/NFL/FOOTBALL = 10 (conservative anti-partial floor)
```

Per-sport finality from the raw provider status:
- **NBA / NFL (BallDontLie):** final iff status starts with `"Final"`; anything
  else (scheduled ISO datetime, in-progress period label) → unknown → defer.
  Captured from the embedded `game.status` already in the `/stats` payload — no
  extra provider call.
- **FOOTBALL (SportMonks):** final = `{FT, AET, FT_PEN}`; recognised not-final =
  `{NS, INPLAY_*, HT, POSTPONED, CANCELLED, ABANDONED, SUSPENDED, …}` → reject;
  anything else → defer. Captured by adding `state` to the existing fixture
  include and reading the state code.

## What changed

### New / changed Python

- **`seed/services/event/completeness.py`** (new) — `IncompleteFixtureError`,
  `is_final()`, `evaluate_completeness()`, per-sport `_MIN_PLAYER_ROWS`,
  SportMonks final/not-final state sets.
- **`seed/shared/models.py`** — new `BoxScoreResult(players, teams,
  provider_status)` container returned by handlers.
- **handlers** (`bdl_nba.py`, `bdl_nfl.py`, `sportmonks_football.py`) —
  `get_box_score` now returns `BoxScoreResult`, surfacing the raw provider
  finality label. Football adds `;state` to the fixture include +
  `_extract_fixture_state()`. Handlers stay thin: they surface the status, the
  gate interprets it.
- **`seed/services/event/cli.py`** —
  - `_seed_fixture_box_scores` runs the gate + a shrink-on-re-seed guard *before*
    any DELETE, and takes a `force` flag.
  - `process` gains `--fixture-id` (target one fixture, ignoring
    pending/delay/retry-cap filters — repair path) and `--force` (bypass the
    contract + shrink guard; an empty payload is still never seeded).
  - Incomplete fixtures are caught distinctly → `record_incomplete()`,
    counted as `incomplete=N` in the run summary, and left pending/retryable.
- **`seed/services/event/fixtures.py`** — `record_incomplete()` (increments
  `seed_attempts`, sets `last_incomplete_reason`, leaves `last_seed_error`
  untouched).

### SQL

- **`sql/migrations/100_fixture_completeness.sql`** (new) — adds
  `fixtures.last_incomplete_reason TEXT` + redefines `mark_fixture_seeded()` to
  clear both `last_seed_error` and `last_incomplete_reason` on a successful seed.
  Additive, idempotent, wrapped in BEGIN/COMMIT.
- **`sql/shared.sql`** — mirrored both changes (column on the `fixtures` DDL +
  the `mark_fixture_seeded` body) to keep the canonical base in sync and avoid
  the finalize-tail drift that bit migrations 049/050.

## Applied to production

Applied **only** migration 100 directly (the bulk `migrate.sh` would also apply
the untracked `099_team_rosters.sql` from a parallel session), recording the
ledger exactly as the runner does:

```
psql "$DB" -v ON_ERROR_STOP=1 -f sql/migrations/100_fixture_completeness.sql
psql "$DB" -v ON_ERROR_STOP=1 -qc "INSERT INTO public.schema_migrations(version)
    VALUES ('100_fixture_completeness') ON CONFLICT DO NOTHING;"
```

Pre-migration backup: the Session 1 verified dump
`/mnt/data/backup/scoracle/scoracle-20260621T191924Z.dump` (449 MiB, SHA-256
recorded) — taken before any audit migration.

Verified post-apply: column present (`text`, nullable); `mark_fixture_seeded`
body clears both error fields; ledger row `100_fixture_completeness` present.

**No API restart required** — the migration is additive (prepared statements
select explicit columns) and `mark_fixture_seeded` is seeder-side. The next
`event process` cron tick automatically picks up the new gate.

## Verification

- `pytest seed/tests/` — 40 passed. New `test_event_completeness.py` covers:
  - `is_final` tri-state across NBA/NFL/FOOTBALL (final / scheduled / in-progress
    / postponed / unrecognised / None);
  - `evaluate_completeness` accept-complete, reject not-final, reject missing
    team, reject missing score, **0-0 score is present**, reject too-few players,
    unknown-status-defers-to-structure;
  - the `_seed_fixture_box_scores` gate: empty rejected even with `--force`;
    incomplete rejected **before any DELETE** (atomicity) and without calling
    `finalize_fixture`; complete accepted (delete + finalize); shrinking re-seed
    rejected; `--force` bypasses completeness + shrink.
- `process --help` renders the new `--fixture-id` / `--force` options.
- Migration applied + verified against the live DB (see above).

## Maps to the audit's "Done when"

`status='seeded'` now means "complete enough to serve". An empty/partial/team-only
payload leaves the fixture pending and records a distinct incompleteness reason;
a recognised not-final game is rejected; a shrinking re-seed is rejected unless
forced; replacement is atomic.

## Not done here (deliberate, deferred to later sessions)

- **Dead-letter / retry-cap report** for fixtures that stay incomplete to the cap
  → Session 13 (`pipeline_runs` + retry-exhausted query). For now an incomplete
  fixture increments `seed_attempts` and is bounded by the existing cap.
- **Score `or`-truthiness fix** in the NBA/NFL handlers (a legitimate 0 could be
  dropped) → Session 5. Not a practical issue for NBA/NFL final scores, and the
  gate's score check is `is not None`, so it's orthogonal.
- Football `state` include should be confirmed against the live SportMonks plan
  during the final-gate football proof (standard include; degrades to
  structural-only if absent).

## Files changed

- `seed/services/event/completeness.py` (new)
- `seed/shared/models.py`
- `seed/services/event/handlers/bdl_nba.py`
- `seed/services/event/handlers/bdl_nfl.py`
- `seed/services/event/handlers/sportmonks_football.py`
- `seed/services/event/cli.py`
- `seed/services/event/fixtures.py`
- `sql/migrations/100_fixture_completeness.sql` (new, **applied to prod**)
- `sql/shared.sql`
- `seed/tests/test_event_completeness.py` (new)
- `progress_docs/2026-06-21_first-gpt-audit-session-4-fixture-finality.md` (this doc)
