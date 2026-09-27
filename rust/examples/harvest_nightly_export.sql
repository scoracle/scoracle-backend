-- Read-only export of every Google team-query edge in one Detroit-local sweep day.
-- psql -X -qAt -v ON_ERROR_STOP=1 -v sweep_date=2026-09-26 \
--   -f examples/harvest_nightly_export.sql > /tmp/harvester-nightly.jsonl
-- Duplicate rows retain their distinct team hypotheses and best feed rank but
-- point at the canonical article ID/body; acquisition fetches that URL once.
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT DISTINCT ON (canonical.id, team.sport, team.id) json_build_object(
    'article_id', canonical.id,
    'title', canonical.title,
    'description', canonical.description,
    'source', canonical.source,
    'url', canonical.url,
    'feed_rank', query.feed_rank,
    'published_at', canonical.published_at,
    'body', coalesce(canonical.full_text, ''),
    'hypothesis', json_build_object(
        'name', team.name,
        'entity_type', 'team',
        'entity_id', team.id,
        'sport', team.sport
    )
)
FROM public.news_articles query
JOIN public.news_articles canonical
  ON canonical.id = coalesce(query.duplicate_of, query.id)
JOIN public.teams team
  ON team.id = (query.raw->>'query_team_id')::int
 AND team.sport = upper(query.raw->>'query_sport')
WHERE query.fetched_at >= (:'sweep_date'::date::timestamp AT TIME ZONE 'America/Detroit')
  AND query.fetched_at < ((:'sweep_date'::date + 1)::timestamp AT TIME ZONE 'America/Detroit')
ORDER BY canonical.id, team.sport, team.id, query.feed_rank NULLS LAST;
COMMIT;
