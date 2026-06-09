-- ============================================================================
-- 048_backfill_nfl_position.sql
-- Backfill the missing NFL player POSITION on older seasons so the Phase-3
-- counting-stat pizza (and position-cohort ranking) works there too.
--
-- Bug: NFL player_stats.position was only captured from 2023 on — 2018-2022 rows are
-- 100% empty (9,159 rows). With no position, public.position_group() returns NULL, so
-- template_block() yields NULL and the Composite card falls back to the 3 facet z-pizzas
-- (and nflSideOfBall can't pick a side) — i.e. old NFL seasons lost the counting-stat
-- layout. The canonical position lives in players.meta->>'position_abbreviation'
-- ("QB", "WR", …), which position_group already understands.
--
-- Fix: backfill position from meta on the empty rows, then re-rank the affected seasons
-- (percentiles + ratings partition by position, so the cohorts changed). Pure data fix —
-- no code change; the deployed template_block + frontend render the template once the
-- position resolves. 'UNK' is skipped (stays empty → z-pizza, correct).
--
-- Per-season percentile/rating cohorts are internally consistent: each old season was
-- 100% empty, so every row is backfilled from meta (abbreviation) uniformly.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/048_backfill_nfl_position.sql
-- ============================================================================

BEGIN;

-- Affected seasons (those with empty positions) — captured BEFORE the backfill.
CREATE TEMP TABLE _affected ON COMMIT DROP AS
SELECT DISTINCT season
FROM public.player_stats
WHERE sport = 'NFL' AND (position IS NULL OR position = '');

-- 1. Backfill position from the player's canonical meta abbreviation.
UPDATE public.player_stats ps
SET position = pl.meta->>'position_abbreviation'
FROM public.players pl
WHERE ps.sport = 'NFL'
  AND pl.id = ps.player_id AND pl.sport = 'NFL'
  AND (ps.position IS NULL OR ps.position = '')
  AND NULLIF(pl.meta->>'position_abbreviation', '') IS NOT NULL
  AND pl.meta->>'position_abbreviation' <> 'UNK';

-- 2. Re-rank the affected seasons — percentiles (template wedges) AND ratings
-- (scoped position re-ranks) partition by position, which just changed. The milestone
-- NOTIFY trigger is disabled during the bulk recalc (a backfill shouldn't notify).
ALTER TABLE public.player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;

DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT season FROM _affected ORDER BY season LOOP
        PERFORM recalculate_percentiles('NFL', r.season);
        PERFORM compute_rating('NFL', r.season);
    END LOOP;
END $$;

ALTER TABLE public.player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

-- 3. Smoke — the empty-position rows should be (nearly) gone, and a known old QB season
-- must now resolve to the quarterback template group.
DO $$
DECLARE
    v_remaining BIGINT;
    v_dak_pg    TEXT;
BEGIN
    SELECT count(*) INTO v_remaining FROM public.player_stats
     WHERE sport = 'NFL' AND season <= 2022 AND (position IS NULL OR position = '');
    -- Dak Prescott (player_id 25) 2022 — was empty, should now map to quarterback.
    SELECT public.position_group('NFL', position) INTO v_dak_pg
     FROM public.player_stats WHERE sport='NFL' AND player_id = 25 AND season = 2022;
    RAISE NOTICE 'BACKFILL OK: % old NFL rows still empty (UNK only); Dak 2022 position_group = %',
        v_remaining, v_dak_pg;
    IF v_dak_pg IS DISTINCT FROM 'quarterback' THEN
        RAISE EXCEPTION 'SMOKE FAILURE: Dak 2022 did not resolve to quarterback (got %)', v_dak_pg;
    END IF;
END $$;

COMMIT;
