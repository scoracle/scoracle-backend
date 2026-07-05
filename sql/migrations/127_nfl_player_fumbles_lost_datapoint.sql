-- 127_nfl_player_fumbles_lost_datapoint.sql
--
-- NFL player z-score cleanup:
--   * Split fumbles_lost out of the aggregate Giveaways datapoint.
--   * Add Fumbles Lost as its own negative offensive datapoint.
-- This keeps the fumble-loss penalty visible without double-counting it.

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
        $rx$\('Giveaways',[[:space:]]+CASE WHEN p_rate_mode = 'total' THEN[[:space:]]+COALESCE\(\(p_stats->>'passing_interceptions'\)::numeric,0\)[[:space:]]+\+ COALESCE\(\(p_stats->>'fumbles_lost'\)::numeric,0\)[[:space:]]+ELSE[[:space:]]+COALESCE\(\(p_stats->>\('passing_interceptions' \|\| rs\.suffix\)\)::numeric,\(p_stats->>'passing_interceptions'\)::numeric,0\)[[:space:]]+\+ COALESCE\(\(p_stats->>\('fumbles_lost' \|\| rs\.suffix\)\)::numeric,\(p_stats->>'fumbles_lost'\)::numeric,0\)[[:space:]]+END,[[:space:]]+TRUE,[[:space:]]+FALSE,[[:space:]]+-1,[[:space:]]+'offense',[[:space:]]+NULL\)$rx$,
        $repl$('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Fumbles Lost',     NULLIF(p_stats->>'fumbles_lost','')::numeric,        TRUE, FALSE, -1, 'offense', 'fumbles_lost')$repl$
    );

    IF v_new = v_def THEN
        RAISE EXCEPTION '127 FAIL: expected NFL Giveaways row with fumbles_lost not found in rating_datapoints';
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
    v_giveaways INTEGER;
    v_fumbles_lost INTEGER;
    v_recovery INTEGER;
BEGIN
    SELECT count(*) INTO v_giveaways
    FROM rating_datapoints(
        'NFL',
        '{"passing_interceptions": 3, "fumbles_lost": 2}'::jsonb
    )
    WHERE label = 'Giveaways'
      AND value = 3
      AND in_comp
      AND NOT in_spec
      AND sign = -1
      AND facet = 'offense';

    SELECT count(*) INTO v_fumbles_lost
    FROM rating_datapoints(
        'NFL',
        '{"passing_interceptions": 3, "fumbles_lost": 2}'::jsonb
    )
    WHERE label = 'Fumbles Lost'
      AND value = 2
      AND in_comp
      AND NOT in_spec
      AND sign = -1
      AND facet = 'offense';

    SELECT count(*) INTO v_recovery
    FROM rating_datapoints(
        'NFL',
        '{"fumbles_lost": 2, "fumbles_recovered": 9}'::jsonb
    )
    WHERE label = 'Fumble Recovery';

    IF v_giveaways <> 1 THEN
        RAISE EXCEPTION '127 FAIL: Giveaways should count passing_interceptions only';
    END IF;
    IF v_fumbles_lost <> 1 THEN
        RAISE EXCEPTION '127 FAIL: Fumbles Lost datapoint missing or malformed';
    END IF;
    IF v_recovery <> 0 THEN
        RAISE EXCEPTION '127 FAIL: Fumble Recovery datapoint was reintroduced';
    END IF;

    RAISE NOTICE '127 OK: NFL player z-scores expose Fumbles Lost as a negative datapoint';
END $$;

INSERT INTO public.schema_migrations(version)
VALUES ('127_nfl_player_fumbles_lost_datapoint')
ON CONFLICT (version) DO NOTHING;

COMMIT;
