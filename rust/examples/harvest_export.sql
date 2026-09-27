-- Read-only frozen trial corpus. Run with psql -X -qAt -v ON_ERROR_STOP=1
-- -v sweep_date=2026-09-24 -v sample_size=120 -f examples/harvest_export.sql
-- Use an existing secure database connection; never put credentials in artifacts.
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT json_build_object(
    'article_id', a.id, 'title', a.title, 'source', a.source, 'url', a.url,
    'feed_rank', a.feed_rank,
    'published_at', a.published_at, 'fetched_at', a.fetched_at, 'body', a.full_text,
    'hypothesis', json_build_object('name', t.name, 'entity_type', 'team',
                                  'entity_id', t.id, 'sport', t.sport),
    'baseline', json_build_object('status', er.status, 'model', er.model_version,
                                 'contract', er.contract_version, 'read', er.read))
FROM public.news_articles a
JOIN public.editor_reads er ON er.article_id = a.id
JOIN public.teams t ON t.id = (a.raw->>'query_team_id')::int
                  AND t.sport = upper(a.raw->>'query_sport')
WHERE a.fetched_at >= (:'sweep_date'::date::timestamp AT TIME ZONE 'America/Detroit')
  AND a.fetched_at < ((:'sweep_date'::date + 1)::timestamp AT TIME ZONE 'America/Detroit')
  AND length(a.full_text) > 200
  AND a.duplicate_of IS NULL
ORDER BY md5(a.id::text)
LIMIT :sample_size;
COMMIT;
