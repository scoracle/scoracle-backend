-- Scoracle Data — Derived Stats & Percentile Self-Sufficiency v5.4
-- Created: 2026-02-10
-- Purpose: Complete the Postgres-centric derived stats layer. After this
--          migration, all three sports have triggers that compute derived
--          stats on INSERT/UPDATE, and the percentile function reads its
--          own inverse-stats list from stat_definitions (no Python config).
--
-- Changes:
--   1. NBA player derived stats trigger (per-36, true_shooting_pct, efficiency)
--   2. NBA team derived stats trigger (win_pct stored in JSONB)
--   3. NFL player derived stats trigger (td_int_ratio, catch_pct)
--   4. NFL team derived stats trigger (win_pct stored in JSONB)
--   5. Extend football trigger: add key_passes_per_90, shots_per_90,
--      tackles_per_90, interceptions_per_90, goals_conceded_per_90, save_pct
--   6. Update recalculate_percentiles() to read inverse stats from
--      stat_definitions instead of requiring a Python-passed array
--   7. Drop unused GIN indexes on stats and percentiles JSONB columns

-- ============================================================================
-- 1. NBA PLAYER DERIVED STATS TRIGGER
-- ============================================================================
-- BDL returns per-game averages. We compute:
--   per-36 stats:  stat / minutes * 36 (industry standard normalization)
--   true_shooting_pct: pts / (2 * (fga + 0.44 * fta)) * 100
--   efficiency: (pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover)
--
-- Only fires WHEN (NEW.sport = 'NBA').

CREATE OR REPLACE FUNCTION compute_nba_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes  NUMERIC;
    pts      NUMERIC;
    reb      NUMERIC;
    ast      NUMERIC;
    stl      NUMERIC;
    blk      NUMERIC;
    fga      NUMERIC;
    fgm      NUMERIC;
    fta      NUMERIC;
    ftm      NUMERIC;
    turnover NUMERIC;
    tsa      NUMERIC;  -- true shooting attempts
BEGIN
    minutes  := (NEW.stats->>'minutes')::NUMERIC;
    pts      := (NEW.stats->>'pts')::NUMERIC;
    reb      := (NEW.stats->>'reb')::NUMERIC;
    ast      := (NEW.stats->>'ast')::NUMERIC;
    stl      := (NEW.stats->>'stl')::NUMERIC;
    blk      := (NEW.stats->>'blk')::NUMERIC;
    fga      := (NEW.stats->>'fga')::NUMERIC;
    fgm      := (NEW.stats->>'fgm')::NUMERIC;
    fta      := (NEW.stats->>'fta')::NUMERIC;
    ftm      := (NEW.stats->>'ftm')::NUMERIC;
    turnover := (NEW.stats->>'turnover')::NUMERIC;

    -- Per-36 minute stats (requires minutes > 0)
    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF pts IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'pts_per_36', ROUND(pts / minutes * 36, 1));
        END IF;
        IF reb IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'reb_per_36', ROUND(reb / minutes * 36, 1));
        END IF;
        IF ast IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'ast_per_36', ROUND(ast / minutes * 36, 1));
        END IF;
        IF stl IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'stl_per_36', ROUND(stl / minutes * 36, 1));
        END IF;
        IF blk IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'blk_per_36', ROUND(blk / minutes * 36, 1));
        END IF;
    END IF;

    -- True Shooting Percentage: pts / (2 * (fga + 0.44 * fta)) * 100
    -- Standard NBA formula accounting for 3-pointers and free throws
    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1));
        END IF;
    END IF;

    -- Efficiency Rating: (pts + reb + ast + stl + blk) - ((fga-fgm) + (fta-ftm) + tov)
    -- Simple efficiency metric (not PER, which requires team pace data)
    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL
       AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL
       AND fta IS NOT NULL AND ftm IS NOT NULL
       AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'efficiency', ROUND(
                (pts + reb + ast + stl + blk)
                - ((fga - fgm) + (fta - ftm) + turnover),
            1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_nba_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION compute_nba_derived_stats();

-- ============================================================================
-- 2. NBA TEAM DERIVED STATS TRIGGER
-- ============================================================================
-- Computes win_pct and stores it in the JSONB so it's available for
-- percentile calculation without special-case logic.

CREATE OR REPLACE FUNCTION compute_nba_team_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    wins   NUMERIC;
    losses NUMERIC;
    total  NUMERIC;
BEGIN
    wins   := (NEW.stats->>'wins')::NUMERIC;
    losses := (NEW.stats->>'losses')::NUMERIC;

    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses;
        IF total > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'win_pct', ROUND(wins / total, 3));
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_nba_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION compute_nba_team_derived_stats();

