-- Read-only production report for the latest completed nightly news ingest.
-- Run with: psql "$DATABASE_PRIVATE_URL" -X -f rust/examples/harvester_nightly_report.sql
-- Counts and times only; no publisher text or URLs leave PostgreSQL.
-- Readiness metrics below count only the current Harvester contract. Historic
-- classifications remain visible in the final provenance breakdown, but do
-- not satisfy the current contract's coverage or assignment totals.
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
), headline_gate AS (
    SELECT DISTINCT ON (g.article_id,g.entity_type,g.entity_id,g.sport) g.*
      FROM public.harvester_headline_gates g
      JOIN cohort q ON q.article_id=g.article_id AND q.entity_type=g.entity_type
       AND q.entity_id=g.entity_id AND q.sport=g.sport
     WHERE g.contract_version='harvest-headline-v1'
       AND g.policy_version='headline-read-p025-v1'
     ORDER BY g.article_id,g.entity_type,g.entity_id,g.sport,g.created_at DESC
), classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.*
      FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), acquisition AS (
    SELECT a.article_id,a.status,a.updated_at
      FROM public.harvester_acquisitions a JOIN article x USING (article_id)
), assignment AS (
    SELECT d.plugin_id,d.status,d.reason FROM public.harvester_assignments d
      JOIN classification c ON c.id=d.classification_id
), work AS (
    SELECT w.entity_id AS article_id,w.sport,w.status,w.attempts,w.updated_at
      FROM public.pipeline_work w
      JOIN article a ON a.article_id=w.entity_id
     WHERE w.stage='harvester' AND w.entity_type='article'
       AND w.input_version LIKE 'harvest-context-v5:%'
), edge_state AS (
    SELECT q.article_id,q.entity_type,q.entity_id,q.sport,
           EXISTS (SELECT 1 FROM headline_gate g
                    WHERE g.article_id=q.article_id AND g.entity_type=q.entity_type
                      AND g.entity_id=q.entity_id AND g.sport=q.sport
                      AND NOT g.admitted) AS headline_rejected,
           EXISTS (SELECT 1 FROM classification c
                    WHERE c.article_id=q.article_id AND c.entity_type=q.entity_type
                      AND c.entity_id=q.entity_id AND c.sport=q.sport) AS classified,
           EXISTS (SELECT 1 FROM acquisition a
                    WHERE a.article_id=q.article_id AND a.status='duplicate')
           OR EXISTS (SELECT 1 FROM acquisition a JOIN work w
                        ON w.article_id=q.article_id AND w.sport=q.sport
                       WHERE a.article_id=q.article_id
                         AND a.status IN ('retryable_error','blocked','low_content','classification_error')
                         AND w.status='failed' AND w.attempts>=5) AS terminal_error
      FROM cohort q
)
SELECT 'harvest-context-v5' AS readiness_contract,
       i.id AS ingest_run_id, i.started_at AS ingest_started_at,
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
       (SELECT count(*) FROM article a WHERE EXISTS
         (SELECT 1 FROM headline_gate g WHERE g.article_id=a.article_id AND g.admitted)
         AND NOT EXISTS
         (SELECT 1 FROM acquisition x WHERE x.article_id=a.article_id)) AS missing_acquisition_state,
       (SELECT count(*) FROM headline_gate) AS headline_gate_edges,
       (SELECT count(*) FROM headline_gate WHERE admitted) AS headline_admitted_edges,
       (SELECT count(*) FROM headline_gate WHERE NOT admitted) AS headline_rejected_edges,
       (SELECT count(*) FROM headline_gate WHERE choice='irrelevant' AND admitted)
           AS plugin_admitted_laya_negative_edges,
       (SELECT count(*) FROM classification) AS classified_edges,
       (SELECT count(*) FROM classification WHERE entity_choice='relevant') AS laya_entity_relevant,
       (SELECT count(*) FROM classification WHERE entity_choice='irrelevant') AS laya_entity_irrelevant,
       (SELECT count(*) FROM edge_state WHERE NOT classified AND NOT headline_rejected)
           AS unclassified_edges,
       (SELECT count(*) FROM edge_state WHERE NOT classified AND NOT headline_rejected
             AND terminal_error)
           AS explicit_terminal_error_edges,
       (SELECT count(*) FROM edge_state WHERE NOT classified AND NOT headline_rejected
             AND NOT terminal_error)
           AS unaccounted_edges,
       (SELECT count(*) FROM assignment) AS character_assignments,
       (SELECT count(*) FROM assignment WHERE status='pending') AS pending_assignments,
       (SELECT count(*) FROM assignment WHERE reason='delivery_held') AS held_assignments,
       (SELECT count(*) FROM work WHERE status='pending') AS pending_harvester_work,
       (SELECT count(*) FROM work WHERE status='running') AS running_harvester_work,
       (SELECT count(*) FROM work WHERE status='failed' AND attempts<5) AS retry_scheduled,
       (SELECT count(*) FROM work WHERE status='failed' AND attempts>=5) AS dead_letters,
       (SELECT max(created_at) FROM classification) AS last_classified_at,
       round(extract(epoch FROM ((SELECT max(created_at) FROM classification)-i.started_at))::numeric,1)
           AS seconds_from_ingest_start_to_last_classification,
       CASE WHEN NOT EXISTS (
           SELECT 1 FROM work WHERE status IN ('pending','running')
              OR (status='failed' AND attempts<5)
       ) AND NOT EXISTS (
           SELECT 1 FROM edge_state WHERE NOT classified AND NOT headline_rejected
             AND NOT terminal_error
       ) AND NOT EXISTS (
           SELECT 1 FROM article a WHERE EXISTS
             (SELECT 1 FROM headline_gate g WHERE g.article_id=a.article_id AND g.admitted)
             AND NOT EXISTS
             (SELECT 1 FROM acquisition x WHERE x.article_id=a.article_id)
       ) THEN round(extract(epoch FROM (
           GREATEST((SELECT max(created_at) FROM classification),
                    (SELECT max(created_at) FROM headline_gate),
                    (SELECT max(updated_at) FROM acquisition),
                    (SELECT max(updated_at) FROM work WHERE status='failed'))
           - i.started_at))::numeric,1) END AS corpus_end_to_end_seconds,
       (SELECT round(sum((model_provenance->'relevance'->>'inference_ms')::numeric
                         +COALESCE((model_provenance->'character_routing'->>'inference_ms')::numeric,0)),1)
          FROM classification) AS laya_inference_ms_total
  FROM ingest i;

