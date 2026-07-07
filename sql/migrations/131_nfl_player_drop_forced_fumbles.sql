-- 131_nfl_player_drop_forced_fumbles.sql
--
-- NFL player z-score cleanup:
--   * Remove Forced Fumbles from the player rating engine.
--
-- BDL does not include forced fumbles in the event box-score payload. Keep the
-- rating equation tied to box-score source-of-truth fields only.

BEGIN;

DO $do$
DECLARE
    v_def TEXT;
    v_new TEXT;
BEGIN
    SELECT pg_get_functiondef('public.rating_datapoints(text,jsonb,text,text)'::regprocedure)
      INTO v_def;

    v_new := regexp_replace(
        v_def,
        $rx$[[:space:]]*\('Forced Fumbles',[[:space:]]+NULLIF\(p_stats->>'fumbles_forced',''\)::numeric,[[:space:]]+TRUE,[[:space:]]+TRUE,[[:space:]]+1,[[:space:]]+'defense',[[:space:]]+'fumbles_forced'\),$rx$,
        ''
    );

    IF v_new = v_def THEN
        RAISE EXCEPTION '131 FAIL: expected Forced Fumbles row not found in rating_datapoints';
    END IF;

    EXECUTE v_new;
END $do$;

ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM player_stats WHERE sport = 'NFL' ORDER BY 1 LOOP
        PERFORM compute_rating('NFL', s);
        PERFORM compute_event_starline('NFL', s);
    END LOOP;
END $$;
ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

DO $$
DECLARE
    v_forced INTEGER;
    v_sacks INTEGER;
    v_breakdown INTEGER;
BEGIN
    SELECT count(*) INTO v_forced
    FROM rating_datapoints('NFL', '{"fumbles_forced": 2}'::jsonb)
    WHERE label = 'Forced Fumbles';

    SELECT count(*) INTO v_sacks
    FROM rating_datapoints('NFL', '{"defensive_sacks": 3}'::jsonb)
    WHERE label = 'Sacks'
      AND value = 3
      AND in_comp
      AND in_spec
      AND sign = 1
      AND facet = 'defense';

    IF v_forced <> 0 THEN
        RAISE EXCEPTION '131 FAIL: Forced Fumbles datapoint still emitted';
    END IF;
    IF v_sacks <> 1 THEN
        RAISE EXCEPTION '131 FAIL: defensive datapoints were malformed while removing Forced Fumbles';
    END IF;

    SELECT count(*) INTO v_breakdown
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport = 'NFL'
      AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' = 'Forced Fumbles';

    IF v_breakdown <> 0 THEN
        RAISE EXCEPTION '131 FAIL: % NFL player rating_breakdown rows still contain Forced Fumbles', v_breakdown;
    END IF;

    RAISE NOTICE '131 OK: NFL player z-scores omit Forced Fumbles';
END $$;

INSERT INTO public.schema_migrations(version)
VALUES ('131_nfl_player_drop_forced_fumbles')
ON CONFLICT (version) DO NOTHING;

COMMIT;