-- ============================================================================
-- 3. NFL PLAYER DERIVED STATS TRIGGER
-- ============================================================================
-- Computes:
--   td_int_ratio:  passing_touchdowns / passing_interceptions
--   catch_pct:     receptions / receiving_targets * 100

CREATE OR REPLACE FUNCTION compute_nfl_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    pass_td  NUMERIC;
    pass_int NUMERIC;
    rec      NUMERIC;
    targets  NUMERIC;
BEGIN
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;

    -- TD/INT ratio (requires interceptions > 0 to avoid division by zero)
    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;

    -- Catch percentage (requires targets > 0)
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'catch_pct', ROUND(rec / targets * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_nfl_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION compute_nfl_derived_stats();

-- ============================================================================
-- 4. NFL TEAM DERIVED STATS TRIGGER
-- ============================================================================
-- Computes win_pct accounting for ties: wins / (wins + losses + ties)

CREATE OR REPLACE FUNCTION compute_nfl_team_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    wins   NUMERIC;
    losses NUMERIC;
    ties   NUMERIC;
    total  NUMERIC;
BEGIN
    wins   := (NEW.stats->>'wins')::NUMERIC;
    losses := (NEW.stats->>'losses')::NUMERIC;
    ties   := COALESCE((NEW.stats->>'ties')::NUMERIC, 0);

    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses + ties;
        IF total > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'win_pct', ROUND(wins / total, 3));
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_nfl_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION compute_nfl_team_derived_stats();

-- ============================================================================
-- 5. EXTEND FOOTBALL DERIVED STATS TRIGGER
-- ============================================================================
-- The original trigger (003) computed goals_per_90, assists_per_90,
-- shot_accuracy, pass_accuracy, duel_success_rate, dribble_success_rate.
-- This version adds the remaining per-90 and GK stats registered in
-- stat_definitions:
--   key_passes_per_90, shots_per_90, tackles_per_90, interceptions_per_90,
--   goals_conceded_per_90, save_pct

CREATE OR REPLACE FUNCTION compute_football_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes    NUMERIC;
    goals      NUMERIC;
    assists    NUMERIC;
    key_passes NUMERIC;
    shots_t    NUMERIC;
    shots_on   NUMERIC;
    passes_t   NUMERIC;
    passes_a   NUMERIC;
    tackles    NUMERIC;
    intercepts NUMERIC;
    duels_t    NUMERIC;
    duels_w    NUMERIC;
    dribbles_a NUMERIC;
    dribbles_s NUMERIC;
    saves      NUMERIC;
    conceded   NUMERIC;
BEGIN
    -- Extract raw values once
    minutes    := (NEW.stats->>'minutes_played')::NUMERIC;
    goals      := (NEW.stats->>'goals')::NUMERIC;
    assists    := (NEW.stats->>'assists')::NUMERIC;
    key_passes := (NEW.stats->>'key_passes')::NUMERIC;
    shots_t    := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on   := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t   := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a   := (NEW.stats->>'passes_accurate')::NUMERIC;
    tackles    := (NEW.stats->>'tackles')::NUMERIC;
    intercepts := (NEW.stats->>'interceptions')::NUMERIC;
    duels_t    := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w    := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s := (NEW.stats->>'dribbles_success')::NUMERIC;
    saves      := (NEW.stats->>'saves')::NUMERIC;
    conceded   := (NEW.stats->>'goals_conceded')::NUMERIC;

    -- Per-90 metrics (requires minutes > 0)
    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF goals IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'goals_per_90', ROUND(goals * 90 / minutes, 3));
        END IF;
        IF assists IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'assists_per_90', ROUND(assists * 90 / minutes, 3));
        END IF;
        IF key_passes IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'key_passes_per_90', ROUND(key_passes * 90 / minutes, 3));
        END IF;
        IF shots_t IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'shots_per_90', ROUND(shots_t * 90 / minutes, 3));
        END IF;
        IF tackles IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'tackles_per_90', ROUND(tackles * 90 / minutes, 3));
        END IF;
        IF intercepts IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'interceptions_per_90', ROUND(intercepts * 90 / minutes, 3));
        END IF;
        -- Goalkeeper: goals conceded per 90
        IF conceded IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'goals_conceded_per_90', ROUND(conceded * 90 / minutes, 3));
        END IF;
    END IF;

    -- Accuracy rates (requires denominator > 0)
    IF shots_t IS NOT NULL AND shots_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1));
    END IF;

    IF passes_t IS NOT NULL AND passes_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1));
    END IF;

    IF duels_t IS NOT NULL AND duels_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1));
    END IF;

    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1));
    END IF;

    -- Goalkeeper: save percentage
    IF saves IS NOT NULL AND conceded IS NOT NULL THEN
        IF (saves + conceded) > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'save_pct', ROUND(saves / (saves + conceded) * 100, 1));
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger already exists from 003, CREATE OR REPLACE FUNCTION is sufficient.
-- The trigger definition itself (trg_football_derived_stats) doesn't change,
-- but we re-create it for clarity and to ensure it points to the updated function.
DROP TRIGGER IF EXISTS trg_football_derived_stats ON player_stats;
CREATE TRIGGER trg_football_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'FOOTBALL')
    EXECUTE FUNCTION compute_football_derived_stats();

