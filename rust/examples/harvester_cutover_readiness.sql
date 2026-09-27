-- Read-only Harvester cutover checks. A live cutover requires zero pending/error
-- assignments for the observed cohort and no unaccounted acquisition failures.
WITH cohort AS (
    SELECT p.article_id, p.entity_type, p.entity_id, p.sport
      FROM public.harvester_query_provenance p
     WHERE p.first_seen_at >= now() - interval '24 hours'
), articles AS (
    SELECT DISTINCT article_id FROM cohort
)
SELECT 'canonical_articles' AS measure, count(*)::bigint AS total FROM articles
UNION ALL
SELECT 'missing_acquisition_state', count(*) FROM articles a
 WHERE NOT EXISTS (SELECT 1 FROM public.harvester_acquisitions h WHERE h.article_id=a.article_id)
UNION ALL
SELECT 'classified_entity_edges', count(*) FROM cohort q
 WHERE EXISTS (
    SELECT 1 FROM public.harvester_classifications c
     WHERE c.article_id=q.article_id AND c.entity_type=q.entity_type
       AND c.entity_id=q.entity_id AND c.sport=q.sport
 )
UNION ALL
SELECT 'unclassified_entity_edges', count(*) FROM cohort q
 WHERE NOT EXISTS (
    SELECT 1 FROM public.harvester_classifications c
     WHERE c.article_id=q.article_id AND c.entity_type=q.entity_type
       AND c.entity_id=q.entity_id AND c.sport=q.sport
 )
UNION ALL
SELECT 'acquisition_errors', count(*) FROM articles a
 JOIN public.harvester_acquisitions h USING (article_id)
 WHERE h.status IN ('retryable_error','blocked','low_content','classification_error')
ORDER BY measure;

SELECT d.plugin_id, d.status, count(*) AS total
  FROM public.harvester_assignments d
  JOIN public.harvester_classifications c ON c.id=d.classification_id
  JOIN public.harvester_query_provenance q
    ON q.article_id=c.article_id AND q.entity_type=c.entity_type
   AND q.entity_id=c.entity_id AND q.sport=c.sport
 WHERE q.first_seen_at >= now() - interval '24 hours'
 GROUP BY d.plugin_id, d.status
 ORDER BY d.plugin_id, d.status;

-- Character handoffs can finish while their internal Inspector/Scout obligations
-- are still outstanding. Keep those receipts visible before any switch.
SELECT 'insider_identity_review' AS obligation, r.status, count(*) AS total
  FROM public.harvester_insider_identity_reviews r
  JOIN public.harvester_classifications c ON c.id=r.classification_id
 WHERE c.created_at >= now() - interval '24 hours'
 GROUP BY r.status
UNION ALL
SELECT 'unresolved_name', n.reason, count(*)
  FROM public.harvester_unresolved_names n
 WHERE n.created_at >= now() - interval '24 hours'
 GROUP BY n.reason
UNION ALL
SELECT 'insider_scored_wrap', w.status, count(*)
  FROM public.harvester_insider_wraps w
 WHERE w.updated_at >= now() - interval '24 hours'
 GROUP BY w.status
UNION ALL
SELECT 'insider_source_pair', p.status, count(*)
  FROM public.harvester_insider_pairs p
  JOIN public.harvester_classifications c ON c.id=p.classification_id
 WHERE c.created_at >= now() - interval '24 hours'
 GROUP BY p.status
ORDER BY obligation,status;

-- A claim backlog and an old pending assignment are operationally distinct.
SELECT stage,status,count(*) AS total,
       min(updated_at) AS oldest_updated_at
  FROM public.pipeline_work
 WHERE stage IN ('harvester','graph','narratives','vibe','transfers','rating')
   AND updated_at >= now() - interval '48 hours'
 GROUP BY stage,status
 ORDER BY stage,status;
