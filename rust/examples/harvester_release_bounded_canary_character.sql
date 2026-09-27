-- Release one character only for a named, time-bounded live canary batch.
-- Harvester may be back in shadow: that flag controls new Harvester
-- publications, while this script releases existing, verified assignments.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v run_id=333 -v canary_after='2026-09-27 11:48:10-04' \
--   -v plugin_id=scoracle.character.narrative -v entity_limit=5 \
--   -f rust/examples/harvester_release_bounded_canary_character.sql
BEGIN;
CREATE TEMP TABLE harvester_bounded_release ON COMMIT DROP AS
WITH canary AS (
    SELECT article_id,sport,enqueued_at FROM public.harvester_live_canary_items
     WHERE run_id=:'run_id'::bigint
       AND enqueued_at>=:'canary_after'::timestamptz
), selected AS (
    SELECT c.entity_type,c.entity_id,c.sport
      FROM public.harvester_assignments d
      JOIN public.harvester_classifications c ON c.id=d.classification_id
      JOIN canary x ON x.article_id=c.article_id AND x.sport=c.sport
     WHERE d.plugin_id=:'plugin_id' AND d.status='pending'
       AND d.reason='delivery_held' AND c.contract_version='harvest-context-v5'
       AND c.created_at>=x.enqueued_at AND c.entity_choice='relevant'
       AND d.plugin_id IN ('scoracle.character.narrative','scoracle.character.vibe',
                           'scoracle.character.transfers','scoracle.character.rating')
     GROUP BY c.entity_type,c.entity_id,c.sport
     ORDER BY max(c.id)
     LIMIT LEAST(GREATEST(:'entity_limit'::int,1),5)
), released AS (
    UPDATE public.harvester_assignments d SET reason=NULL,updated_at=NOW()
      FROM public.harvester_classifications c,canary x,selected s
     WHERE d.classification_id=c.id AND d.plugin_id=:'plugin_id'
       AND d.status='pending' AND d.reason='delivery_held'
       AND c.contract_version='harvest-context-v5' AND c.entity_choice='relevant'
       AND c.created_at>=x.enqueued_at AND c.article_id=x.article_id AND c.sport=x.sport
       AND c.entity_type=s.entity_type AND c.entity_id=s.entity_id AND c.sport=s.sport
     RETURNING c.id AS classification_id,c.entity_type,c.entity_id,c.sport
)
SELECT * FROM released;

INSERT INTO public.pipeline_work
    (stage,entity_type,entity_id,sport,status,input_version,available_at,updated_at)
SELECT CASE :'plugin_id'
         WHEN 'scoracle.character.narrative' THEN 'narratives'
         WHEN 'scoracle.character.vibe' THEN 'vibe'
         WHEN 'scoracle.character.transfers' THEN 'transfers'
         WHEN 'scoracle.character.rating' THEN 'rating'
       END,
       entity_type,entity_id,sport,'pending',
       'harvest-context-v5:bounded-canary:c' || max(classification_id)::text,NOW(),NOW()
  FROM harvester_bounded_release
 GROUP BY entity_type,entity_id,sport
ON CONFLICT (stage,entity_type,entity_id,sport) DO UPDATE SET
    status='pending',attempts=0,
    available_at=CASE WHEN pipeline_work.status='pending'
                      THEN pipeline_work.available_at ELSE NOW() END,
    updated_at=NOW(),last_error=NULL,
    input_version=EXCLUDED.input_version,running_input_version=NULL,claim_token=NULL
WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
   OR pipeline_work.status='failed';

SELECT :'plugin_id' AS plugin_id,count(*) AS released_assignments,
       count(DISTINCT (entity_type,entity_id,sport)) AS released_entities
  FROM harvester_bounded_release;
COMMIT;
