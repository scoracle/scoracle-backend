-- Read-only export of every canonical Google candidate for selected entity ids.
-- psql -X -qAt -v ON_ERROR_STOP=1 -v sweep_date=2026-09-26 \
--   -v 'entity_names=Chelsea|Dallas Cowboys|Detroit Pistons' \
--   -f examples/harvest_focus_export.sql
--
-- Google headline/description exists before publisher acquisition. full_text is included
-- when an earlier worker already retained it so a replay need not fetch it again.
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT json_build_object(
    'article_id', a.id,
    'title', a.title,
    'description', a.description,
    'source', a.source,
    'url', a.url,
    'feed_rank', a.feed_rank,
    'published_at', a.published_at,
    'body', coalesce(a.full_text, ''),
    'hypothesis', json_build_object(
        'name', t.name,
        'entity_type', 'team',
        'entity_id', t.id,
        'sport', t.sport
    )
)
FROM public.news_articles a
JOIN public.teams t
  ON t.id = (a.raw->>'query_team_id')::int
 AND t.sport = upper(a.raw->>'query_sport')
WHERE a.fetched_at >= (:'sweep_date'::date::timestamp AT TIME ZONE 'America/Detroit')
  AND a.fetched_at < ((:'sweep_date'::date + 1)::timestamp AT TIME ZONE 'America/Detroit')
  AND t.name = ANY(string_to_array(:'entity_names', '|'))
  AND a.duplicate_of IS NULL
ORDER BY t.name, a.feed_rank NULLS LAST, a.id;
COMMIT;
