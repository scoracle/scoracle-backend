-- Run after migrations 252/253, or with their functions shadowed in pg_temp.
-- Every write targets explicitly temporary copies; production statistics are untouched.
BEGIN;
SET LOCAL search_path = pg_temp, public;
CREATE TEMP TABLE player_stats AS SELECT * FROM public.player_stats WITH NO DATA;
CREATE TEMP TABLE team_stats AS SELECT * FROM public.team_stats WITH NO DATA;

INSERT INTO pg_temp.player_stats(player_id,league_id,sport,season,position,stats) VALUES
(-1,8,'FOOTBALL',2099,'Midfielder','{"appearances":20,"shots_on_target":2,"assists":2}'),
(-2,8,'FOOTBALL',2099,'Midfielder','{"appearances":20,"shots_on_target":4,"assists":4}'),
(-3,8,'FOOTBALL',2099,'Midfielder','{"appearances":20,"expected_goals":0.2,"assists":2}'),
(-4,8,'FOOTBALL',2099,'Midfielder','{"appearances":20,"expected_goals":0.4,"assists":4}'),
(-5,8,'FOOTBALL',2099,'Midfielder','{"appearances":1,"expected_goals":0,"expected_assists":1.01,"assists":0}');
SELECT public.compute_rating('FOOTBALL',2099);

DO $check$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-5 AND (d->>'pct' IS NOT NULL OR d->'scoped_pct'<>'{}'::jsonb)
    ) THEN RAISE EXCEPTION 'Ineligible player received a rank'; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-5 AND d->>'measure'='expected goals' AND (d->>'value')::numeric=0 AND d->>'pct' IS NULL
    ) THEN RAISE EXCEPTION 'Measured zero was lost'; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id=-5 AND d->>'measure'='expected assists' AND (d->>'value')::numeric=1.01 AND d->>'z' IS NULL
    ) THEN RAISE EXCEPTION 'Raw measure without any eligible cohort was lost or given a fake baseline'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id IN (-1,-2,-3,-4) AND d->>'label'='Shooting'
          AND (d->>'pct')::numeric IS DISTINCT FROM CASE WHEN ps.player_id IN (-1,-3) THEN 0::numeric ELSE 100::numeric END
    ) THEN RAISE EXCEPTION 'Different Shooting measurements share a ranked population'; END IF;
    IF (SELECT count(*) FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE ps.player_id IN (-1,-2,-3,-4) AND d->>'label'='Shooting') <> 4
        THEN RAISE EXCEPTION 'Expected ranked Shooting observations missing'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps
        CROSS JOIN LATERAL jsonb_each(ps.rating_modes) m
        CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') d
        WHERE ps.player_id=-5 AND d->>'pct' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Rate mode relabeled an ineligible sample as ranked'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE d->>'value' IS NULL OR COALESCE(d->>'measure','')=''
    ) THEN RAISE EXCEPTION 'Missing value or measurement identity entered stored breakdown'; END IF;
END $check$;

INSERT INTO pg_temp.team_stats(team_id,league_id,sport,season,stats) VALUES
(-1,8,'FOOTBALL',2099,'{"big_chances_created":2,"expected_goals_for":1}'),
(-2,8,'FOOTBALL',2099,'{"big_chances_created":4,"expected_goals_for":2}'),
(-3,8,'FOOTBALL',2099,'{"assists":0.2,"expected_goals_for":1}'),
(-4,8,'FOOTBALL',2099,'{"assists":0.4,"expected_goals_for":2}');
SELECT public.compute_team_rating('FOOTBALL',2099);
DO $check$
BEGIN
    IF (SELECT count(*) FROM pg_temp.team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE d->>'label'='Creation') <> 4 THEN RAISE EXCEPTION 'Expected team Creation observations missing'; END IF;
    IF EXISTS (
        SELECT 1 FROM pg_temp.team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE d->>'label'='Creation'
          AND (d->>'pct')::numeric IS DISTINCT FROM CASE WHEN ts.team_id IN (-1,-3) THEN 0::numeric ELSE 100::numeric END
    ) THEN RAISE EXCEPTION 'Different team Creation measurements share a ranked population'; END IF;
END $check$;
DO $rates$
BEGIN
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NBA','{"pts":24}', 'per_36','Guard'))
        THEN RAISE EXCEPTION 'Rate mode without a denominator survived'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NBA','{"pts":24,"minutes":30}', 'per_36','Guard') WHERE label='Scoring' AND value IS NOT NULL)
        THEN RAISE EXCEPTION 'NBA missing rate fell back to per-game average'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements('FOOTBALL','{"shots_on_target":32,"appearances":20}', 'per_game','Midfielder') WHERE label='Shooting' AND value IS NOT NULL)
        THEN RAISE EXCEPTION 'Football missing rate fell back to season total'; END IF;
    IF (SELECT (value,measure) IS NOT DISTINCT FROM (0.14::numeric,'expected goals'::text)
        FROM public.rating_measurements('FOOTBALL','{"shots_on_target":32,"expected_goals_per_90":0.14,"minutes_played":90}', 'per_90','Midfielder') WHERE label='Shooting') IS NOT TRUE
        THEN RAISE EXCEPTION 'Rate measurement does not follow the value actually selected'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NFL','{"passing_yards":100,"games_played":2,"tackles_for_loss":3}', 'per_game','QB')
        WHERE label IN ('Air Yards Responsible','Tackles For Loss') AND value IS NOT NULL)
        THEN RAISE EXCEPTION 'NFL missing rate became a total or zero'; END IF;
END $rates$;
ROLLBACK;
