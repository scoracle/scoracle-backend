-- Canonical retained reporting, sampled without any model verdict or writer output.
-- psql -X -qAt -v ON_ERROR_STOP=1 -v sample_size=40 -f classifier_export.sql
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
WITH sources AS (
    SELECT a.id AS article_id, a.title, a.source, a.url, a.published_at,
           a.fetched_at, a.full_text AS body,
           jsonb_agg(DISTINCT jsonb_build_object(
               'entity_type', p.entity_type, 'entity_id', p.entity_id,
               'sport', p.sport, 'name', t.name)) AS query_entities,
           array_agg(DISTINCT original.id ORDER BY original.id) AS source_article_ids,
           min(p.sport) AS sample_sport
      FROM public.harvester_query_provenance p
      JOIN public.news_articles original ON original.id=p.article_id
      JOIN public.news_articles a ON a.id=COALESCE(original.duplicate_of, original.id)
      LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
     WHERE length(a.full_text)>200
     GROUP BY a.id
), sampled AS (
    SELECT *, row_number() OVER (
        PARTITION BY sample_sport, source,
                     CASE WHEN length(body)<2000 THEN 'short'
                          WHEN length(body)<8000 THEN 'medium' ELSE 'long' END
        ORDER BY md5(article_id::text)) AS sample_rank
      FROM sources
)
SELECT jsonb_build_object(
    'article_id', article_id, 'title', title, 'source', source, 'url', url,
    'published_at', published_at, 'fetched_at', fetched_at, 'body', body,
    'source_article_ids', source_article_ids, 'query_entities', query_entities)
  FROM sampled
 ORDER BY sample_rank, md5(article_id::text)
 LIMIT :sample_size;
COMMIT;
