-- The shared study locates source articles and emits one canonical article per
-- observation. Source text must name the requested entity. Generated storyline
-- titles, relationship predicates and junction-authored events never become
-- observations or topics here: a plugin supplies its own grouping through
-- memories::apply_topics, and a plugin that needs a narrower candidate set
-- passes it as the include list.
WITH candidates AS (
  SELECT DISTINCT e.article_id FROM narrative_events e
  WHERE e.sport=$1 AND e.origin='extraction'
  AND ((e.subject_type=$2 AND e.subject_id=$3) OR (e.object_type=$2 AND e.object_id=$3))
  AND e.event_date>=to_timestamp($5::double precision)
  AND e.event_date<to_timestamp($6::double precision)
), surfaces AS (
  SELECT public.nrm($4) norm
  UNION SELECT norm FROM entity_name_surfaces WHERE sport=$1 AND entity_type=$2 AND entity_id=$3
), eligible AS (
  SELECT a.* FROM candidates c JOIN news_articles a ON a.id=c.article_id
  WHERE a.published_at>=to_timestamp($5::double precision)
  AND a.published_at<to_timestamp($6::double precision)
  AND NOT (a.id=ANY($7::bigint[]))
  AND NOT (COALESCE(a.duplicate_of,a.id)=ANY($7::bigint[]))
  AND NOT EXISTS(SELECT 1 FROM news_articles f WHERE f.id=ANY($7::bigint[])
      AND COALESCE(f.duplicate_of,f.id)=COALESCE(a.duplicate_of,a.id))
  -- A caller-resolved candidate set narrows before the bound is applied, so a
  -- narrow request over a wide window is not truncated by the population guard.
  AND (cardinality($8::bigint[])=0 OR a.id=ANY($8::bigint[]))
  AND COALESCE(a.source,'')<>'' AND a.title<>''
  AND EXISTS(SELECT 1 FROM surfaces s WHERE s.norm<>'' AND
  strpos(' '||public.nrm(a.title)||' ', ' '||s.norm||' ')>0)
)
SELECT jsonb_build_object('article_id',a.id,'canonical_id',COALESCE(a.duplicate_of,a.id),
  'topic','article/'||COALESCE(a.duplicate_of,a.id)::text,
  'publisher',a.source,'reported_at',floor(extract(epoch FROM a.published_at))::bigint,
  'headline',a.title)::text
FROM eligible a ORDER BY a.id LIMIT 20001
