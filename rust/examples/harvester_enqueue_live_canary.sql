-- Reprocess a bounded, already-classified sample through live Harvester.
-- Run only after the shadow corpus has drained, the CI-passed worker has been
-- restarted, and HARVESTER_SHADOW_MODE=0 with HARVESTER_DELIVERY_CHARACTERS=''.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v run_id=333 -v article_limit=5 \
--   -f rust/examples/harvester_enqueue_live_canary.sql
-- No article IDs, URLs, or publisher text are returned to the caller.
BEGIN;
CREATE TEMP TABLE harvester_canary_articles ON COMMIT DROP AS
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE id=:'run_id'::bigint AND job='pipeline' AND status='success'
       AND finished_at IS NOT NULL
), cohort AS (
    SELECT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), articles AS (
    SELECT DISTINCT article_id FROM cohort
), classified AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.*
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v2'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), shadow_ready AS (
    SELECT EXISTS (SELECT 1 FROM ingest) AND EXISTS (SELECT 1 FROM articles)
       AND NOT EXISTS (
           SELECT 1 FROM public.pipeline_work w
            WHERE w.stage='harvester' AND
                  (w.status IN ('pending','running') OR
                   (w.status='failed' AND w.attempts<5))
       )
       AND NOT EXISTS (
           SELECT 1 FROM articles a WHERE NOT EXISTS (
               SELECT 1 FROM public.harvester_acquisitions h WHERE h.article_id=a.article_id)
       )
       AND NOT EXISTS (
           SELECT 1 FROM articles a JOIN public.harvester_acquisitions h
             ON h.article_id=a.article_id WHERE h.status='classification_error'
       )
       AND NOT EXISTS (
           SELECT 1 FROM cohort q WHERE NOT EXISTS (
               SELECT 1 FROM classified c
                WHERE c.article_id=q.article_id AND c.entity_type=q.entity_type
                  AND c.entity_id=q.entity_id AND c.sport=q.sport)
             AND NOT EXISTS (
               SELECT 1 FROM public.harvester_acquisitions h
                WHERE h.article_id=q.article_id AND (
                  h.status='duplicate' OR
                  (h.status IN ('retryable_error','blocked','low_content')
                   AND EXISTS (SELECT 1 FROM public.pipeline_work w
                                WHERE w.stage='harvester' AND w.entity_type='article'
                                  AND w.entity_id=q.article_id AND w.sport=q.sport
                                  AND w.status='failed' AND w.attempts>=5))))
       )
       AND NOT EXISTS (
           SELECT 1 FROM classified c JOIN public.news_articles a ON a.id=c.article_id
            WHERE a.full_text IS NULL OR a.title IS DISTINCT FROM c.headline
               OR encode(sha256(convert_to(a.full_text,'UTF8')),'hex') IS DISTINCT FROM c.body_sha256
               OR substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                            FOR c.context_end-c.context_start) IS DISTINCT FROM convert_to(c.context_text,'UTF8')
               OR substring(convert_to(a.full_text,'UTF8') FROM c.model_input_start+1
                            FOR c.model_input_end-c.model_input_start) IS DISTINCT FROM convert_to(c.model_input_text,'UTF8')
       ) AS ready
), eligible AS (
    SELECT DISTINCT ON (c.article_id,c.sport) c.article_id,c.sport
      FROM classified c
      JOIN public.news_articles a ON a.id=c.article_id
     WHERE c.entity_choice='relevant' AND a.full_text IS NOT NULL
       AND a.title=c.headline
       AND encode(sha256(convert_to(a.full_text,'UTF8')),'hex')=c.body_sha256
       AND substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                     FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8')
     ORDER BY c.article_id,c.sport,c.id DESC
)
SELECT e.article_id,e.sport,h.attempts AS acquisition_attempts_before
  FROM eligible e
  JOIN public.harvester_acquisitions h ON h.article_id=e.article_id
 CROSS JOIN shadow_ready g
 WHERE g.ready AND NOT EXISTS (
     SELECT 1 FROM public.pipeline_work w
      WHERE w.stage='harvester' AND w.entity_type='article'
        AND w.entity_id=e.article_id AND w.sport=e.sport
        AND w.status IN ('pending','running')
 )
 ORDER BY e.article_id,e.sport
 LIMIT LEAST(GREATEST(:'article_limit'::int,1),10);

INSERT INTO public.pipeline_work
    (stage,entity_type,entity_id,sport,status,input_version,available_at,updated_at)
SELECT 'harvester','article',article_id,sport,'pending',
       'harvest-context-v2:live-canary:run' || :'run_id' || ':a' || article_id::text,
       NOW(),NOW()
  FROM harvester_canary_articles
ON CONFLICT (stage,entity_type,entity_id,sport) DO UPDATE SET
    status='pending',attempts=0,
    available_at=CASE WHEN pipeline_work.status='pending'
                      THEN pipeline_work.available_at ELSE NOW() END,
    updated_at=NOW(),last_error=NULL,
    input_version=EXCLUDED.input_version,running_input_version=NULL,claim_token=NULL
WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
   OR pipeline_work.status='failed';

-- Successful queue claims are deleted, so retain the selected cohort and its
-- pre-replay acquisition attempt count in the same enqueue transaction.
INSERT INTO public.harvester_live_canary_items
    (run_id,article_id,sport,enqueued_at,acquisition_attempts_before)
SELECT :'run_id'::bigint,article_id,sport,NOW(),acquisition_attempts_before
  FROM harvester_canary_articles
ON CONFLICT (run_id,article_id,sport) DO UPDATE SET
    enqueued_at=EXCLUDED.enqueued_at,
    acquisition_attempts_before=EXCLUDED.acquisition_attempts_before;

SELECT count(*) AS enqueued_live_canary_articles FROM harvester_canary_articles;
COMMIT;
