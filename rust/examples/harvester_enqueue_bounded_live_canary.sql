-- Bounded live handoff rehearsal from byte-verified v7 shadow classifications.
-- Unlike the full-nightly release gate, this selects only articles already
-- proven by the current headline-first contract. Run only after verifying
-- HARVESTER_SHADOW_MODE=0 and HARVESTER_DELIVERY_CHARACTERS='' on the worker.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v run_id=333 -v article_limit=5 \
--   -f rust/examples/harvester_enqueue_bounded_live_canary.sql
-- Returns only a count. The selected cohort is durable in migration 285.
BEGIN;
CREATE TEMP TABLE harvester_bounded_canary ON COMMIT DROP AS
WITH ingest AS (
    SELECT id,started_at,finished_at FROM public.pipeline_runs
     WHERE id=:'run_id'::bigint AND job='pipeline' AND status='success'
       AND finished_at IS NOT NULL
), quiet AS (
    SELECT NOT EXISTS (
        SELECT 1 FROM public.pipeline_work
         WHERE stage='harvester'
           AND (status IN ('pending','running') OR
                (status='failed' AND attempts<5
                 AND available_at<=now()+interval '5 minutes'))
    ) AS ready
), eligible AS (
    SELECT DISTINCT ON (c.article_id,c.sport)
           c.article_id,c.sport,a.attempts AS acquisition_attempts_before
      FROM public.harvester_classifications c
      JOIN public.news_articles n ON n.id=c.article_id
      JOIN public.harvester_acquisitions a ON a.article_id=c.article_id
      JOIN public.harvester_headline_gates g
        ON g.article_id=c.article_id AND g.entity_type=c.entity_type
       AND g.entity_id=c.entity_id AND g.sport=c.sport
      JOIN public.harvester_query_provenance q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
      JOIN ingest i ON q.last_seen_at BETWEEN i.started_at AND i.finished_at
     WHERE c.contract_version='harvest-context-v7'
       AND c.model_provenance->'question_set_versions'->>'relevance'='harvest-headline-relevance-v3'
       AND c.entity_choice='relevant' AND g.contract_version='harvest-headline-v3'
       AND g.policy_version='explicit-headline-read-p025-v2' AND g.admitted
       AND g.input_hash=c.model_provenance->>'headline_gate_input_hash'
       AND a.status='acquired' AND n.full_text IS NOT NULL AND n.title=c.headline
       AND encode(sha256(convert_to(n.full_text,'UTF8')),'hex')=c.body_sha256
       AND substring(convert_to(n.full_text,'UTF8') FROM c.context_start+1
                     FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8')
       AND substring(convert_to(n.full_text,'UTF8') FROM c.model_input_start+1
                     FOR c.model_input_end-c.model_input_start)=convert_to(c.model_input_text,'UTF8')
       AND NOT EXISTS (
           SELECT 1 FROM public.harvester_assignments d
            WHERE d.classification_id=c.id
       )
       AND NOT EXISTS (
           SELECT 1 FROM public.harvester_live_canary_items x
            WHERE x.run_id=:'run_id'::bigint AND x.article_id=c.article_id AND x.sport=c.sport
       )
     ORDER BY c.article_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT e.article_id,e.sport,e.acquisition_attempts_before
  FROM eligible e CROSS JOIN ingest i CROSS JOIN quiet q
 WHERE q.ready
 ORDER BY e.article_id,e.sport
 LIMIT LEAST(GREATEST(:'article_limit'::int,1),5);

INSERT INTO public.pipeline_work
    (stage,entity_type,entity_id,sport,status,input_version,available_at,updated_at)
SELECT 'harvester','article',article_id,sport,'pending',
       'harvest-context-v7:live-canary:run' || :'run_id' || ':a' || article_id::text,
       NOW(),NOW()
  FROM harvester_bounded_canary
ON CONFLICT (stage,entity_type,entity_id,sport) DO UPDATE SET
    status='pending',attempts=0,available_at=NOW(),updated_at=NOW(),last_error=NULL,
    input_version=EXCLUDED.input_version,running_input_version=NULL,claim_token=NULL
WHERE pipeline_work.status NOT IN ('pending','running');

INSERT INTO public.harvester_live_canary_items
    (run_id,article_id,sport,enqueued_at,acquisition_attempts_before)
SELECT :'run_id'::bigint,article_id,sport,NOW(),acquisition_attempts_before
  FROM harvester_bounded_canary;

SELECT count(*) AS enqueued_bounded_live_canary_articles FROM harvester_bounded_canary;
COMMIT;
