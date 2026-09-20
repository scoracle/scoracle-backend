-- Run on Archbox through the existing db.py sql helper after selecting the debug cases.
-- Does not run models, reset attempts, overwrite revisions, or disturb active claims.
BEGIN;
SET LOCAL statement_timeout = '15s';
CREATE TEMP TABLE held (LIKE pipeline_work);
ALTER TABLE held ADD COLUMN hold_until timestamptz;
\copy held FROM '/mnt/data/backup/scoracle/releases/quality-handoff-20260920/rating-hold.csv' CSV HEADER
UPDATE pipeline_work w
SET available_at = LEAST(h.available_at, NOW())
FROM held h
WHERE w.stage = h.stage AND w.sport = h.sport
  AND w.entity_type = h.entity_type AND w.entity_id = h.entity_id
  AND w.status = 'pending' AND w.available_at = h.hold_until
  AND w.input_version IS NOT DISTINCT FROM h.input_version
RETURNING w.sport, w.entity_type, w.entity_id;
-- A changed revision is deliberately left for review, not overwritten.
SELECT w.sport, w.entity_id, w.input_version, h.input_version AS reserved_revision
FROM pipeline_work w JOIN held h USING (stage,sport,entity_type,entity_id)
WHERE w.status = 'pending' AND w.available_at = h.hold_until;
COMMIT;
