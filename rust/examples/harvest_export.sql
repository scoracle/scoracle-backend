-- Read-only sample of retained source/query edges from Harvester provenance.
-- psql -X -qAt -v ON_ERROR_STOP=1 -v sweep_date=2026-09-27 -v sample_size=120
--   -f examples/harvest_export.sql
-- No Editor reads or teacher answers select or accompany the evidence.
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
WITH candidates AS (
    SELECT DISTINCT ON (a.id, p.sport, p.entity_id)
           a.id, a.title,
           COALESCE(a.source, '') AS source, a.url, a.published_at,
           COALESCE(a.full_text, '') AS body, p.feed_rank,
           t.name, p.entity_id, p.sport
      FROM public.harvester_query_provenance p
      JOIN public.news_articles original ON original.id=p.article_id
      JOIN public.news_articles a ON a.id=COALESCE(original.duplicate_of, original.id)
      JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
     WHERE original.fetched_at >= (:'sweep_date'::date::timestamp AT TIME ZONE 'America/Detroit')
       AND original.fetched_at < ((:'sweep_date'::date + 1)::timestamp AT TIME ZONE 'America/Detroit')
       AND length(a.full_text) > 200
     ORDER BY a.id, p.sport, p.entity_id, p.feed_rank NULLS LAST
)
SELECT json_build_object(
    'article_id', id, 'title', title,
    'source', source, 'url', url, 'published_at', published_at,
    'body', body, 'feed_rank', feed_rank,
    'hypothesis', json_build_object('name', name, 'entity_type', 'team',
                                  'entity_id', entity_id, 'sport', sport)
)
FROM candidates
ORDER BY md5(id::text || ':' || sport || ':' || entity_id::text)
LIMIT :sample_size;
COMMIT;