-- Acquisition failures stay separate from Laya negatives. Attempts include
-- each recorded fetch/classification try; no URL or publisher domain is shown.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), article AS (
    SELECT DISTINCT p.article_id FROM public.harvester_query_provenance p
      CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
)
SELECT a.status,count(*) AS articles,
       min(a.attempts) AS min_attempts,
       round(avg(a.attempts)::numeric,2) AS mean_attempts,
       max(a.attempts) AS max_attempts
  FROM public.harvester_acquisitions a JOIN article x USING (article_id)
 GROUP BY a.status ORDER BY articles DESC,a.status;

WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.id,c.entity_choice,c.distributions,c.model_provenance
      FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT d.plugin_id, count(*) AS assignments,
       count(*) FILTER (WHERE d.status='used') AS used,
       count(*) FILTER (WHERE d.status='irrelevant') AS irrelevant,
       count(*) FILTER (WHERE d.status='relevant_but_unused') AS relevant_but_unused,
       count(*) FILTER (WHERE d.status='redundant') AS redundant,
       count(*) FILTER (WHERE d.status='abstained') AS abstained,
       count(*) FILTER (WHERE d.status='error') AS errors,
       count(*) FILTER (WHERE d.status='pending') AS pending,
       count(*) FILTER (WHERE d.reason='delivery_held') AS held
  FROM public.harvester_assignments d JOIN classification c ON c.id=d.classification_id
 GROUP BY d.plugin_id ORDER BY d.plugin_id;

-- Once live character delivery starts, compare each selected Laya route with
-- the character's actual source use. This is assignment/triage evidence, not
-- precision or recall against independently adjudicated labels.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.*
      FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT d.plugin_id,count(*) AS assignments,
       count(*) FILTER (WHERE c.distributions->d.plugin_id->>'choice'='relevant')
           AS laya_recommended,
       count(*) FILTER (WHERE d.status='used') AS character_used,
       count(*) FILTER (WHERE c.distributions->d.plugin_id->>'choice'='relevant'
                         AND d.status='used') AS recommended_and_used,
       count(*) FILTER (WHERE c.distributions->d.plugin_id->>'choice'='irrelevant'
                         AND d.status='used') AS not_recommended_but_used,
       count(*) FILTER (WHERE d.status IN ('abstained','irrelevant','relevant_but_unused'))
           AS other_terminal,
       count(*) FILTER (WHERE d.status='pending') AS pending
  FROM public.harvester_assignments d
  JOIN classification c ON c.id=d.classification_id
 GROUP BY d.plugin_id ORDER BY d.plugin_id;

