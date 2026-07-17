-- Sigil ghost backfill — Session E of the sigil pipeline cleanup plan
-- (scoracle-wiki/raw/scoracle-sigil-pipeline-cleanup-plan-2026-07-16.md §Session E).
--
-- Ghosts: entities with a real scored vibe (vibe_scores.sentiment IS NOT NULL) and no
-- sigil_synthesis row ever — all last active before sigil launched 2026-06-17, so
-- news-triggered healing can never reach them. Measured 2026-07-17: 1,141 total
-- (54 headliner / 337 starter / 368 bench / 382 inactive).
--
-- Mechanism: INSERT pipeline_work momentum rows (the pipe's own path — the momentum
-- handler cascades into sigil; existing gates decide who markers out). Ghosts lacking a
-- momentum_scores row honestly land direction=steady (None → steady) — expected, not a
-- bug. Each ghost costs ~3 model calls (momentum read → sigil decide → voice).
--
-- Stagger (mandatory): the daemon's drain_all drains each stage TO EMPTY before moving
-- on, so a bulk-available batch starves every other stage. available_at admits
-- ~:RATE_PER_HOUR entities/hour inside a 10:00–22:00 ET daily window, headliners first,
-- starting :START_DAY (2026-07-20 per Scott — after the accelerated D-wave window
-- closes at 07-20 00:30 ET).
--
-- Cohort (Scott's pick, pending as of 2026-07-17): set :TIERS to
--   headliner+starter:  ARRAY['headliner','starter']            (~391 rows, ~2 days)
--   everything:         ARRAY['headliner','starter','bench','inactive']  (~1,141, ~6 days)
--
-- Run (named approval required — this touches prod):
--   psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f scripts/ops/sigil_ghost_backfill.sql
-- Then verify the schedule:
--   SELECT min(available_at), max(available_at), count(*) FROM pipeline_work
--   WHERE input_version = 'ghost-backfill-2026-07-17';
-- Acceptance after the wave drains: re-run the ghost CTE below bare -> ~0 for the cohort.
-- Rollback before rows activate:
--   DELETE FROM pipeline_work WHERE input_version = 'ghost-backfill-2026-07-17'
--   AND status = 'pending';

\set TIERS 'ARRAY[''headliner'',''starter'',''bench'',''inactive'']'
\set START_DAY '''2026-07-20'''
\set RATE_PER_HOUR 16
\set WINDOW_HOURS 12

BEGIN;

WITH scored_vibe AS (
    SELECT DISTINCT entity_type, entity_id, sport
    FROM vibe_scores
    WHERE sentiment IS NOT NULL
),
ghosts AS (
    SELECT v.entity_type, v.entity_id, v.sport,
           COALESCE(p.tier, t.tier) AS tier
    FROM scored_vibe v
    LEFT JOIN players p ON v.entity_type = 'player' AND p.id = v.entity_id AND p.sport = v.sport
    LEFT JOIN teams  t ON v.entity_type = 'team'   AND t.id = v.entity_id AND t.sport = v.sport
    WHERE NOT EXISTS (
        SELECT 1 FROM sigil_synthesis s
        WHERE s.entity_type = v.entity_type
          AND s.entity_id   = v.entity_id
          AND s.sport       = v.sport
    )
    AND COALESCE(p.tier, t.tier) = ANY (:TIERS)
),
ordered AS (
    SELECT g.*,
           row_number() OVER (
               ORDER BY array_position(ARRAY['headliner','starter','bench','inactive'], g.tier),
                        g.entity_type, g.sport, g.entity_id
           ) - 1 AS idx
    FROM ghosts g
)
INSERT INTO pipeline_work
    (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
SELECT
    'momentum', o.entity_type, o.entity_id, o.sport, 'pending',
    'ghost-backfill-2026-07-17',
    -- day N at 10:00 ET + fractional-hour offset inside the 12h window
    (:START_DAY::date
        + (o.idx / (:RATE_PER_HOUR * :WINDOW_HOURS)) * interval '1 day'
        + interval '10 hours'
        + ((o.idx % (:RATE_PER_HOUR * :WINDOW_HOURS))::float8 / :RATE_PER_HOUR) * interval '1 hour'
    ) AT TIME ZONE 'America/New_York',
    NOW()
FROM ordered o
ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING;

-- Review before COMMIT: row count + schedule span.
SELECT count(*) AS enqueued,
       min(available_at) AS first_row,
       max(available_at) AS last_row
FROM pipeline_work
WHERE input_version = 'ghost-backfill-2026-07-17';

COMMIT;
