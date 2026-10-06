-- Stop future processing of pending Harvester assignments for one character.
-- First remove this character from HARVESTER_DELIVERY_CHARACTERS and restart
-- cognition. For an urgent rollback, stop cognition before running this script
-- so an already-running character call cannot publish during the change.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v plugin_id=scoracle.character.narrative \
--   -f rust/examples/harvester_hold_character.sql
-- Terminal assignments and already-published products remain auditable.
BEGIN;
CREATE TEMP TABLE harvester_newly_held ON COMMIT DROP AS
WITH held AS (
    UPDATE public.harvester_assignments d
       SET reason='delivery_held',updated_at=NOW()
     WHERE d.plugin_id=:'plugin_id' AND d.status='pending'
       AND d.reason IS DISTINCT FROM 'delivery_held'
       AND d.plugin_id IN ('scoracle.character.narrative','scoracle.character.vibe',
                           'scoracle.character.transfers','scoracle.character.rating')
     RETURNING d.classification_id
)
SELECT classification_id FROM held;
SELECT :'plugin_id' AS plugin_id,count(*) AS newly_held_assignments
  FROM harvester_newly_held;
COMMIT;
