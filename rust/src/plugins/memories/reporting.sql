-- Graph locates source articles. Source text must name the requested entity;
-- graph predicates and generated storyline titles never become observations.
WITH candidates AS (
 SELECT DISTINCT e.article_id FROM narrative_events e
 WHERE e.sport=$1 AND e.origin='extraction'
 AND ((e.subject_type=$2 AND e.subject_id=$3) OR (e.object_type=$2 AND e.object_id=$3))
 AND ($8::integer IS NULL OR
 (e.subject_type=$2 AND e.subject_id=$3 AND e.object_type=$9 AND e.object_id=$8) OR
 (e.object_type=$2 AND e.object_id=$3 AND e.subject_type=$9 AND e.subject_id=$8))
 AND (cardinality($11::text[])=0 OR e.predicate=ANY($11::text[]))
 AND e.event_date>=to_timestamp($5::double precision)
 AND e.event_date<to_timestamp($6::double precision)
), surfaces AS (
 SELECT public.nrm($4) norm
 UNION SELECT norm FROM entity_name_surfaces WHERE sport=$1 AND entity_type=$2 AND entity_id=$3
), pair_surfaces AS (
 SELECT public.nrm($10) norm
 UNION SELECT norm FROM entity_name_surfaces WHERE sport=$1 AND entity_type=$9 AND entity_id=$8
), eligible AS (
 SELECT a.* FROM candidates c JOIN news_articles a ON a.id=c.article_id
 WHERE a.published_at>=to_timestamp($5::double precision)
 AND a.published_at<to_timestamp($6::double precision)
 AND NOT (a.id=ANY($7::bigint[]))
 AND NOT (COALESCE(a.duplicate_of,a.id)=ANY($7::bigint[]))
 AND NOT EXISTS(SELECT 1 FROM news_articles f WHERE f.id=ANY($7::bigint[])
     AND COALESCE(f.duplicate_of,f.id)=COALESCE(a.duplicate_of,a.id))
 AND COALESCE(a.source,'')<>'' AND a.title<>''
 AND ($8::integer IS NULL OR EXISTS(SELECT 1 FROM pair_surfaces s WHERE s.norm<>'' AND
 strpos(' '||public.nrm(a.title)||' ',' '||s.norm||' ')>0))
 AND EXISTS(SELECT 1 FROM surfaces s WHERE s.norm<>'' AND
 strpos(' '||public.nrm(a.title)||' ',' '||s.norm||' ')>0)
)
SELECT jsonb_build_object('article_id',a.id,'canonical_id',COALESCE(a.duplicate_of,a.id),
 'topic',CASE WHEN $8::integer IS NOT NULL THEN 'requested_pair' ELSE COALESCE('storyline/'||(SELECT min(sa.storyline_id)::text
 FROM storyline_articles sa JOIN storyline_entities se ON se.storyline_id=sa.storyline_id
 WHERE sa.article_id=a.id AND se.sport=$1 AND se.entity_type=$2 AND se.entity_id=$3),
 'article/'||COALESCE(a.duplicate_of,a.id)::text) END,
 'publisher',a.source,'reported_at',floor(extract(epoch FROM a.published_at))::bigint,
 'headline',a.title)::text
FROM eligible a ORDER BY a.id LIMIT 20001
