-- Read-only production report for the latest completed nightly news ingest.
-- Run with: psql "$DATABASE_PRIVATE_URL" -X -f rust/examples/harvester_nightly_report.sql
-- Counts and times only; no publisher text or URLs leave PostgreSQL.
WITH ingest AS (
    SELECT id, started_at, finished_at, status, attempted, succeeded, failed
      FROM public.pipeline_runs
     WHERE job = 'pipeline' AND finished_at IS NOT NULL
     ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id, p.entity_type, p.entity_id, p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at >= i.started_at
       AND p.last_seen_at <= i.finished_at
), article AS (
    SELECT DISTINCT article_id FROM cohort
), classification AS (
    SELECT c.* FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
), acquisition AS (
    SELECT a.status FROM public.harvester_acquisitions a JOIN article x USING (article_id)
), assignment AS (
    SELECT d.plugin_id,d.status FROM public.harvester_assignments d
      JOIN classification c ON c.id=d.classification_id
)
SELECT i.id AS ingest_run_id, i.started_at AS ingest_started_at,
       i.finished_at AS ingest_finished_at, i.status AS ingest_status,
       round(extract(epoch FROM i.finished_at-i.started_at)::numeric,1) AS ingest_seconds,
       i.attempted AS ingest_attempted, i.succeeded AS ingest_succeeded,
       i.failed AS ingest_failed,
       (SELECT count(*) FROM article) AS canonical_candidates,
       (SELECT count(*) FROM cohort) AS query_entity_edges,
       (SELECT count(DISTINCT article_id) FROM public.harvester_query_provenance p
         WHERE p.first_seen_at BETWEEN i.started_at AND i.finished_at) AS first_seen_candidates,
       (SELECT count(*) FROM acquisition WHERE status='acquired') AS acquired,
       (SELECT count(*) FROM acquisition WHERE status IN ('retryable_error','blocked','low_content')) AS acquisition_errors,
       (SELECT count(*) FROM acquisition WHERE status='classification_error') AS laya_errors,
       (SELECT count(*) FROM classification) AS classified_edges,
       (SELECT count(*) FROM classification WHERE entity_choice='relevant') AS laya_entity_relevant,
       (SELECT count(*) FROM classification WHERE entity_choice='irrelevant') AS laya_entity_irrelevant,
       (SELECT count(*) FROM cohort)-(SELECT count(*) FROM classification) AS unclassified_edges,
       (SELECT count(*) FROM assignment) AS character_assignments,
       (SELECT count(*) FROM assignment WHERE status='pending') AS pending_assignments,
       (SELECT count(*) FROM public.pipeline_work w JOIN article a ON a.article_id=w.entity_id
         WHERE w.stage='harvester' AND w.entity_type='article') AS outstanding_harvester_work,
       (SELECT max(created_at) FROM classification) AS last_classified_at,
       round(extract(epoch FROM ((SELECT max(created_at) FROM classification)-i.started_at))::numeric,1)
           AS seconds_from_ingest_start_to_last_classification,
       (SELECT round(sum((model_provenance->'relevance'->>'inference_ms')::numeric
                         +(model_provenance->'character_routing'->>'inference_ms')::numeric),1)
          FROM classification) AS laya_inference_ms_total
  FROM ingest i;

WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classification AS (
    SELECT c.id,c.distributions,c.model_provenance FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
)
SELECT d.plugin_id, count(*) AS assignments,
       count(*) FILTER (WHERE d.status='used') AS used,
       count(*) FILTER (WHERE d.status='irrelevant') AS irrelevant,
       count(*) FILTER (WHERE d.status='relevant_but_unused') AS relevant_but_unused,
       count(*) FILTER (WHERE d.status='redundant') AS redundant,
       count(*) FILTER (WHERE d.status='abstained') AS abstained,
       count(*) FILTER (WHERE d.status='error') AS errors,
       count(*) FILTER (WHERE d.status='pending') AS pending
  FROM public.harvester_assignments d JOIN classification c ON c.id=d.classification_id
 GROUP BY d.plugin_id ORDER BY d.plugin_id;

-- Laya recommendations are advisory. Without adjudicated human labels, these
-- counts and downstream dispositions measure behavior, not precision or recall.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classified AS (
    SELECT c.distributions,c.model_provenance FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
)
SELECT count(*) AS classified_edges,
       count(*) FILTER (WHERE (
           SELECT count(*) FROM jsonb_each(distributions) x
            WHERE x.value->>'choice'='relevant')=4) AS all_four_recommended,
       round(avg((SELECT count(*) FROM jsonb_each(distributions) x
                   WHERE x.value->>'choice'='relevant'))::numeric,2) AS mean_recommended_fanout,
       round(avg((model_provenance->'relevance'->>'inference_ms')::numeric
                +(model_provenance->'character_routing'->>'inference_ms')::numeric),1) AS mean_laya_ms,
       round(percentile_cont(0.95) WITHIN GROUP (ORDER BY
           (model_provenance->'relevance'->>'inference_ms')::numeric
           +(model_provenance->'character_routing'->>'inference_ms')::numeric)::numeric,1) AS p95_laya_ms
  FROM classified;

WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
)
SELECT route.key AS plugin_id, count(*) AS classified_edges,
       count(*) FILTER (WHERE route.value->>'choice'='relevant') AS laya_recommended,
       count(*) FILTER (WHERE route.value->>'choice'='irrelevant') AS laya_not_recommended
  FROM public.harvester_classifications c
  JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
   AND q.entity_id=c.entity_id AND q.sport=c.sport
 CROSS JOIN LATERAL jsonb_each(c.distributions) route
 GROUP BY route.key ORDER BY route.key;
