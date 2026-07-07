-- 132_nfl_player_positionless_air_ground.sql
--
-- NFL player z-score cleanup:
--   * Move NFL player composites from offense/defense/special facet-balancing to
--     the same flat, positionless z-score sum used by the exploratory leaderboard.
--   * Replace the player rating equation with box-score-only responsibility buckets:
--       Air Yards Responsible, Ground Yards Responsible, Points Responsible For,
--       Giveaways, Tackling, Tackles For Loss, Interceptions.
--   * Drop Receiving, Fumbles Lost, Sacks, Pass Defense, Field Goals, and Punting
--     as standalone composite datapoints.

BEGIN;

DO $do$
DECLARE
    v_def TEXT;
    v_new TEXT;
    v_start INTEGER;
    v_end INTEGER;
    v_start_marker TEXT := '(''Total Yards'',';
    v_end_marker TEXT := '(''Punting'',          NULLIF(p_stats->>''punts_inside_20'','''')::numeric,     TRUE, TRUE,   1, ''special'', ''punts_inside_20'')';
    v_repl TEXT := $repl$('Air Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>'interception_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_yards' || rs.suffix))::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>('receiving_yards' || rs.suffix))::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>('kick_return_yards' || rs.suffix))::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_returner_return_yards' || rs.suffix))::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_yards' || rs.suffix))::numeric,(p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>('interception_yards' || rs.suffix))::numeric,(p_stats->>'interception_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Ground Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'rushing_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('rushing_yards' || rs.suffix))::numeric,(p_stats->>'rushing_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', 'rushing_yards'),
        ('Points Responsible For',
            CASE WHEN p_rate_mode = 'total' THEN
                  6 * (
                      COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>'extra_points_made')::numeric,0)
            ELSE
                  6 * (
                      COALESCE((p_stats->>('passing_touchdowns' || rs.suffix))::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('rushing_touchdowns' || rs.suffix))::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('receiving_touchdowns' || rs.suffix))::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('kick_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('punt_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('interception_touchdowns' || rs.suffix))::numeric,(p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('fumbles_touchdowns' || rs.suffix))::numeric,(p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>('field_goals_made' || rs.suffix))::numeric,(p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>('extra_points_made' || rs.suffix))::numeric,(p_stats->>'extra_points_made')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>('fumbles_lost' || rs.suffix))::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss',
            CASE WHEN p_rate_mode = 'total' THEN
                  GREATEST(
                      COALESCE((p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>'defensive_sacks')::numeric,0)
                  )
            ELSE
                  GREATEST(
                      COALESCE((p_stats->>('tackles_for_loss' || rs.suffix))::numeric,(p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>('defensive_sacks' || rs.suffix))::numeric,(p_stats->>'defensive_sacks')::numeric,0)
                  )
            END,                                                                  TRUE, TRUE,   1, 'defense', NULL),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions')$repl$;
BEGIN
    SELECT pg_get_functiondef('public.rating_datapoints(text,jsonb,text,text)'::regprocedure)
      INTO v_def;

    v_start := strpos(v_def, v_start_marker);
    v_end := strpos(v_def, v_end_marker);

    IF v_start = 0 OR v_end = 0 OR v_end <= v_start THEN
        RAISE EXCEPTION '132 FAIL: expected NFL rating_datapoints block not found';
    END IF;

    v_new := substr(v_def, 1, v_start - 1)
          || v_repl
          || substr(v_def, v_end + length(v_end_marker));

    EXECUTE v_new;
END $do$;

DO $do$
DECLARE
    v_def TEXT;
    v_new TEXT;
BEGIN
    SELECT pg_get_functiondef('public._compute_rating_bundle(text,integer,text)'::regprocedure)
      INTO v_def;

    v_new := replace(
        v_def,
        $old$comp AS (
        SELECT player_id, league_id, composite FROM comp_flat  WHERE p_sport <> 'NFL'
        UNION ALL
        SELECT player_id, league_id, composite FROM comp_facet WHERE p_sport =  'NFL'
    ),$old$,
        $new$comp AS (
        SELECT player_id, league_id, composite FROM comp_flat
    ),$new$
    );

    IF v_new = v_def THEN
        RAISE EXCEPTION '132 FAIL: expected NFL facet-balanced season composite branch not found';
    END IF;

    EXECUTE v_new;
END $do$;

DO $do$
DECLARE
    v_def TEXT;
    v_new TEXT;
BEGIN
    SELECT pg_get_functiondef('public.compute_event_starline(text,integer)'::regprocedure)
      INTO v_def;

    v_new := replace(
        v_def,
        'v_balanced BOOLEAN := (p_sport = ''NFL'');',
        'v_balanced BOOLEAN := FALSE;'
    );

    IF v_new = v_def THEN
        RAISE EXCEPTION '132 FAIL: expected NFL facet-balanced event starline flag not found';
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
    v_labels TEXT[];
    v_removed INTEGER;
    v_air NUMERIC;
    v_ground NUMERIC;
    v_points NUMERIC;
    v_giveaways NUMERIC;
    v_tfl NUMERIC;
BEGIN
    SELECT array_agg(label ORDER BY label) INTO v_labels
    FROM rating_datapoints('NFL', '{
        "passing_yards": 100,
        "receiving_yards": 50,
        "kick_return_yards": 20,
        "punt_returner_return_yards": 5,
        "punt_yards": 10,
        "interception_yards": 15,
        "rushing_yards": 80,
        "passing_touchdowns": 2,
        "rushing_touchdowns": 1,
        "receiving_touchdowns": 1,
        "kick_return_touchdowns": 1,
        "punt_return_touchdowns": 1,
        "interception_touchdowns": 1,
        "fumbles_touchdowns": 1,
        "field_goals_made": 2,
        "extra_points_made": 3,
        "passing_interceptions": 2,
        "fumbles_lost": 1,
        "total_tackles": 9,
        "tackles_for_loss": 2,
        "defensive_sacks": 4,
        "defensive_interceptions": 2,
        "passes_defended": 99
    }'::jsonb);

    IF v_labels <> ARRAY[
        'Air Yards Responsible',
        'Giveaways',
        'Ground Yards Responsible',
        'Interceptions',
        'Points Responsible For',
        'Tackles For Loss',
        'Tackling'
    ] THEN
        RAISE EXCEPTION '132 FAIL: unexpected NFL datapoint labels: %', v_labels;
    END IF;

    SELECT count(*) INTO v_removed
    FROM rating_datapoints('NFL', '{"passes_defended": 99, "defensive_sacks": 4, "fumbles_lost": 1}'::jsonb)
    WHERE label IN ('Receiving', 'Fumbles Lost', 'Sacks', 'Pass Defense', 'Field Goals', 'Punting');

    IF v_removed <> 0 THEN
        RAISE EXCEPTION '132 FAIL: removed NFL datapoints still emit';
    END IF;

    SELECT value INTO v_air FROM rating_datapoints('NFL', '{
        "passing_yards": 100,
        "receiving_yards": 50,
        "kick_return_yards": 20,
        "punt_returner_return_yards": 5,
        "punt_yards": 10,
        "interception_yards": 15
    }'::jsonb) WHERE label = 'Air Yards Responsible';
    SELECT value INTO v_ground FROM rating_datapoints('NFL', '{"rushing_yards": 80}'::jsonb) WHERE label = 'Ground Yards Responsible';
    SELECT value INTO v_points FROM rating_datapoints('NFL', '{
        "passing_touchdowns": 2,
        "rushing_touchdowns": 1,
        "receiving_touchdowns": 1,
        "kick_return_touchdowns": 1,
        "punt_return_touchdowns": 1,
        "interception_touchdowns": 1,
        "fumbles_touchdowns": 1,
        "field_goals_made": 2,
        "extra_points_made": 3
    }'::jsonb) WHERE label = 'Points Responsible For';
    SELECT value INTO v_giveaways FROM rating_datapoints('NFL', '{"passing_interceptions": 2, "fumbles_lost": 1}'::jsonb) WHERE label = 'Giveaways';
    SELECT value INTO v_tfl FROM rating_datapoints('NFL', '{"tackles_for_loss": 2, "defensive_sacks": 4}'::jsonb) WHERE label = 'Tackles For Loss';

    IF v_air <> 200 OR v_ground <> 80 OR v_points <> 57 OR v_giveaways <> 3 OR v_tfl <> 4 THEN
        RAISE EXCEPTION '132 FAIL: derived datapoint values malformed: air %, ground %, points %, giveaways %, tfl %',
            v_air, v_ground, v_points, v_giveaways, v_tfl;
    END IF;

    SELECT count(*) INTO v_removed
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport = 'NFL'
      AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' IN ('Receiving', 'Fumbles Lost', 'Sacks', 'Pass Defense', 'Field Goals', 'Punting');

    IF v_removed <> 0 THEN
        RAISE EXCEPTION '132 FAIL: % NFL player rating_breakdown rows still contain removed labels', v_removed;
    END IF;

    RAISE NOTICE '132 OK: NFL player z-scores use flat air/ground responsibility equation';
END $$;

INSERT INTO public.schema_migrations(version)
VALUES ('132_nfl_player_positionless_air_ground')
ON CONFLICT (version) DO NOTHING;

COMMIT;
