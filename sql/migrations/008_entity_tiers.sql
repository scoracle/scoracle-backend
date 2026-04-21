-- Entity tiering for vibe scheduling.
--
-- Tier values and what they mean:
--   headliner  — real-time vibe on any qualifying milestone event.
--                Expected ~150 per sport + all teams (small, high-value).
--   starter    — daily batch vibe if they played that day. Covers the
--                long tail of active contributors.
--   bench      — has played at some point but below the starter bar.
--                No vibe coverage unless they promote.
--   inactive   — no activity this season. Purge candidates.
--
-- Teams default to 'headliner' because there are few of them (~158
-- total across sports) and they're always newsworthy.
--
-- Recompute with:
--   SELECT * FROM recompute_entity_tiers('NBA', 2025);
-- Run weekly from cron; cheap (<1s per sport).

ALTER TABLE players
    ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'inactive'
    CHECK (tier IN ('headliner', 'starter', 'bench', 'inactive'));

ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'headliner'
    CHECK (tier IN ('headliner', 'starter', 'bench', 'inactive'));

CREATE INDEX IF NOT EXISTS idx_players_tier_sport ON players(tier, sport);
CREATE INDEX IF NOT EXISTS idx_teams_tier_sport   ON teams(tier, sport);


-- recompute_entity_tiers(sport, season):
--   Walks this season's event_box_scores to rank players, then writes
--   a single UPDATE per player setting the appropriate tier. Teams
--   always 'headliner'; the function still touches teams to reset any
--   misconfigured rows.
--
-- Returns the count landing in each tier for observability.

CREATE OR REPLACE FUNCTION recompute_entity_tiers(
    p_sport TEXT,
    p_season INTEGER,
    p_headliner_limit INTEGER DEFAULT 150
)
RETURNS TABLE (tier TEXT, entity_count BIGINT) AS $$
DECLARE
    v_starter_min_apps INTEGER;
    v_starter_min_minutes_per_app NUMERIC;
BEGIN
    -- Per-sport starter thresholds. Same baseline heuristics we use
    -- elsewhere for "has this entity played enough to matter."
    CASE p_sport
        WHEN 'NBA' THEN
            v_starter_min_apps := 20;
            v_starter_min_minutes_per_app := 15.0;
        WHEN 'NFL' THEN
            v_starter_min_apps := 5;
            v_starter_min_minutes_per_app := 0.0;  -- no MPG in NFL box scores
        WHEN 'FOOTBALL' THEN
            v_starter_min_apps := 8;
            v_starter_min_minutes_per_app := 60.0;
        ELSE
            RAISE EXCEPTION 'unsupported sport: %', p_sport;
    END CASE;

    -- Step 1: compute per-player aggregate for the season.
    CREATE TEMP TABLE player_season_agg ON COMMIT DROP AS
    SELECT
        ebs.player_id,
        ebs.sport,
        count(*) AS apps,
        COALESCE(avg(ebs.minutes_played), 0) AS avg_mpa,
        COALESCE(sum(ebs.minutes_played), 0) AS total_minutes
    FROM event_box_scores ebs
    JOIN fixtures f ON f.id = ebs.fixture_id
    WHERE ebs.sport = p_sport
      AND f.season = p_season
    GROUP BY ebs.player_id, ebs.sport;

    -- Step 2: figure out who's a headliner (top N by total_minutes among
    -- players who clear the starter bar).
    CREATE TEMP TABLE player_tiers ON COMMIT DROP AS
    WITH qualified AS (
        SELECT player_id, sport, apps, avg_mpa, total_minutes
        FROM player_season_agg
        WHERE apps >= v_starter_min_apps
          AND avg_mpa >= v_starter_min_minutes_per_app
    ),
    ranked AS (
        SELECT *, ROW_NUMBER() OVER (ORDER BY total_minutes DESC) AS rnk
        FROM qualified
    )
    SELECT
        player_id, sport,
        CASE
            WHEN rnk <= p_headliner_limit THEN 'headliner'
            ELSE 'starter'
        END AS tier
    FROM ranked;

    -- Step 3: reset everyone in this sport to 'inactive', then apply
    -- the computed tiers. A player with any box score this season but
    -- below the starter bar lands on 'bench'.
    UPDATE players SET tier = 'inactive', updated_at = NOW()
    WHERE sport = p_sport;

    UPDATE players p SET tier = 'bench', updated_at = NOW()
    FROM player_season_agg agg
    WHERE p.id = agg.player_id AND p.sport = agg.sport;

    UPDATE players p SET tier = pt.tier, updated_at = NOW()
    FROM player_tiers pt
    WHERE p.id = pt.player_id AND p.sport = pt.sport;

    -- Teams are always headliner in this model.
    UPDATE teams t SET tier = 'headliner', updated_at = NOW()
    WHERE t.sport = p_sport AND t.tier <> 'headliner';

    -- Return summary. Fully qualify every `tier` reference — the
    -- OUT parameter shadows the column name otherwise.
    RETURN QUERY
    SELECT p.tier, count(*)::BIGINT
    FROM players p
    WHERE p.sport = p_sport
    GROUP BY p.tier
    UNION ALL
    SELECT 'team_headliner'::TEXT, count(*)::BIGINT
    FROM teams t
    WHERE t.sport = p_sport AND t.tier = 'headliner'
    ORDER BY 1;
END;
$$ LANGUAGE plpgsql;
