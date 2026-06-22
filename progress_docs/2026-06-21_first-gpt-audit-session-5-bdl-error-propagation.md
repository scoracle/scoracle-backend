# First GPT Audit — Session 5: Repair BDL exception and rate-limit propagation

**Worked:** 2026-06-21 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 5

**Depends on:** Session 3 (live NBA/NFL ingestion) + Session 4 (completeness gate),
shipped together in `131274d`; Session 3 finalized/deployed in `eb4912e`.

**Product authority:** wiki `Product Narrative`

## Goal

Make BallDontLie provider failures retain their real meaning all the way to the
operator. A 429 must pause the run (not churn fixture retries); an all-requests-
failed schedule load must surface the error (not look like an empty schedule);
and a legitimate `0` score must survive parsing.

## What Session 3 already did (entangled)

`131274d` already added `except RateLimitExhausted: raise` to the
**fixture-processing path** — `get_games` (the param-candidate loop),
`_fetch_box_score_lines` (NBA/NFL), `_fetch_team_stats` (NFL) — and the
`event process` loop already exits **2** on `RateLimitExhausted` without
touching `seed_attempts`. This session closes the remaining gaps.

## Decisions

1. **A 429 is special; everything else is a recordable failure.** The handlers
   re-raise `RateLimitExhausted` past every broad `except Exception`; any other
   error stays an ordinary failure that the loop records against the fixture.
   No new exception taxonomy — the existing `RateLimitExhausted` vs. "other"
   split already maps cleanly to the audit's four cases.
2. **Distinguish "every request failed" from "succeeded with no rows."**
   `get_games` now tracks whether *any* candidate request succeeded. If none
   did, it raises the last error; a request that returns an empty list is a
   legitimately empty window and returns `[]`.
3. **Preserve a legitimate 0.** A `_pick_score(primary, fallback)` helper falls
   back only when the primary key is *absent* (`is not None`), never on a falsy
   `0`. The old `visitor_team_score or away_team_score` dropped a real 0.
4. **`load_fixtures` pauses cleanly on 429** with exit 2, mirroring
   `event process`, instead of dumping a traceback — so a schedule-refresh cron
   tick logs a clean pause and resumes next tick.

## Why the zero-score fix matters beyond cosmetics

An NFL shutout (away team scores exactly 0) hit the bug: `0 or away_team_score`
→ `away_team_score` is usually absent → `away_score=None`. In `get_box_score`
that `None` fails the `isinstance(away_score, int)` guard, so the score is never
recorded — and **Session 4's completeness gate requires both scores present**,
so a genuinely-complete final game would be rejected as incomplete. The fix
closes that interaction.

## What changed

### `seed/services/event/handlers/bdl_nba.py`

- New `_pick_score()` helper.
- `_get_first_success()` — re-raises `RateLimitExhausted` immediately (so a 429
  on the first path does not issue a second request inside the throttle window).
- `get_games()` — raises the last error when every param candidate fails;
  `away_score` via `_pick_score`.
- `get_player()`, `get_all_players()` — re-raise `RateLimitExhausted` before the
  broad handler (these feed meta/roster seeding, which previously swallowed a
  429 into `None`/`[]`).
- `get_box_score()` — `away_score` via `_pick_score`.

### `seed/services/event/handlers/bdl_nfl.py`

- Same set: `_pick_score`; `get_games` raise-on-all-fail + score; `get_player`,
  `get_all_players` re-raise; `get_box_score` score. (`get_games`,
  `_fetch_box_score_lines`, `_fetch_team_stats` 429 re-raises were already there.)

### `seed/services/event/cli.py`

- `load_fixtures` — `except RateLimitExhausted` → clean message + `sys.exit(2)`,
  matching `event process`.

### Tests

- `seed/tests/test_event_bdl_rate_limits.py` (extended) — 429 now propagates
  through `get_teams`, `get_player`, `get_all_players` (NBA + NFL), and
  `_get_first_success` re-raises without a second request.
- `seed/tests/test_event_bdl_schedule_scores.py` (new) — schedule load raises
  when all candidates fail vs. returns `[]` on empty success; `_pick_score`
  preserves 0 / falls back only when absent; `get_games` preserves a 0 away
  score for NBA and an NFL shutout.

## Maps to the audit's verification

- **Simulated 429 exits the whole process with resume state preserved /
  `seed_attempts` unchanged** — `process` loop's exit-2 path (shipped in
  `131274d`) is untouched; handlers now guarantee the 429 reaches it from every
  path, including meta/roster.
- **Simulated HTTP 500 is reported clearly** — `get_games` raises the last
  error; `load_fixtures` has no `except` for non-429, so it surfaces (non-zero
  exit), and `process` records it as a fixture failure.
- **Empty successful response distinguishable from a failed request** —
  covered by `test_schedule_success_empty_returns_empty` vs.
  `test_schedule_all_candidates_fail_raises`.
- **Zero scores remain zero** — `_pick_score` + `get_games` zero-score tests.

## Verification

- `pytest seed/tests/` — **55 passed** (was 40; +15 new cases).
- `python -m py_compile` / ast-parse clean on all three touched modules.

## Not done here (deliberate)

- **Football (SportMonks) error propagation** — out of Session 5's BDL scope.
  The new `load_fixtures` 429 handler will still catch a `RateLimitExhausted`
  from the football branch if one is ever raised, but SportMonks-specific
  exception narrowing is not part of this session.
- **CLI-level integration test of the exit-2 path** — needs heavy pool/conn
  mocking; the exit-2 behavior is implemented (since `131274d`) and the handler
  layer that feeds it is now unit-covered.

## Files changed

- `seed/services/event/handlers/bdl_nba.py`
- `seed/services/event/handlers/bdl_nfl.py`
- `seed/services/event/cli.py`
- `seed/tests/test_event_bdl_rate_limits.py`
- `seed/tests/test_event_bdl_schedule_scores.py` (new)
- `progress_docs/2026-06-21_first-gpt-audit-session-5-bdl-error-propagation.md` (this doc)
