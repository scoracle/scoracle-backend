-- Player team-history tracking (event-driven, from box scores)
--
-- HISTORY, because the name of this file is now wider than its contents: this was the
-- "Metadata System for Event-Driven Player Updates" — a refresh QUEUE (metadata_refresh_queue),
-- a completion log (metadata_sync_log), a status view and three consumer helpers, all fed by a
-- trigger on event_box_scores. Migration 215 (2026-08-06) dropped every one of those. The queue
-- had a live producer and NO consumer: 42,782 rows enqueued between 2026-04-18 and 2026-06-17,
-- processed_at NULL on all of them, and nothing in Rust, Go, Python or a shell heredoc had ever
-- called get_metadata_queue_batch / get_metadata_queue_status / mark_metadata_processed.
-- metadata_sync_log was never written once. Migration 216 dropped update_sync_log_timestamp(),
-- the trigger function orphaned by that table's removal.
--
-- What SURVIVES is the useful half, and it is all that is below: the trigger keeps maintaining
-- player_team_history (a player->team record with valid_from / valid_until / is_current). It no
-- longer enqueues anything.
--
-- NOT APPLIED BY ANY TOOLING. sql/build.sh stands up new environments by cloning prod's schema
-- with pg_dump, and never replays this file; sql/migrate.sh only globs sql/migrations/*.sql.
-- The authoritative definitions live in sql/schema/schema.sql (the snapshot) and in the
-- migrations. This file is kept as the readable description of the feature — if you change the
-- trigger, change it in a migration derived from pg_get_functiondef, then mirror it here.

-- ============================================================================
-- 1. TEAM HISTORY TRACKING: Records player team movement
-- ============================================================================

CREATE TABLE IF NOT EXISTS player_team_history (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    team_id INTEGER NOT NULL,
    season INTEGER,
    valid_from TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    valid_until TIMESTAMP WITH TIME ZONE,
    is_current BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_team_history_player
    ON player_team_history (player_id, sport, is_current);

CREATE INDEX IF NOT EXISTS idx_team_history_current
    ON player_team_history (sport, is_current, team_id);

-- ============================================================================
-- 2. TRIGGER FUNCTION: Detects team changes from box scores
--    (mirrors migration 215, which derived it from prod's pg_get_functiondef)
-- ============================================================================

CREATE OR REPLACE FUNCTION detect_team_change()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
    current_team_id INTEGER;
BEGIN
    -- Skip if player_id is NULL (team stats rows)
    IF NEW.player_id IS NULL THEN
        RETURN NEW;
    END IF;

    -- Get the player's current team from history
    SELECT team_id INTO current_team_id
    FROM player_team_history
    WHERE player_id = NEW.player_id
      AND sport = NEW.sport
      AND is_current = TRUE
    ORDER BY valid_from DESC
    LIMIT 1;

    -- If no history exists (new player) OR team changed
    IF current_team_id IS NULL OR current_team_id != NEW.team_id THEN

        -- Mark old history as not current (if exists)
        UPDATE player_team_history
        SET is_current = FALSE,
            valid_until = NOW()
        WHERE player_id = NEW.player_id
          AND sport = NEW.sport
          AND is_current = TRUE;

        -- Insert new team history
        INSERT INTO player_team_history (
            player_id,
            sport,
            team_id,
            season
        ) VALUES (
            NEW.player_id,
            NEW.sport,
            NEW.team_id,
            NEW.season
        );

        -- mig 215: the metadata_refresh_queue enqueue was removed here. The queue had a
        -- producer but never had a consumer (0 of 42,782 rows processed in four months).
        -- This trigger now maintains history ONLY.

        RAISE DEBUG 'Team change detected: player_id=%, old_team=%, new_team=%',
            NEW.player_id, current_team_id, NEW.team_id;
    END IF;

    RETURN NEW;
END;
$function$;

-- ============================================================================
-- 3. ATTACH TRIGGER: Listen for box score inserts
-- ============================================================================

DROP TRIGGER IF EXISTS trg_detect_team_change ON event_box_scores;
CREATE TRIGGER trg_detect_team_change
    AFTER INSERT ON event_box_scores
    FOR EACH ROW
    EXECUTE FUNCTION detect_team_change();

-- ============================================================================
-- 4. DOCUMENTATION
-- ============================================================================

COMMENT ON TABLE player_team_history IS
'DRIVER: trg_detect_team_change -> detect_team_change() (trigger, AFTER INSERT on '
'event_box_scores). SQL-only: no Rust, Go, Python or shell caller. Player->team record '
'with valid_from/valid_until/is_current. Fills only while box scores arrive, so it is '
'FLAT in the off-season by design (last write 2026-06-17) — that is dormancy, not rot. '
'mig 215 kept this and dropped the metadata_refresh_queue it used to feed.';
