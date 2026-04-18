-- Metadata System for Event-Driven Player Updates
-- This system automatically detects player team changes and queues metadata refresh

-- ============================================================================
-- 1. QUEUE TABLE: Tracks metadata refresh requests
-- ============================================================================

CREATE TABLE IF NOT EXISTS metadata_refresh_queue (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER,
    reason TEXT, -- 'team_change', 'bootstrap', 'manual'
    priority INTEGER DEFAULT 1, -- 1=high (team change), 2=normal (bootstrap)
    requested_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    processed_at TIMESTAMP WITH TIME ZONE,
    retry_count INTEGER DEFAULT 0,
    error_message TEXT,
    
    -- Prevent duplicate requests for same player.
    -- Not deferrable: the team-change trigger uses this as an ON CONFLICT
    -- arbiter (see detect_team_change), which Postgres rejects on deferrable
    -- unique constraints.
    CONSTRAINT unique_pending_request UNIQUE (player_id, sport, processed_at)
);

-- Index for efficient querying
CREATE INDEX IF NOT EXISTS idx_metadata_queue_pending 
    ON metadata_refresh_queue (processed_at, priority, requested_at) 
    WHERE processed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_metadata_queue_player 
    ON metadata_refresh_queue (player_id, sport);

-- ============================================================================
-- 2. TEAM HISTORY TRACKING: Records player team movement
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
-- 3. METADATA SYNC LOG: Tracks when metadata was last updated
-- ============================================================================

CREATE TABLE IF NOT EXISTS metadata_sync_log (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    last_sync_at TIMESTAMP WITH TIME ZONE,
    metadata_version INTEGER DEFAULT 1,
    sync_source TEXT, -- 'bootstrap', 'team_change', 'scheduled'
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    
    CONSTRAINT unique_player_sync UNIQUE (player_id, sport)
);

CREATE INDEX IF NOT EXISTS idx_sync_log_player 
    ON metadata_sync_log (player_id, sport);

-- Auto-update timestamp
CREATE OR REPLACE FUNCTION update_sync_log_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_log_timestamp ON metadata_sync_log;
CREATE TRIGGER trg_sync_log_timestamp
    BEFORE UPDATE ON metadata_sync_log
    FOR EACH ROW EXECUTE FUNCTION update_sync_log_timestamp();

-- ============================================================================
-- 4. TRIGGER FUNCTION: Detects team changes from box scores
-- ============================================================================

CREATE OR REPLACE FUNCTION detect_team_change()
RETURNS TRIGGER AS $$
DECLARE
    current_team_id INTEGER;
    history_record_id INTEGER;
    queue_id INTEGER;
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
        
        -- Queue metadata refresh (high priority for team change)
        -- Use ON CONFLICT to avoid duplicates
        INSERT INTO metadata_refresh_queue (
            player_id, 
            sport, 
            season, 
            reason, 
            priority
        ) VALUES (
            NEW.player_id, 
            NEW.sport, 
            NEW.season, 
            'team_change', 
            1
        )
        ON CONFLICT (player_id, sport, processed_at) 
        DO UPDATE SET 
            priority = EXCLUDED.priority,
            requested_at = NOW(),
            retry_count = 0,
            error_message = NULL
        WHERE metadata_refresh_queue.processed_at IS NULL;
        
        -- Log the detection
        RAISE DEBUG 'Team change detected: player_id=%, old_team=%, new_team=%',
            NEW.player_id, current_team_id, NEW.team_id;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 5. ATTACH TRIGGER: Listen for box score inserts
-- ============================================================================

DROP TRIGGER IF EXISTS trg_detect_team_change ON event_box_scores;
CREATE TRIGGER trg_detect_team_change
    AFTER INSERT ON event_box_scores
    FOR EACH ROW
    EXECUTE FUNCTION detect_team_change();

-- ============================================================================
-- 6. HELPER FUNCTION: Check queue status
-- ============================================================================

