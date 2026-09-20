-- Studio owns context selection and rendering. These SQL prose builders have
-- no remaining application caller. Historical-only tables must be exported and
-- restore-verified before applying; no live character products are removed.
BEGIN;
DROP FUNCTION public.narrative_context_for_entity(text,text,integer);
DROP FUNCTION public.narrative_context_for_pair(text,integer,integer);
DROP FUNCTION public.stat_context_for_entity(text,text,integer,integer);
DROP FUNCTION public.refresh_latest_momentum_scores_per_entity();
CREATE OR REPLACE FUNCTION public.restamp_card_weeks() RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
    t text;
    v_total integer := 0;
    v_rows integer;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'news_summaries', 'vibe_scores', 'insider_scores', 'transfer_rumors',
        'momentum_summaries', 'stat_summaries', 'sigil_synthesis',
        'rating_history', 'momentum_scores'
    ] LOOP
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = sw.season, week_no = sw.week_no
              FROM public.season_weeks sw
             WHERE sw.sport = x.sport
               AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at
               AND (x.week_season IS DISTINCT FROM sw.season
                    OR x.week_no IS DISTINCT FROM sw.week_no)
        $f$, t);
        GET DIAGNOSTICS v_rows = ROW_COUNT;
        v_total := v_total + v_rows;
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = NULL, week_no = NULL
             WHERE x.week_no IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM public.season_weeks sw
                   WHERE sw.sport = x.sport
                     AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at)
        $f$, t);
        GET DIAGNOSTICS v_rows = ROW_COUNT;
        v_total := v_total + v_rows;
    END LOOP;
    RETURN v_total;
END;
$_$;

DROP TABLE public.oracle_readings;
DROP TABLE public.vibe_scores_echo_scrub_20260905;

INSERT INTO public.schema_migrations(version) VALUES ('263_sql_layer_cleanup') ON CONFLICT DO NOTHING;
COMMIT;
