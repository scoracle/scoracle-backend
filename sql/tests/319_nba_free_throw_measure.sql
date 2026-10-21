BEGIN;
DO $$ BEGIN
 IF (SELECT (measure,value) FROM public.rating_measurements('NBA','{"fta":4.4,"fga":12.3}'::jsonb) WHERE label='Foul Drawing')
 IS DISTINCT FROM ROW('free throws attempted'::text,4.4::numeric) THEN
 RAISE EXCEPTION 'Player Foul Drawing must describe fta, not fga'; END IF;
 IF (SELECT (measure,value) FROM public.rating_measurements_team('NBA','{"fta":20.1,"fga":88.3}'::jsonb) WHERE label='Foul Drawing')
 IS DISTINCT FROM ROW('free throws attempted'::text,20.1::numeric) THEN
 RAISE EXCEPTION 'Team Foul Drawing must describe fta, not fga'; END IF;
 IF (SELECT (measure,value) FROM public.rating_measurements('NBA','{"fta":4.4,"fta_per_36":5.5,"minutes":28.0}'::jsonb,'per_36') WHERE label='Foul Drawing')
 IS DISTINCT FROM ROW('free throws attempted'::text,5.5::numeric) THEN
 RAISE EXCEPTION 'Rate mode must retain free-throw identity'; END IF;
END $$;
ROLLBACK;
