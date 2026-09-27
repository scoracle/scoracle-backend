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
), eligible AS (
    SELECT DISTINCT ON (p.article_id,p.sport) p.article_id,p.sport
      FROM public.harvester_query_provenance p
      JOIN ingest i ON p.last_seen_at BETWEEN i.started_at AND i.finished_at
      JOIN public.harvester_classifications c
        ON c.article_id=p.article_id AND c.entity_type=p.entity_type
       AND c.entity_id=p.entity_id AND c.sport=p.sport
      JOIN public.news_articles a ON a.id=c.article_id
     WHERE c.entity_choice='relevant' AND a.full_text IS NOT NULL
       AND a.title=c.headline
       AND encode(sha256(convert_to(a.full_text,'UTF8')),'hex')=c.body_sha256
       AND substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                     FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8')
     ORDER BY p.article_id,p.sport,c.id DESC
)
SELECT e.article_id,e.sport FROM eligible e
 WHERE NOT EXISTS (
     SELECT 1 FROM public.pipeline_work w
      WHERE w.stage='harvester' AND
            (w.status IN ('pending','running') OR
             (w.status='failed' AND w.attempts<5))
 )
   AND NOT EXISTS (
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
       'harvest-context-v1:live-canary:run' || :'run_id' || ':a' || article_id::text,
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

SELECT count(*) AS enqueued_live_canary_articles FROM harvester_canary_articles;
COMMIT;
