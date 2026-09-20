-- Prefer histories belonging to the current article's story or exact pair. Only
-- older source articles are selected; generated storyline titles are not evidence.
WITH candidates AS (
 SELECT s.id,s.status,s.last_seen_at,
 EXISTS(SELECT 1 FROM public.storyline_articles x WHERE x.storyline_id=s.id AND x.article_id=ANY($4)) AS same_story
 FROM public.storylines s JOIN public.storyline_entities e ON e.storyline_id=s.id
 WHERE e.entity_type=$1 AND e.entity_id=$2 AND e.sport=$3 AND s.sport=$3
 AND s.last_seen_at>now()-INTERVAL '90 days'
 AND ($5::integer IS NULL OR EXISTS(SELECT 1 FROM public.storyline_entities p WHERE p.storyline_id=s.id AND p.entity_type='team' AND p.entity_id=$5 AND p.sport=$3))
 ORDER BY same_story DESC,s.last_seen_at DESC,s.id DESC LIMIT 2
)
SELECT a.id AS article_id,a.fetched_at::text AS observed_at,floor(extract(epoch FROM a.fetched_at))::bigint AS observed_unix,
 jsonb_build_object('report',a.title,'source',a.source,'reported_at',a.published_at,'story_id',s.id,'story_status',s.status) AS data
FROM candidates s JOIN LATERAL (
 SELECT n.* FROM public.storyline_articles sa JOIN public.news_articles n ON n.id=sa.article_id
 WHERE sa.storyline_id=s.id AND NOT n.id=ANY($4) AND n.duplicate_of IS NULL
 AND n.fetched_at<now()-INTERVAL '1 day'
 AND EXISTS(SELECT 1 FROM public.news_article_entities ne WHERE ne.article_id=n.id AND ne.entity_type=$1 AND ne.entity_id=$2 AND ne.sport=$3)
 AND ($5::integer IS NULL OR EXISTS(SELECT 1 FROM public.news_article_entities ne WHERE ne.article_id=n.id AND ne.entity_type='team' AND ne.entity_id=$5 AND ne.sport=$3))
 ORDER BY n.published_at ASC NULLS LAST,n.id LIMIT 1
) a ON true
ORDER BY s.same_story DESC,s.last_seen_at DESC,s.id DESC
