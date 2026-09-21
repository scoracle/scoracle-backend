-- Run after migration 264 (rating_measurements/rating_measurements_team must
-- carry source measurement identity for the NBA and NFL branches).
-- Every write targets explicitly temporary copies; production statistics are
-- untouched. Numerical parity itself is proven inside the migration; this file
-- exercises the rebuilt identity through the real compute paths.
BEGIN;
SET LOCAL search_path = pg_temp, public;
CREATE TEMP TABLE player_stats AS SELECT * FROM public.player_stats WITH NO DATA;
CREATE TEMP TABLE team_stats AS SELECT * FROM public.team_stats WITH NO DATA;

INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,0,'NBA',2098,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"stl":1.1,"blk":2.0,"fg3m":1.1,"plus_minus":3.4,"turnover":2.6,"pf":1.9,"fta":5.5,"minutes":34.0,"games_played":70,"fg3a":6.0,"fta_per_36":5.5,"pts_per_36":25.0}'),
(-2,0,'NBA',2098,'C','{"pts":10.1,"reb":4.4,"ast":2.2,"stl":0.6,"blk":0.5,"fg3m":0.2,"plus_minus":-1.1,"turnover":1.2,"pf":2.4,"fta":1.5,"minutes":8.0,"games_played":12,"pts_per_36":16.0}'),
(-5,0,'NBA',2098,'C','{"pts":14.1,"reb":5.4,"ast":3.2,"stl":0.8,"blk":0.9,"fg3m":0.7,"plus_minus":0.4,"turnover":1.6,"pf":2.1,"fta":2.5,"minutes":30.0,"games_played":55,"pts_per_36":18.0}'),
(-3,0,'NFL',2098,'QB','{"passing_yards":4000,"rushing_yards":300,"passing_touchdowns":30,"receiving_touchdowns":0,"kick_return_touchdowns":0,"punt_return_touchdowns":0,"interception_touchdowns":0,"fumbles_touchdowns":0,"field_goals_made":0,"extra_points_made":0,"passing_interceptions":10,"fumbles_lost":2,"total_tackles":0,"tackles_for_loss":0,"defensive_sacks":0,"defensive_interceptions":0,"games_played":16}'),
(-4,0,'NFL',2098,'LB','{"passing_yards":0,"rushing_yards":0,"passing_touchdowns":0,"field_goals_made":0,"passing_interceptions":0,"fumbles_lost":0,"total_tackles":110,"tackles_for_loss":4,"defensive_sacks":9,"defensive_interceptions":2,"games_played":16}');

SELECT public.compute_rating('NBA',2098);
SELECT public.compute_rating('NFL',2098);

DO $check$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(COALESCE(ps.rating_breakdown,'[]'::jsonb)) d
        WHERE ps.sport IN ('NBA','NFL') AND (d->>'measure' = d->>'label' OR COALESCE(d->>'measure','')='')
    ) THEN RAISE EXCEPTION 'NBA/NFL stored breakdown echoes the display label or lost identity'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-1 AND d->>'label'='Rim Protection' AND d->>'measure'='blocks'
          AND (d->>'value')::numeric=2.0) <> 1
        THEN RAISE EXCEPTION 'Rim Protection did not persist its source measurement (blk as blocks)'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-1 AND d->>'label'='Ball Security' AND d->>'measure'='turnovers'
          AND d->>'sign'='-1') <> 1
        THEN RAISE EXCEPTION 'Ball Security identity or polarity changed'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-1 AND d->>'label'='On-Court Impact' AND d->>'measure'='plus-minus') <> 1
        THEN RAISE EXCEPTION 'On-Court Impact lost source identity (plus_minus as plus-minus)'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-1 AND (d->>'pct') IS NOT NULL
          AND d->>'label' IN ('Scoring','Playmaking','Rim Protection','Ball Security')) <> 4
        THEN RAISE EXCEPTION 'Eligible NBA player lost a percentile'; END IF;

    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-2 AND (d->>'pct') IS NOT NULL
    ) THEN RAISE EXCEPTION 'Thin NBA sample was ranked'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-3 AND d->>'label'='Points Responsible For'
          AND d->>'measure'='6 x touchdowns (passing, rushing, receiving, kick return, punt return, interception, fumble) + 3 x field goals made + extra points made') <> 1
        THEN RAISE EXCEPTION 'Points Responsible For lost formula identity'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-4 AND d->>'label'='Tackles For Loss'
          AND d->>'measure'='max of tackles for loss and defensive sacks'
          AND (d->>'value')::numeric=9.0) <> 1
        THEN RAISE EXCEPTION 'Tackles For Loss lost max-combination value or identity'; END IF;

    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-3 AND d->>'label'='Giveaways' AND d->>'measure'='passing interceptions + fumbles lost'
          AND (d->>'value')::numeric=12.0) <> 1
        THEN RAISE EXCEPTION 'Giveaways value or identity changed'; END IF;