-- Laya theme choices select destinations in v4. Without adjudicated human labels, these
-- counts and downstream dispositions measure behavior, not precision or recall.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classified AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.entity_choice,c.distributions,c.model_provenance
      FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT count(*) AS classified_edges,
       count(*) FILTER (WHERE entity_choice='relevant') AS relevant_edges,
       count(*) FILTER (WHERE (
           SELECT count(*) FROM jsonb_each(distributions) x
            WHERE x.value->>'choice'='relevant')=4) AS all_four_recommended,
       count(*) FILTER (WHERE entity_choice='relevant' AND (
           SELECT count(*) FROM jsonb_each(distributions) x
            WHERE x.value->>'choice'='relevant')=4) AS relevant_all_four_recommended,
       round(avg((SELECT count(*) FROM jsonb_each(distributions) x
                   WHERE x.value->>'choice'='relevant'))::numeric,2) AS mean_recommended_fanout,
       round((avg((SELECT count(*) FROM jsonb_each(distributions) x
                   WHERE x.value->>'choice'='relevant'))
             FILTER (WHERE entity_choice='relevant'))::numeric,2)
           AS mean_recommended_fanout_relevant,
       round(avg((model_provenance->'relevance'->>'inference_ms')::numeric
                +COALESCE((model_provenance->'character_routing'->>'inference_ms')::numeric,0)),1) AS mean_laya_ms,
       round(percentile_cont(0.95) WITHIN GROUP (ORDER BY
           (model_provenance->'relevance'->>'inference_ms')::numeric
           +COALESCE((model_provenance->'character_routing'->>'inference_ms')::numeric,0))::numeric,1) AS p95_laya_ms
  FROM classified;

WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.distributions
      FROM public.harvester_classifications c
      JOIN cohort q ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT route.key AS plugin_id, count(*) AS classified_edges,
       count(*) FILTER (WHERE route.value->>'choice'='relevant') AS laya_recommended,
       count(*) FILTER (WHERE route.value->>'choice'='irrelevant') AS laya_not_recommended
  FROM classification c
 CROSS JOIN LATERAL jsonb_each(c.distributions) route
 GROUP BY route.key ORDER BY route.key;

-- Verify every saved opening and Laya input against the retained publisher
-- bytes. Only aggregate counts leave PostgreSQL; the source text stays on host.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), classified AS (
    SELECT c.* FROM public.harvester_classifications c JOIN cohort q
      ON q.article_id=c.article_id AND q.entity_type=c.entity_type
     AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
)
SELECT count(*) AS classified_edges,
       count(*) FILTER (WHERE a.full_text IS NOT NULL AND
           encode(sha256(convert_to(a.full_text,'UTF8')),'hex')=c.body_sha256)
           AS body_hash_matches,
       count(*) FILTER (WHERE a.title=c.headline) AS headline_matches,
       count(*) FILTER (WHERE a.full_text IS NOT NULL AND
           substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                     FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8'))
           AS context_byte_matches,
       count(*) FILTER (WHERE a.full_text IS NOT NULL AND
           substring(convert_to(a.full_text,'UTF8') FROM c.model_input_start+1
                     FOR c.model_input_end-c.model_input_start)=convert_to(c.model_input_text,'UTF8'))
           AS laya_input_byte_matches
  FROM classified c JOIN public.news_articles a ON a.id=c.article_id;

-- One article claim classifies every query-entity edge before publication.
-- This fan-out distribution explains long individual claims without exposing
-- article identities or source text.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), per_article AS (
    SELECT p.article_id,count(*) AS edges
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
     GROUP BY p.article_id
)
SELECT count(*) AS canonical_candidates,sum(edges) AS query_entity_edges,
       round(avg(edges)::numeric,2) AS mean_edges_per_article,
       round(percentile_cont(0.95) WITHIN GROUP (ORDER BY edges)::numeric,2)
           AS p95_edges_per_article,
       max(edges) AS max_edges_per_article,
       count(*) FILTER (WHERE edges>10) AS articles_over_10_edges,
       count(*) FILTER (WHERE edges>30) AS articles_over_30_edges
  FROM per_article;

-- A replay with the same body/model/contract reuses its classification row and
-- original created_at/model_provenance. Keep this count visible: if nonzero,
-- classification latency and fan-out describe the retained result, not fresh
-- inference performed during this nightly run. Run 333 has zero such rows.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), latest AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.created_at
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT count(*) FILTER (WHERE c.created_at<i.started_at) AS retained_prior_classifications,
       count(*) FILTER (WHERE c.created_at>=i.started_at) AS created_since_ingest_start
  FROM latest c CROSS JOIN ingest i;

-- Keep model, checkpoint, and question-contract changes visible within a
-- sweep. These values are provenance labels, not calibrated probabilities.
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE job='pipeline' AND finished_at IS NOT NULL ORDER BY started_at DESC LIMIT 1
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), latest AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.contract_version,c.model_revision,c.model_provenance
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT contract_version,model_revision,
       model_provenance->'relevance'->>'model' AS model_name,
       model_provenance->'question_set_versions' AS question_set_versions,
       count(*) AS classified_edges
  FROM latest
 GROUP BY contract_version,model_revision,model_name,question_set_versions
 ORDER BY classified_edges DESC,model_revision;
