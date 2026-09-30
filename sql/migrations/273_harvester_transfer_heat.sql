-- Transfer heat computed from source-grounded Harvester identity links.
-- The Editor-era function and its callers remain unchanged during shadowing.
BEGIN;
CREATE FUNCTION public.compute_harvester_transfer_heat(
    p_team_id integer, p_subject_id integer, p_sport text, p_subject_type text,
    OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]
) RETURNS record LANGUAGE sql STABLE AS $$
    WITH corpus AS (
        SELECT a.id, a.source AS src, COALESCE(a.published_at,a.fetched_at) AS ts
          FROM public.news_articles a
          JOIN public.harvester_resolved_links te
            ON te.article_id=a.id AND te.entity_type='team'
           AND te.entity_id=p_team_id AND te.sport=p_sport
          JOIN public.harvester_resolved_links pe
            ON pe.article_id=a.id AND pe.entity_type=p_subject_type
           AND pe.entity_id=p_subject_id AND pe.sport=p_sport
         WHERE a.duplicate_of IS NULL
           AND a.bucket IS DISTINCT FROM 'non_transfer'
           AND COALESCE(a.published_at,a.fetched_at) > now() - interval '14 days'
    ), agg AS (
        SELECT count(DISTINCT src) AS distinct_sources,
               count(*) FILTER (WHERE ts > now() - interval '3 days') AS recent3,
               count(*) AS total, max(ts) AS newest
          FROM corpus
    ), calc AS (
        SELECT *, extract(epoch FROM (now()-newest))/3600.0 AS age_hours,
               least(1.0,distinct_sources::numeric/5.0) AS volume,
               recent3::numeric/greatest(total,1) AS recent_frac
          FROM agg
    ), fin AS (
        SELECT *,exp(-age_hours/72.0) AS recency FROM calc
    )
    SELECT CASE WHEN total=0 THEN NULL
                ELSE greatest(0,least(100,
                     round(100*recency*(0.6*volume+0.4*recent_frac))))::smallint END,
           CASE WHEN total=0 THEN '{}'::jsonb
                ELSE jsonb_build_object(
                    'distinct_sources',distinct_sources,'recent_3d',recent3,
                    'total_14d',total,'newest_age_hours',round(age_hours::numeric,1),
                    'volume',round(volume::numeric,3),
                    'recency',round(recency::numeric,3),
                    'recent_frac',round(recent_frac::numeric,3),
                    'identity_source','harvester_resolved_links') END,
           COALESCE((SELECT array_agg(id ORDER BY ts DESC,id DESC) FROM corpus),'{}'::bigint[])
      FROM fin;
$$;
COMMENT ON FUNCTION public.compute_harvester_transfer_heat(integer,integer,text,text) IS
    'Source-diversity and recency heat from strict Harvester resolved co-mentions; isolated from legacy Editor link ownership.';
INSERT INTO public.schema_migrations(version)
VALUES ('273_harvester_transfer_heat') ON CONFLICT DO NOTHING;
COMMIT;