END $check$;

-- Rate modes keep identity too, and keep requiring their denominator.
DO $rates$
BEGIN
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NBA','{"pts":24.0}','per_36','C') WHERE label='Scoring' AND value IS NOT NULL)
        THEN RAISE EXCEPTION 'Rate mode without a denominator survived'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NBA','{"blk":2.0,"blk_per_36":2.6,"minutes":34.0}','per_36','C') WHERE label='Rim Protection')
        IS DISTINCT FROM 'blocks'
        THEN RAISE EXCEPTION 'NBA rate measurement lost source identity'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NFL','{"passing_yards":300.0,"passing_yards_per_game":20.0,"games_played":16}','per_game','QB') WHERE label='Air Yards Responsible')
        IS DISTINCT FROM 'sum of passing, receiving, kick-return, punt-return, punt and interception yards'
        THEN RAISE EXCEPTION 'NFL rate measurement lost formula identity'; END IF;
    IF EXISTS (SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_each(ps.rating_modes) m
        CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') d
        WHERE ps.sport IN ('NBA','NFL') AND (d->>'measure' = d->>'label' OR COALESCE(d->>'measure','')=''))
        THEN RAISE EXCEPTION 'Stored NBA/NFL rate mode echoes the display label or lost identity'; END IF;
END $rates$;

INSERT INTO pg_temp.team_stats(team_id,league_id,sport,season,stats) VALUES
(-1,0,'NBA',2098,'{"pts":114.2,"ast":26.0,"fg3m":13.1,"fta":21.0,"turnover":13.5,"oreb":10.2,"blk":5.1,"stl":8.0,"reb":45.0,"pts_allowed":110.0,"dreb":34.8,"def_fg_pct":46.0,"def_fg3_pct":35.0}'),
(-2,0,'NBA',2098,'{"pts":101.0,"ast":19.0,"fg3m":9.0,"fta":15.0,"turnover":16.0,"oreb":8.0,"blk":3.0,"stl":6.0,"reb":40.0,"pts_allowed":118.0,"dreb":30.0,"def_fg_pct":50.0,"def_fg3_pct":39.0}'),
(-3,0,'NFL',2098,'{"points_for":28.0,"total_yards":380.0,"turnovers":1.0,"passing_touchdowns":2,"rushing_touchdowns":1,"first_downs":21,"field_goals_made":2,"red_zone_pct":60.0,"third_down_pct":42.0,"penalty_yards_drawn":40,"total_tackles":70,"defensive_sacks":3,"passes_defended":7,"defensive_interceptions":1,"points_against":17.0,"yards_allowed":300.0,"penalty_yards":55,"tackles_for_loss":5,"takeaways":2,"red_zone_def_pct":50.0,"third_down_def_pct":38.0,"first_downs_allowed":18}');

SELECT public.compute_team_rating('NBA',2098);
SELECT public.compute_team_rating('NFL',2098);

DO $team$
BEGIN
    IF (SELECT count(*) FROM pg_temp.team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE ts.sport='NBA' AND d->>'label'='Rim Protection' AND d->>'measure'='blocks') <> 2
        THEN RAISE EXCEPTION 'Team Rim Protection lost source identity'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(COALESCE(ts.rating_breakdown,'[]'::jsonb)) d
        WHERE ts.sport='NBA' AND d->>'measure' = d->>'label'
    ) THEN RAISE EXCEPTION 'NBA team breakdown echoes the display label'; END IF;
    IF (SELECT count(*) FROM pg_temp.team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE ts.sport='NFL' AND d->>'label'='Touchdowns' AND d->>'measure'='passing + rushing touchdowns'
          AND (d->>'value')::numeric=3.0) <> 1
        THEN RAISE EXCEPTION 'Team Touchdowns lost sum identity'; END IF;
END $team$;
ROLLBACK;
