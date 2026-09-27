-- Bounded rehearsal of the current Harvester contract against old, acquired
-- nightly sources. Run only with HARVESTER_SHADOW_MODE=1 and an idle Harvester
-- queue. Save stdout to a file ON THE PRODUCTION HOST: it contains article IDs.
-- psql "$DATABASE_PRIVATE_URL" -X -qAt -v ON_ERROR_STOP=1 \
--   -v run_id=333 -v per_stratum=6 \
--   -f rust/examples/harvester_enqueue_shadow_probe.sql \
--   > /tmp/harvester-shadow-probe-run333-20260927.csv
BEGIN;
CREATE TEMP TABLE harvester_shadow_probe ON COMMIT DROP AS
WITH ingest AS (
    SELECT started_at,finished_at FROM public.pipeline_runs
     WHERE id=:'run_id'::bigint AND job='pipeline' AND status='success'
       AND finished_at IS NOT NULL
), cohort AS (
    SELECT DISTINCT p.article_id,p.entity_type,p.entity_id,p.sport
      FROM public.harvester_query_provenance p CROSS JOIN ingest i
     WHERE p.last_seen_at BETWEEN i.started_at AND i.finished_at
), old_classification AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport)
           c.article_id,c.entity_type,c.entity_id,c.sport,c.entity_choice
      FROM public.harvester_classifications c JOIN cohort q
        ON q.article_id=c.article_id AND q.entity_type=c.entity_type
       AND q.entity_id=c.entity_id AND q.sport=c.sport
     WHERE c.contract_version='harvest-context-v1'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), one_edge_per_article AS (
    SELECT DISTINCT ON (q.article_id)
           q.article_id,q.sport,c.entity_choice AS old_choice,a.attempts AS acquisition_attempts_before
      FROM cohort q JOIN old_classification c
        ON c.article_id=q.article_id AND c.entity_type=q.entity_type
       AND c.entity_id=q.entity_id AND c.sport=q.sport
      JOIN public.news_articles n ON n.id=q.article_id
      JOIN public.harvester_acquisitions a ON a.article_id=q.article_id
     WHERE q.entity_type='team' AND n.duplicate_of IS NULL
       AND n.full_text IS NOT NULL AND a.status='acquired'
       AND NOT EXISTS (
           SELECT 1 FROM public.pipeline_work w
            WHERE w.stage='harvester' AND w.entity_type='article'
              AND w.entity_id=q.article_id AND w.sport=q.sport
              AND w.status IN ('pending','running')
       )
     ORDER BY q.article_id,
              CASE c.entity_choice WHEN 'irrelevant' THEN 0 ELSE 1 END,
              q.entity_id
), ranked AS (
    SELECT *,row_number() OVER (
        PARTITION BY old_choice ORDER BY md5(article_id::text)
    ) AS sample_rank
      FROM one_edge_per_article
)
SELECT article_id,sport,old_choice,acquisition_attempts_before,now() AS enqueued_at
  FROM ranked
 WHERE sample_rank<=LEAST(GREATEST(:'per_stratum'::int,1),10)
   AND old_choice IN ('relevant','irrelevant');

-- The stdout file is the durable local probe manifest; successful queue claims
-- disappear. It must be kept on the production host with owner-only access.
SELECT article_id || ',' || sport || ',' || old_choice || ',' ||
       acquisition_attempts_before || ',' || enqueued_at
  FROM harvester_shadow_probe ORDER BY old_choice,article_id;

INSERT INTO public.pipeline_work
    (stage,entity_type,entity_id,sport,status,input_version,available_at,updated_at)
SELECT 'harvester','article',article_id,sport,'pending',
       'harvest-context-v5:shadow-probe:run' || :'run_id' || ':a' || article_id::text,
       NOW(),NOW()
  FROM harvester_shadow_probe
ON CONFLICT (stage,entity_type,entity_id,sport) DO UPDATE SET
    status='pending',attempts=0,
    available_at=NOW(),updated_at=NOW(),last_error=NULL,
    input_version=EXCLUDED.input_version,running_input_version=NULL,claim_token=NULL
WHERE pipeline_work.status NOT IN ('pending','running');
COMMIT;
