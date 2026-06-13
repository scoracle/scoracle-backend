-- ============================================================================
-- 079_football_gate_10_games.sql
-- Set the FOOTBALL rating-eligibility gate to appearances >= 10.
--
-- Context: 078 lowered it 15 -> 3 (a rolled-back impact preview showed ≥1/≥3/≥5 all
-- inflate the ranked cohort). Scott's call: a MEANINGFUL gate — 10 appearances, ">25%
-- of a 38-game PL season" — for ranking integrity, with the separate "data-for-everyone"
-- design (Phase 2) rendering datapoints for sub-10 players UNRANKED (so no empty profiles).
-- This migration is Phase 1: set the composite gate to 10 + recompute.
--
-- At 10, Roméo Lavia (id 37536874, 2025: 12 apps) is now RATED directly. The gate lives in
-- public.rating_thresholds and is applied by both _compute_rating_bundle (067) and
-- recalculate_percentiles (058).
--
-- NOT parity-preserving (the cohort changes vs the pre-078 baseline of 15 — modest, since
-- 10-14-appearance players are a moderate sample). No API restart (reads unchanged); no
-- matview refresh (autofill carries no rating data). Lock-free recompute.
--
-- Apply: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/079_football_gate_10_games.sql
-- ============================================================================

BEGIN;

UPDATE public.rating_thresholds
   SET min_value = 10
 WHERE sport = 'FOOTBALL' AND stat_key = 'appearances';

SET session_replication_role = replica;
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM player_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM recalculate_percentiles('FOOTBALL', r.season);
        PERFORM compute_rating('FOOTBALL', r.season);
    END LOOP;
END $$;
SET session_replication_role = DEFAULT;

DO $$
DECLARE v_gate numeric; v_rated_2025 bigint; v_romeo numeric;
BEGIN
    SELECT min_value INTO v_gate FROM public.rating_thresholds WHERE sport='FOOTBALL' AND stat_key='appearances';
    SELECT count(*) FILTER (WHERE rating_composite IS NOT NULL) INTO v_rated_2025
      FROM player_stats WHERE sport='FOOTBALL' AND season=2025;
    SELECT rating_composite_rank INTO v_romeo
      FROM player_stats WHERE player_id=37536874 AND season=2025;
    RAISE NOTICE '079: FOOTBALL appearances gate now >= % ; 2025 rated players=% ; Roméo Lavia (12 apps) rank=%',
                 v_gate, v_rated_2025, COALESCE(v_romeo::text, 'STILL NULL');
    IF v_gate <> 10 THEN RAISE EXCEPTION '079 FAIL: gate not 10'; END IF;
    IF v_romeo IS NULL THEN RAISE EXCEPTION '079 FAIL: Roméo Lavia (12 apps) should be rated at gate 10'; END IF;
END $$;

COMMIT;
