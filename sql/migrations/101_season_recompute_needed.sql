-- 101_season_recompute_needed.sql  (FIRST-GPT-AUDIT Session 6 — durable deferred recompute)
--
-- Deferred backfill (`event process --batch`, or the auto-batch path for
-- concluded seasons) finalizes each fixture with finalize_fixture(p_recompute =
-- FALSE) — skipping the expensive whole-season recompute — then runs ONE
-- recompute_season + rating_history snapshot per (sport, season) at end-of-run.
--
-- Previously the set of dirty (sport, season) pairs lived ONLY in an in-memory
-- Python set. If the process died before the end-of-run drain, the season was
-- left seeded-but-unrecomputed with no durable record that it still needed
-- work — invisible drift between mark_fixture_seeded and the rating engine.
--
-- This table makes that state durable:
--   * upserted in the SAME transaction that finalizes a deferred fixture
--     (seed/shared/upsert.py mark_season_recompute_needed);
--   * drained from these rows at end-of-run and by `event recompute-drain`;
--   * deleted only AFTER a successful recompute_season + snapshot_rating_history
--     (so a crash mid-recompute leaves the marker in place for retry);
--   * cleared by `event recompute --sport --season` for the targeted pair.
--
-- attempts/last_error let an operator see a season that keeps failing to
-- recompute rather than silently retrying forever.
--
-- Additive + idempotent. New table only — does NOT touch finalize_fixture, so it
-- is safe to apply while a seeder is mid-run (no cached-plan invalidation).

BEGIN;

CREATE TABLE IF NOT EXISTS season_recompute_needed (
    sport        TEXT        NOT NULL,
    season       INTEGER     NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error   TEXT,
    attempts     INTEGER     NOT NULL DEFAULT 0,
    PRIMARY KEY (sport, season)
);

COMMENT ON TABLE season_recompute_needed IS
    'Durable dirty-season queue for deferred backfill (FIRST-GPT-AUDIT Session '
    '6). A row means (sport, season) had >=1 fixture finalized with '
    'recompute=FALSE and still needs recompute_season + snapshot_rating_history. '
    'Upserted inside the finalize transaction; deleted only after a successful '
    'recompute + snapshot (end-of-run drain, `event recompute-drain`, or '
    '`event recompute --sport --season`).';

COMMIT;
