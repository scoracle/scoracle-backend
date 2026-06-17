-- 088_rename_vibe_to_sentiment.sql
--
-- Demotes the old sentiment score from "the Vibe" to an internal ingredient.
-- vibe_scores → sentiment_scores. The table is append-only; the rename is
-- safe to run atomically (ACCESS EXCLUSIVE is brief on an append-only table).
--
-- Deploy protocol: apply this migration AND restart the Go binary atomically.
-- db.New() prepares every statement at boot and will fail fast against the
-- old table name, so never restart the old binary after this migration runs.
--
-- NOTIFY channel: pg_notify('vibe_trigger', ...) wire string is KEPT in this
-- release (R1). Both the function renamed below AND the Go listener
-- (internal/listener/news_volume_worker.go) still use the 'vibe_trigger' wire.
-- R2 will atomically flip BOTH sides to 'sentiment_trigger'; never flip one
-- without the other or notifications will be silently dropped.

BEGIN;

ALTER TABLE vibe_scores RENAME TO sentiment_scores;

-- Index renames
ALTER INDEX IF EXISTS idx_vibe_scores_entity_recent  RENAME TO idx_sentiment_scores_entity_recent;
ALTER INDEX IF EXISTS idx_vibe_scores_recent          RENAME TO idx_sentiment_scores_recent;
ALTER INDEX IF EXISTS idx_vibe_scores_sport_sentiment RENAME TO idx_sentiment_scores_sport_sentiment;

-- Constraint rename
ALTER TABLE sentiment_scores
    RENAME CONSTRAINT vibe_scores_trigger_type_check
                   TO sentiment_scores_trigger_type_check;

-- Drop legacy blurb column: always NULL since migration 009 promoted the
-- output from a blurb to a numeric sentiment score. New blurbs belong on
-- vibe_synthesis (Phase B). No active SELECT reads this column.
ALTER TABLE sentiment_scores DROP COLUMN IF EXISTS blurb;

-- Rename the trigger function from notify_vibe_trigger → notify_sentiment_trigger.
-- Body is identical to the migration-034 version: fires pg_notify('vibe_trigger', ...)
-- (R1 wire — kept intentionally; see header) AND pg_notify('transfer_trigger', ...)
-- for team events. Kept in one function to avoid double-counting the 4→5 crossing.
CREATE OR REPLACE FUNCTION notify_sentiment_trigger() RETURNS TRIGGER AS $$
DECLARE
    prior_count INTEGER;
BEGIN
    SELECT COUNT(DISTINCT article_id) INTO prior_count
    FROM news_article_entities
    WHERE entity_type = NEW.entity_type
      AND entity_id = NEW.entity_id
      AND sport = NEW.sport
      AND created_at > NOW() - INTERVAL '60 minutes'
      AND article_id <> NEW.article_id;

    IF prior_count = 4 THEN
        -- R1: wire string kept as 'vibe_trigger' intentionally.
        -- R2 will flip this to 'sentiment_trigger' atomically with the Go side.
        PERFORM pg_notify('vibe_trigger', json_build_object(
            'entity_type', NEW.entity_type,
            'entity_id', NEW.entity_id,
            'sport', NEW.sport,
            'article_count', prior_count + 1,
            'ts', extract(epoch from now())::bigint
        )::text);

        IF NEW.entity_type = 'team' THEN
            PERFORM pg_notify('transfer_trigger', json_build_object(
                'entity_type', NEW.entity_type,
                'entity_id', NEW.entity_id,
                'sport', NEW.sport,
                'article_count', prior_count + 1,
                'ts', extract(epoch from now())::bigint
            )::text);
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Swap the trigger on news_article_entities to use the renamed function.
DROP TRIGGER IF EXISTS vibe_trigger ON news_article_entities;
CREATE TRIGGER sentiment_trigger
    AFTER INSERT ON news_article_entities
    FOR EACH ROW EXECUTE FUNCTION notify_sentiment_trigger();

COMMIT;
