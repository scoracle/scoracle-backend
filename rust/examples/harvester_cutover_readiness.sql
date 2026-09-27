-- Read-only Harvester cutover checks for the latest completed nightly ingest.
-- A live cutover requires zero actionable work and no unaccounted failures.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL
     ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id, p.entity_type, p.entity_id, p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), articles AS (
    SELECT DISTINCT article_id FROM cohort
), gates AS (
    SELECT DISTINCT ON (g.article_id,g.entity_type,g.entity_id,g.sport) g.*
      FROM public.harvester_headline_gates g JOIN cohort q
        ON q.article_id=g.article_id AND q.entity_type=g.entity_type
       AND q.entity_id=g.entity_id AND q.sport=g.sport
     WHERE g.contract_version='harvest-headline-v3'
       AND g.policy_version='explicit-headline-read-p025-v2'
     ORDER BY g.article_id,g.entity_type,g.entity_id,g.sport,g.created_at DESC
), classified AS (
    SELECT c.article_id,c.entity_type,c.entity_id,c.sport
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v7'
)
SELECT 'canonical_articles' AS measure, count(*)::bigint AS total FROM articles
UNION ALL
SELECT 'missing_acquisition_state', count(*) FROM articles a
 WHERE EXISTS (SELECT 1 FROM gates g WHERE g.article_id=a.article_id AND g.admitted)
   AND NOT EXISTS (SELECT 1 FROM public.harvester_acquisitions h WHERE h.article_id=a.article_id)
UNION ALL
SELECT 'headline_gate_edges', count(*) FROM gates
UNION ALL
SELECT 'headline_rejected_edges', count(*) FROM gates WHERE NOT admitted
UNION ALL
SELECT 'headline_admitted_edges', count(*) FROM gates WHERE admitted
UNION ALL
SELECT 'missing_headline_gate_edges', count(*) FROM cohort q
 WHERE NOT EXISTS (SELECT 1 FROM gates g WHERE g.article_id=q.article_id
                     AND g.entity_type=q.entity_type AND g.entity_id=q.entity_id
                     AND g.sport=q.sport)
   AND NOT EXISTS (SELECT 1 FROM public.harvester_acquisitions h
                    WHERE h.article_id=q.article_id AND h.status='duplicate')
UNION ALL
SELECT 'classified_entity_edges', count(*) FROM classified
UNION ALL
SELECT 'unclassified_entity_edges', count(*) FROM gates g WHERE admitted
   AND NOT EXISTS (SELECT 1 FROM classified c WHERE c.article_id=g.article_id
                     AND c.entity_type=g.entity_type AND c.entity_id=g.entity_id
                     AND c.sport=g.sport)
UNION ALL
SELECT 'acquisition_errors', count(*) FROM articles a
 JOIN public.harvester_acquisitions h USING (article_id)
 WHERE h.status IN ('retryable_error','blocked','low_content','classification_error')
ORDER BY measure;

WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL
     ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
)
SELECT d.plugin_id, d.status, count(*) AS total
  FROM public.harvester_assignments d
  JOIN public.harvester_classifications c ON c.id=d.classification_id
  JOIN cohort q
    ON q.article_id=c.article_id AND q.entity_type=c.entity_type
   AND q.entity_id=c.entity_id AND q.sport=c.sport
 WHERE c.contract_version='harvest-context-v7'
 GROUP BY d.plugin_id, d.status
 ORDER BY d.plugin_id, d.status;

-- Character handoffs can finish while their internal Inspector/Scout obligations
-- are still outstanding. Keep those receipts visible before any switch.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL
     ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), article_sport AS (
    SELECT DISTINCT article_id,sport FROM cohort
), teams AS (
    SELECT DISTINCT entity_id,sport FROM cohort WHERE entity_type='team'
), classified AS (
    SELECT c.id FROM public.harvester_classifications c JOIN cohort q
      ON q.article_id=c.article_id AND q.entity_type=c.entity_type
     AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v7'
)
SELECT 'insider_identity_review' AS obligation, r.status, count(*) AS total
  FROM public.harvester_insider_identity_reviews r
  JOIN classified c ON c.id=r.classification_id
 WHERE r.updated_at >= (SELECT started_at FROM ingest)
 GROUP BY r.status
UNION ALL
SELECT 'unresolved_name', n.reason, count(*)
  FROM public.harvester_unresolved_names n
  JOIN article_sport a ON a.article_id=n.article_id AND a.sport=n.sport
 WHERE n.created_at >= (SELECT started_at FROM ingest)
 GROUP BY n.reason
UNION ALL
SELECT 'insider_scored_wrap', w.status, count(*)
  FROM public.harvester_insider_wraps w
  JOIN teams t ON t.entity_id=w.team_id AND t.sport=w.sport
 WHERE w.updated_at >= (SELECT started_at FROM ingest)
 GROUP BY w.status
UNION ALL
SELECT 'insider_source_pair', p.status, count(*)
  FROM public.harvester_insider_pairs p
  JOIN classified c ON c.id=p.classification_id
 WHERE p.updated_at >= (SELECT started_at FROM ingest)
 GROUP BY p.status
ORDER BY obligation,status;

-- A claim backlog and an old pending assignment are operationally distinct.
SELECT stage,status,count(*) AS total,
       min(updated_at) AS oldest_updated_at
  FROM public.pipeline_work
 WHERE stage IN ('editor','harvester','graph','narratives','vibe','transfers','rating')
   AND (stage='editor' OR updated_at >= now() - interval '48 hours')
 GROUP BY stage,status
 ORDER BY stage,status;
