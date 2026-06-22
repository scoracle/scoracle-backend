-- 100_fixture_completeness.sql  (FIRST-GPT-AUDIT Session 4 — fixture finality & completeness)
--
-- The seeder now refuses to mark a fixture seeded unless its provider payload is
-- complete enough to serve (provider not still reporting the game unfinished,
-- both expected teams present with a final score, and a meaningful player-row
-- count) — see seed/services/event/completeness.py. A rejected payload leaves
-- the fixture pending/retryable.
--
-- This migration adds the durable state that keeps an incompleteness rejection
-- distinct from a transport/processing failure:
--   * fixtures.last_incomplete_reason — why the latest payload was judged
--     incomplete (set by record_incomplete(); NULL once the fixture seeds).
--     fixtures.last_seed_error stays reserved for transport/processing errors.
--   * mark_fixture_seeded() clears both error fields on success, so a seeded
--     fixture carries no stale failure reason.
--
-- Additive + idempotent. Mirrors sql/shared.sql (kept in sync to avoid the
-- finalize-tail drift that bit 049/050).

BEGIN;

ALTER TABLE fixtures ADD COLUMN IF NOT EXISTS last_incomplete_reason TEXT;

COMMENT ON COLUMN fixtures.last_incomplete_reason IS
    'Why the most recent provider payload failed the completeness contract '
    '(seed/services/event/completeness.py). Distinct from last_seed_error, which '
    'records transport/processing failures. Cleared by mark_fixture_seeded().';

CREATE OR REPLACE FUNCTION mark_fixture_seeded(
    p_fixture_id INTEGER,
    p_home_score INTEGER DEFAULT NULL,
    p_away_score INTEGER DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    UPDATE fixtures SET
        status = 'seeded', seeded_at = NOW(),
        home_score = COALESCE(p_home_score, home_score),
        away_score = COALESCE(p_away_score, away_score),
        last_seed_error = NULL,
        last_incomplete_reason = NULL,
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$ LANGUAGE plpgsql;

COMMIT;
