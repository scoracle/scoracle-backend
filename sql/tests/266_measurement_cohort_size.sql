-- Run after migration 266 (breakdown carries per-measure comparison population).
BEGIN;
SET LOCAL search_path = pg_temp, public;
CREATE TEMP TABLE player_stats AS SELECT * FROM public.player_stats WITH NO DATA;
CREATE TEMP TABLE team_stats AS SELECT * FROM public.team_stats WITH NO DATA;

INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,0,'NBA',2096,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"blk":2.0,"turnover":2.6,"pf":1.9,"fta":5.5,"plus_minus":3.4,"stl":1.1,"fg3m":1.1,"minutes":34.0,"games_played":70}'),
(-2,0,'NBA',2096,'C','{"pts":14.1,"reb":5.4,"ast":3.2,"blk":0.9,"turnover":1.6,"pf":2.1,"fta":2.5,"plus_minus":0.4,"stl":0.8,"fg3m":0.7,"minutes":30.0,"games_played":55}');

SELECT public.compute_rating('NBA',2096);

DO $check$
BEGIN
    IF (SELECT count(*) FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-1 AND d->>'pct' IS NOT NULL
          AND d->>'cohort' IS NOT NULL AND (d->>'cohort')::numeric >= 2) <> 10
        THEN RAISE EXCEPTION 'Ranked observations did not carry their population'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE d->>'pct' IS NOT NULL AND (d->>'cohort') IS NULL
    ) THEN RAISE EXCEPTION 'Percentile stored without its comparison population'; END IF;
END $check$;

INSERT INTO pg_temp.team_stats(team_id,league_id,sport,season,stats) VALUES
(-1,0,'NBA',2096,'{"pts":114.2,"ast":26.0,"fg3m":13.1,"fta":21.0,"turnover":13.5,"oreb":10.2,"blk":5.1,"stl":8.0,"reb":45.0,"pts_allowed":110.0,"dreb":34.8,"def_fg_pct":46.0,"def_fg3_pct":35.0}'),
(-2,0,'NBA',2096,'{"pts":101.0,"ast":19.0,"fg3m":9.0,"fta":15.0,"turnover":16.0,"oreb":8.0,"blk":3.0,"stl":6.0,"reb":40.0,"pts_allowed":118.0,"dreb":30.0,"def_fg_pct":50.0,"def_fg3_pct":39.0}');

SELECT public.compute_team_rating('NBA',2096);

DO $team$
BEGIN
    IF (SELECT count(*) FROM pg_temp.team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE d->>'pct' IS NOT NULL AND (d->>'cohort')::numeric = 2) <> 26
        THEN RAISE EXCEPTION 'Team percentiles did not carry their two-team population'; END IF;
END $team$;
ROLLBACK;