CREATE OR REPLACE FUNCTION get_metadata_queue_status(
    p_sport TEXT DEFAULT NULL
)
RETURNS TABLE (
    total_pending BIGINT,
    high_priority BIGINT,
    normal_priority BIGINT,
    failed_items BIGINT,
    oldest_request TIMESTAMP WITH TIME ZONE
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        COUNT(*) FILTER (WHERE processed_at IS NULL)::BIGINT as total_pending,
        COUNT(*) FILTER (WHERE processed_at IS NULL AND priority = 1)::BIGINT as high_priority,
        COUNT(*) FILTER (WHERE processed_at IS NULL AND priority = 2)::BIGINT as normal_priority,
        COUNT(*) FILTER (WHERE processed_at IS NOT NULL AND error_message IS NOT NULL)::BIGINT as failed_items,
        MIN(requested_at) FILTER (WHERE processed_at IS NULL) as oldest_request
    FROM metadata_refresh_queue
    WHERE (p_sport IS NULL OR sport = p_sport);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 7. HELPER FUNCTION: Get next batch of queue items
-- ============================================================================

CREATE OR REPLACE FUNCTION get_metadata_queue_batch(
    batch_size INTEGER DEFAULT 10,
    p_sport TEXT DEFAULT NULL
)
RETURNS TABLE (
    id INTEGER,
    player_id INTEGER,
    sport TEXT,
    season INTEGER,
    reason TEXT,
    priority INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        q.id,
        q.player_id,
        q.sport,
        q.season,
        q.reason,
        q.priority
    FROM metadata_refresh_queue q
    WHERE q.processed_at IS NULL
      AND (p_sport IS NULL OR q.sport = p_sport)
    ORDER BY q.priority ASC, q.requested_at ASC
    LIMIT batch_size;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 8. HELPER FUNCTION: Mark queue item as processed
-- ============================================================================

CREATE OR REPLACE FUNCTION mark_metadata_processed(
    p_queue_id INTEGER,
    p_success BOOLEAN DEFAULT TRUE,
    p_error_message TEXT DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    UPDATE metadata_refresh_queue
    SET 
        processed_at = NOW(),
        error_message = CASE WHEN p_success THEN NULL ELSE p_error_message END,
        retry_count = CASE WHEN p_success THEN retry_count ELSE retry_count + 1 END
    WHERE id = p_queue_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 9. INITIAL DATA: Bootstrap existing players into history
-- ============================================================================

-- This ensures existing data doesn't trigger false change detections
INSERT INTO player_team_history (player_id, sport, team_id, season, is_current)
SELECT 
    DISTINCT ON (player_id, sport)
    player_id,
    sport,
    team_id,
    season,
    TRUE
FROM event_box_scores
WHERE player_id IS NOT NULL
ON CONFLICT DO NOTHING;

-- ============================================================================
-- 10. VIEWS: For monitoring the system
-- ============================================================================

CREATE OR REPLACE VIEW metadata_queue_status AS
SELECT 
    sport,
    COUNT(*) FILTER (WHERE processed_at IS NULL) as pending,
    COUNT(*) FILTER (WHERE processed_at IS NULL AND priority = 1) as high_priority,
    COUNT(*) FILTER (WHERE processed_at IS NULL AND priority = 2) as normal_priority,
    COUNT(*) FILTER (WHERE processed_at IS NOT NULL AND error_message IS NOT NULL) as failed,
    COUNT(*) FILTER (WHERE processed_at IS NOT NULL AND error_message IS NULL) as successful,
    MIN(requested_at) FILTER (WHERE processed_at IS NULL) as oldest_pending,
    MAX(processed_at) FILTER (WHERE processed_at IS NOT NULL) as last_processed
FROM metadata_refresh_queue
GROUP BY sport;

-- Grant permissions
GRANT SELECT ON metadata_queue_status TO PUBLIC;

-- ============================================================================
-- DONE!
-- ============================================================================

COMMENT ON TABLE metadata_refresh_queue IS 'Queue for player metadata refresh requests. Populated by trigger on event_box_scores.';
COMMENT ON TABLE player_team_history IS 'Tracks player team movement over time. Used to detect transfers/trades.';
COMMENT ON TABLE metadata_sync_log IS 'Records when player metadata was last synchronized from APIs.';
COMMENT ON FUNCTION detect_team_change() IS 'Trigger function that detects when a player appears in box scores for a different team and queues metadata refresh.';
