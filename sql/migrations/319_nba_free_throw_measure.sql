-- Foul Drawing uses fta (free throws), not fga (field goals).
BEGIN;
SET LOCAL lock_timeout='5s';
SET LOCAL statement_timeout='60s';
DO $functions$
DECLARE signature text; definition text;
BEGIN
    FOREACH signature IN ARRAY ARRAY['public.rating_measurements(text,jsonb,text,text)',
        'public.rating_measurements_team(text,jsonb)'] LOOP
        definition := pg_get_functiondef(signature::regprocedure);
        IF length(definition)-length(replace(definition,
            $$WHEN 'Foul Drawing' THEN 'field goals attempted'$$,''))
            <> length($$WHEN 'Foul Drawing' THEN 'field goals attempted'$$) THEN
            RAISE EXCEPTION 'Unexpected Foul Drawing definition: %',signature;
        END IF;
        EXECUTE replace(definition,$$WHEN 'Foul Drawing' THEN 'field goals attempted'$$,
            $$WHEN 'Foul Drawing' THEN 'free throws attempted'$$);
    END LOOP;
END $functions$;

CREATE TEMP TABLE _nba_player_labels ON COMMIT DROP AS
SELECT player_id,league_id,sport,season,to_jsonb(p) before FROM public.player_stats p WHERE sport='NBA';
CREATE TEMP TABLE _nba_team_labels ON COMMIT DROP AS
SELECT team_id,league_id,sport,season,to_jsonb(t) before FROM public.team_stats t WHERE sport='NBA';

UPDATE public.player_stats SET
 rating_breakdown=replace(rating_breakdown::text,'"measure": "field goals attempted"','"measure": "free throws attempted"')::jsonb,
 rating_modes=replace(rating_modes::text,'"measure": "field goals attempted"','"measure": "free throws attempted"')::jsonb
WHERE sport='NBA' AND (rating_breakdown::text LIKE '%"measure": "field goals attempted"%'
 OR rating_modes::text LIKE '%"measure": "field goals attempted"%');
UPDATE public.team_stats SET
 rating_breakdown=replace(rating_breakdown::text,'"measure": "field goals attempted"','"measure": "free throws attempted"')::jsonb
WHERE sport='NBA' AND rating_breakdown::text LIKE '%"measure": "field goals attempted"%';

DO $parity$
BEGIN
 IF EXISTS(SELECT 1 FROM public.player_stats p JOIN _nba_player_labels b ON p.player_id=b.player_id AND p.league_id IS NOT DISTINCT FROM b.league_id AND p.sport=b.sport AND p.season=b.season
  WHERE to_jsonb(p) IS DISTINCT FROM replace(b.before::text,
   '"measure": "field goals attempted"','"measure": "free throws attempted"')::jsonb)
 OR EXISTS(SELECT 1 FROM public.team_stats t JOIN _nba_team_labels b ON t.team_id=b.team_id AND t.league_id IS NOT DISTINCT FROM b.league_id AND t.sport=b.sport AND t.season=b.season
  WHERE to_jsonb(t) IS DISTINCT FROM replace(b.before::text,
   '"measure": "field goals attempted"','"measure": "free throws attempted"')::jsonb) THEN
 RAISE EXCEPTION 'Free-throw label correction changed other stored data'; END IF;
END $parity$;
INSERT INTO public.schema_migrations(version) VALUES('319_nba_free_throw_measure') ON CONFLICT DO NOTHING;
COMMIT;
