-- Make metadata_refresh_queue.unique_pending_request non-deferrable.
--
-- The detect_team_change() trigger uses this constraint as an ON CONFLICT
-- arbiter. Postgres rejects deferrable unique constraints as arbiters, so
-- every box-score insert for a first-seen player was failing with
-- "ON CONFLICT does not support deferrable unique constraints/exclusion
-- constraints as arbiters".

ALTER TABLE metadata_refresh_queue
    DROP CONSTRAINT IF EXISTS unique_pending_request;

ALTER TABLE metadata_refresh_queue
    ADD CONSTRAINT unique_pending_request UNIQUE (player_id, sport, processed_at);
