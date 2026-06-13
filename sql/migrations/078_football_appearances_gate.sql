-- ============================================================================
-- 078_football_appearances_gate.sql
-- Lower the FOOTBALL rating-eligibility gate from appearances >= 15 to >= 3.
--
-- The gate lives in public.rating_thresholds (sport, stat_key, min_value) and is
-- applied by BOTH _compute_rating_bundle (067) and recalculate_percentiles (058)
-- as `bool_and(COALESCE((stats->>stat_key)::numeric,0) >= min_value)`. At >= 15 it
-- dropped ~5,966 football player-seasons that have FULL stats — e.g. Roméo Lavia
-- (id 37536874) 2025: 12 appearances, 132 stat keys, but rating_composite=NULL.
-- That's the "huge miss": real low-minute contributors weren't rated at all.
--
-- Decision (Scott, after a rolled-back impact preview): appearances >= 3 — rates
-- real low-minute players (Roméo's 12 apps qualify), trims 1-2-match cameos.
--
-- NOT parity-preserving (intentionally): the composite is a z-score across the
-- rated cohort, so adding the newly-eligible players re-skews it — existing
-- football ranks/scores shift UP for mid-tier players (the top stays stable; the
-- new low-app players land low on totals, per-90 surfaces standouts). This is the
-- agreed behavior change, so there is NO parity gate (unlike 077).
--
-- Recompute is superuser + session_replication_role=replica (lock-free; no notify
-- storm). No API restart (reads are unchanged prepared statements over player_stats);
-- no matview refresh (autofill carries no rating data). Cached rating responses
-- refresh on TTL.
--
-- Apply: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/078_football_appearances_gate.sql
-- ============================================================================

BEGIN;

-- 1. Lower the gate.
UPDATE public.rating_thresholds
   SET min_value = 3
 WHERE sport = 'FOOTBALL' AND stat_key = 'appearances';

-- 2. Recompute every FOOTBALL season (percentiles gate on the same table, so both).
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

-- 3. Smoke: the gate is now 3, and Roméo Lavia's 2025 row is rated.
DO $$
DECLARE v_gate numeric; v_rated_2025 bigint; v_romeo numeric;
BEGIN
    SELECT min_value INTO v_gate FROM public.rating_thresholds WHERE sport='FOOTBALL' AND stat_key='appearances';
    SELECT count(*) FILTER (WHERE rating_composite IS NOT NULL) INTO v_rated_2025
      FROM player_stats WHERE sport='FOOTBALL' AND season=2025;
    SELECT rating_composite_rank INTO v_romeo
      FROM player_stats WHERE player_id=37536874 AND season=2025;
    RAISE NOTICE '078: FOOTBALL appearances gate now >= % ; 2025 rated players=% ; Roméo Lavia (12 apps) rank=%',
                 v_gate, v_rated_2025, COALESCE(v_romeo::text, 'STILL NULL');
    IF v_gate <> 3 THEN RAISE EXCEPTION '078 FAIL: gate not set to 3'; END IF;
    IF v_romeo IS NULL THEN RAISE EXCEPTION '078 FAIL: Roméo Lavia 2025 still unrated'; END IF;
END $$;

COMMIT;
