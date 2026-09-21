-- Run after migration 265 (degenerate comparison populations withhold z).
-- Every write targets explicitly temporary copies; production statistics are untouched.
BEGIN;
SET LOCAL search_path = pg_temp, public;
CREATE TEMP TABLE player_stats AS SELECT * FROM public.player_stats WITH NO DATA;
CREATE TEMP TABLE team_stats AS SELECT * FROM public.team_stats WITH NO DATA;

-- A single eligible NBA player: no comparable population, no percentile, no z.
INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,0,'NBA',2097,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"blk":2.0,"turnover":2.6,"pf":1.9,"fta":5.5,"plus_minus":3.4,"stl":1.1,"fg3m":1.1,"minutes":34.0,"games_played":70}');

SELECT public.compute_rating('NBA',2097);

DO $check$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE (d->>'eligible') = 'true' AND d->>'z' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Degenerate population stored a fabricated z'; END IF;
    IF (SELECT count(*) FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE (d->>'eligible') = 'true' AND d->>'pct' IS NULL AND d->>'value' IS NOT NULL) <> 10
        THEN RAISE EXCEPTION 'Degenerate population lost an observation'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps WHERE ps.rating IS NULL
    ) THEN RAISE EXCEPTION 'Degenerate z withholding erased the composite'; END IF;
END $check$;

-- Two eligible players with distinct values: defined spread keeps z AND percentile.
DELETE FROM pg_temp.player_stats;
INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,0,'NBA',2097,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"blk":2.0,"turnover":2.6,"pf":1.9,"fta":5.5,"plus_minus":3.4,"stl":1.1,"fg3m":1.1,"minutes":34.0,"games_played":70}'),
(-2,0,'NBA',2097,'C','{"pts":14.1,"reb":5.4,"ast":3.2,"blk":0.9,"turnover":1.6,"pf":2.1,"fta":2.5,"plus_minus":0.4,"stl":0.8,"fg3m":0.7,"minutes":30.0,"games_played":55}');

SELECT public.compute_rating('NBA',2097);

DO $check$
BEGIN
    IF (SELECT count(*) FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE (d->>'eligible') = 'true' AND d->>'z' IS NULL AND d->>'pct' IS NOT NULL) <> 0
        THEN RAISE EXCEPTION 'Defined-spread population lost its z'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id = -1 AND d->>'label' = 'Scoring'
          AND (d->>'pct') IS NULL
    ) THEN RAISE EXCEPTION 'Defined-spread percentile was withheld'; END IF;
END $check$;

-- All-tied values: spread undefined again; z withheld, observations retained.
DELETE FROM pg_temp.player_stats;
INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,0,'NBA',2097,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"blk":2.0,"turnover":2.6,"pf":1.9,"fta":5.5,"plus_minus":3.4,"stl":1.1,"fg3m":1.1,"minutes":34.0,"games_played":70}'),
(-2,0,'NBA',2097,'C','{"pts":25.1,"reb":9.4,"ast":6.2,"blk":2.0,"turnover":2.6,"pf":1.9,"fta":5.5,"plus_minus":3.4,"stl":1.1,"fg3m":1.1,"minutes":34.0,"games_played":70}');

SELECT public.compute_rating('NBA',2097);

DO $check$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE (d->>'eligible') = 'true' AND d->>'z' IS NOT NULL
    ) THEN RAISE EXCEPTION 'All-tied population stored a fabricated z'; END IF;
    IF (SELECT count(*) FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE d->>'value' IS NOT NULL) <> 20
        THEN RAISE EXCEPTION 'All-tied population lost observations'; END IF;
END $check$;

-- Team: single team season — every population is degenerate.
INSERT INTO pg_temp.team_stats(team_id,league_id,sport,season,stats) VALUES
(-1,0,'NBA',2097,'{"pts":114.2,"ast":26.0,"fg3m":13.1,"fta":21.0,"turnover":13.5,"oreb":10.2,"blk":5.1,"stl":8.0,"reb":45.0,"pts_allowed":110.0,"dreb":34.8,"def_fg_pct":46.0,"def_fg3_pct":35.0}');

SELECT public.compute_team_rating('NBA',2097);

DO $team$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_temp.team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE d->>'z' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Single-team population stored a fabricated z'; END IF;
    IF (SELECT count(*) FROM pg_temp.team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d) <> 13
        THEN RAISE EXCEPTION 'Team observations lost'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.team_stats ts WHERE ts.rating IS NULL
    ) THEN RAISE EXCEPTION 'Team composite lost'; END IF;
END $team$;
ROLLBACK;
