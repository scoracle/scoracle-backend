-- Release at most entity_limit entity groups of one held Harvester character.
-- Run on the production host only, after setting HARVESTER_SHADOW_MODE=0 and
-- HARVESTER_DELIVERY_CHARACTERS to the desired live subset. Example:
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v plugin_id=scoracle.character.narrative -v entity_limit=5 \
--   -f rust/examples/harvester_release_character.sql
-- Repeat until released_assignments=0. Publisher text is never selected.
BEGIN;
CREATE TEMP TABLE harvester_release_rows ON COMMIT DROP AS
WITH selected AS (
    SELECT c.entity_type,c.entity_id,c.sport
      FROM public.harvester_assignments d
     JOIN public.harvester_classifications c ON c.id=d.classification_id
     WHERE d.plugin_id=:'plugin_id' AND d.reason='delivery_held' AND d.status='pending'
       AND c.contract_version='harvest-context-v4' AND c.entity_choice='relevant'
       AND d.plugin_id IN ('scoracle.character.narrative','scoracle.character.vibe',
                           'scoracle.character.transfers','scoracle.character.rating')
     GROUP BY c.entity_type,c.entity_id,c.sport
     ORDER BY max(c.id)
     LIMIT LEAST(GREATEST(:'entity_limit'::int,1),100)
), released AS (
    UPDATE public.harvester_assignments d SET reason=NULL,updated_at=NOW()
      FROM public.harvester_classifications c, selected s
     WHERE d.classification_id=c.id AND d.plugin_id=:'plugin_id'
       AND d.status='pending' AND d.reason='delivery_held'
       AND c.contract_version='harvest-context-v4' AND c.entity_choice='relevant'
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
       'harvest-context-v4:release:c' || max(classification_id)::text,NOW(),NOW()
  FROM harvester_release_rows
 GROUP BY entity_type,entity_id,sport
ON CONFLICT (stage,entity_type,entity_id,sport) DO UPDATE SET
    status='pending',attempts=0,
    available_at=CASE WHEN pipeline_work.status='pending'
                      THEN pipeline_work.available_at ELSE NOW() END,
    updated_at=NOW(),last_error=NULL,
    input_version=EXCLUDED.input_version,running_input_version=NULL,claim_token=NULL
WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
   OR pipeline_work.status='failed';

SELECT :'plugin_id' AS plugin_id,
       count(*) AS released_assignments,
       count(DISTINCT (entity_type,entity_id,sport)) AS released_entities
  FROM harvester_release_rows;
COMMIT;