-- ============================================================================
-- 6. UPDATE recalculate_percentiles() — READ FROM stat_definitions
-- ============================================================================
-- The function now reads inverse stat keys from the stat_definitions table
-- instead of requiring them to be passed as a parameter from Python.
-- The p_inverse_stats parameter is kept for backward compatibility but
-- is merged with the DB-sourced list (DB takes precedence).

CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT,
    p_season INTEGER,
    p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    -- Build inverse stats list from stat_definitions (canonical source)
    -- merged with any explicitly passed values (backward compatibility)
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions
        WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;

    -- Default to empty array if no inverse stats found
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- ========================================================================
    -- PLAYER PERCENTILES (partitioned by position for fair comparison)
    -- ========================================================================
    WITH stat_keys AS (
        -- Discover all numeric stat keys present for this sport/season
        SELECT DISTINCT key
        FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    player_positions AS (
        -- Get position for each player (for position-group partitioning)
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps
        JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        -- Expand JSONB stats into rows: one row per player per stat key
        SELECT
            ps.player_id,
            pp.position,
            sk.key AS stat_key,
            (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps
        CROSS JOIN stat_keys sk
        JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key
          AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        -- Calculate percent_rank within each position group per stat
        SELECT
            player_id,
            position,
            stat_key,
            CASE
                WHEN stat_key = ANY(v_inverse) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        -- Re-aggregate into one JSONB object per player
        SELECT
            player_id,
            position,
            max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_position_group', position,
                    '_sample_size', max(sample_size)
                ) AS percentiles_json
        FROM ranked
        GROUP BY player_id, position
    )
    UPDATE player_stats ps
    SET percentiles = agg.percentiles_json,
        updated_at = NOW()
    FROM aggregated agg
    WHERE ps.player_id = agg.player_id
      AND ps.sport = p_sport
      AND ps.season = p_season;

    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- ========================================================================
    -- TEAM PERCENTILES (all teams compared together, no position partitioning)
    -- ========================================================================
    WITH stat_keys AS (
        SELECT DISTINCT key
        FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT
            ts.team_id,
            sk.key AS stat_key,
            (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts
        CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season
          AND ts.stats ? sk.key
          AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT
            team_id,
            stat_key,
            CASE
                WHEN stat_key = ANY(v_inverse) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT
            team_id,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object('_sample_size', max(sample_size))
            AS percentiles_json
        FROM ranked
        GROUP BY team_id
    )
    UPDATE team_stats ts
    SET percentiles = agg.percentiles_json,
        updated_at = NOW()
    FROM aggregated agg
    WHERE ts.team_id = agg.team_id
      AND ts.sport = p_sport
      AND ts.season = p_season;

    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 7. DROP UNUSED GIN INDEXES
-- ============================================================================
-- These indexes support JSONB containment (@>) and existence (?) operators.
-- Our queries use ->> (text extraction) which cannot use GIN indexes.
-- They add write overhead on every INSERT/UPDATE with no read benefit.

DROP INDEX IF EXISTS idx_player_stats_gin;
DROP INDEX IF EXISTS idx_team_stats_gin;
DROP INDEX IF EXISTS idx_player_stats_percentiles_gin;
DROP INDEX IF EXISTS idx_team_stats_percentiles_gin;

-- ============================================================================
-- 8. SCHEMA VERSION BUMP
-- ============================================================================

UPDATE meta SET value = '5.4', updated_at = NOW() WHERE key = 'schema_version';
