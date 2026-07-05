-- 126_nfl_player_forced_fumbles.sql
--
-- NFL player z-score cleanup:
--   * Add Forced Fumbles as the defensive fumble-creation datapoint.
--   * Remove Fumble Recovery from the rating engine. Recoveries are rare and noisy,
--     and they were artificially boosting offensive players who happened to recover
--     loose balls.

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
        $rx$\('Fumble Recovery',[[:space:]]+NULLIF\(p_stats->>'fumbles_recovered',''\)::numeric,[[:space:]]+TRUE,[[:space:]]+TRUE,[[:space:]]+1,[[:space:]]+'defense',[[:space:]]+'fumbles_recovered'\)$rx$,
        $repl$('Forced Fumbles',    NULLIF(p_stats->>'fumbles_forced','')::numeric,     TRUE, TRUE,   1, 'defense', 'fumbles_forced')$repl$
    );

    IF v_new = v_def THEN
        RAISE EXCEPTION '126 FAIL: expected Fumble Recovery row not found in rating_datapoints';
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
    v_recovery INTEGER;
BEGIN
    SELECT count(*) INTO v_forced
    FROM rating_datapoints('NFL', '{"fumbles_forced": 2, "fumbles_recovered": 9}'::jsonb)
    WHERE label = 'Forced Fumbles'
      AND value = 2
      AND in_comp
      AND in_spec
      AND sign = 1
      AND facet = 'defense';

    SELECT count(*) INTO v_recovery
    FROM rating_datapoints('NFL', '{"fumbles_forced": 2, "fumbles_recovered": 9}'::jsonb)
    WHERE label = 'Fumble Recovery';

    IF v_forced <> 1 THEN
        RAISE EXCEPTION '126 FAIL: Forced Fumbles datapoint missing or malformed';
    END IF;
    IF v_recovery <> 0 THEN
        RAISE EXCEPTION '126 FAIL: Fumble Recovery datapoint still emitted';
    END IF;

    SELECT count(*) INTO v_recovery
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport = 'NFL'
      AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' = 'Fumble Recovery';

    IF v_recovery <> 0 THEN
        RAISE EXCEPTION '126 FAIL: % NFL player rating_breakdown rows still contain Fumble Recovery', v_recovery;
    END IF;

    RAISE NOTICE '126 OK: NFL player z-scores use Forced Fumbles and omit Fumble Recovery';
END $$;

INSERT INTO public.schema_migrations(version)
VALUES ('126_nfl_player_forced_fumbles')
ON CONFLICT (version) DO NOTHING;

COMMIT;
