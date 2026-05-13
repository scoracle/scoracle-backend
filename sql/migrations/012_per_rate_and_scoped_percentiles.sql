-- 012_per_rate_and_scoped_percentiles.sql
--
-- Two-part feature delta:
--
-- (1) Per-36 / per-90 expansion for NBA + Football player stats.
--     Adds derived rate keys for every volume stat that has a minutes denom,
--     so percentile comparisons normalize for playing time across all
--     counting categories (oreb, dreb, fgm/fga/etc. for NBA; xG, passes,
--     duels, dribbles, saves, etc. for Football).
--
-- (2) Scoped percentiles. Adds scoped_percentiles JSONB sibling to
--     player_stats / team_stats, populated by recalculate_percentiles().
--     - Players: partitioned by (position, scope) where scope is league_id
--       (FOOTBALL) or team conference (NBA/NFL).
--     - Teams: partitioned by scope only (no position).
--     Existing `percentiles` column (sport-wide, position-only) is left
--     untouched so the existing percentile NOTIFY trigger keeps firing
--     on the same diff.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/012_per_rate_and_scoped_percentiles.sql

BEGIN;

-- ============================================================================
-- 1. SCHEMA — add scoped_percentiles columns
-- ============================================================================

ALTER TABLE player_stats ADD COLUMN IF NOT EXISTS scoped_percentiles JSONB DEFAULT '{}';
ALTER TABLE team_stats   ADD COLUMN IF NOT EXISTS scoped_percentiles JSONB DEFAULT '{}';

-- ============================================================================
-- 2. STAT DEFINITIONS — new derived per-36 / per-90 entries
-- ============================================================================

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'oreb_per_36',        'Off Rebounds Per 36 Min','player', 'advanced',   false, true,  true,  41),
    ('NBA', 'dreb_per_36',        'Def Rebounds Per 36 Min','player', 'advanced',   false, true,  true,  42),
    ('NBA', 'fgm_per_36',         'FG Made Per 36 Min',     'player', 'advanced',   false, true,  true,  43),
    ('NBA', 'fga_per_36',         'FG Att Per 36 Min',      'player', 'advanced',   false, true,  true,  44),
    ('NBA', 'fg3m_per_36',        '3PT Made Per 36 Min',    'player', 'advanced',   false, true,  true,  45),
    ('NBA', 'fg3a_per_36',        '3PT Att Per 36 Min',     'player', 'advanced',   false, true,  true,  46),
    ('NBA', 'ftm_per_36',         'FT Made Per 36 Min',     'player', 'advanced',   false, true,  true,  47),
    ('NBA', 'fta_per_36',         'FT Att Per 36 Min',      'player', 'advanced',   false, true,  true,  48)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'expected_goals_per_90',      'xG Per 90',                 'player', 'scoring',    false, true,  true,  200),
    ('FOOTBALL', 'shots_on_target_per_90',     'Shots on Target/90',        'player', 'shooting',   false, true,  true,  201),
    ('FOOTBALL', 'passes_total_per_90',        'Passes Per 90',             'player', 'passing',    false, true,  true,  202),
    ('FOOTBALL', 'passes_accurate_per_90',     'Accurate Passes/90',        'player', 'passing',    false, true,  true,  203),
    ('FOOTBALL', 'crosses_total_per_90',       'Crosses Per 90',            'player', 'passing',    false, true,  true,  204),
    ('FOOTBALL', 'crosses_accurate_per_90',    'Accurate Crosses/90',       'player', 'passing',    false, true,  true,  205),
    ('FOOTBALL', 'clearances_per_90',          'Clearances Per 90',         'player', 'defensive',  false, true,  true,  206),
    ('FOOTBALL', 'blocks_per_90',              'Blocks Per 90',             'player', 'defensive',  false, true,  true,  207),
    ('FOOTBALL', 'duels_total_per_90',         'Duels Per 90',              'player', 'duels',      false, true,  true,  208),
    ('FOOTBALL', 'duels_won_per_90',           'Duels Won Per 90',          'player', 'duels',      false, true,  true,  209),
    ('FOOTBALL', 'dribbles_attempts_per_90',   'Dribble Attempts/90',       'player', 'dribbling',  false, true,  true,  210),
    ('FOOTBALL', 'dribbles_success_per_90',    'Successful Dribbles/90',    'player', 'dribbling',  false, true,  true,  211),
    ('FOOTBALL', 'saves_per_90',               'Saves Per 90',              'player', 'goalkeeper', false, true,  true,  212),
    ('FOOTBALL', 'saves_insidebox_per_90',     'Saves Inside Box/90',       'player', 'goalkeeper', false, true,  true,  213),
    ('FOOTBALL', 'chances_created_per_90',     'Chances Created Per 90',    'player', 'passing',    false, true,  true,  214),
    ('FOOTBALL', 'big_chances_created_per_90', 'Big Chances Created/90',    'player', 'passing',    false, true,  true,  215),
    ('FOOTBALL', 'long_balls_per_90',          'Long Balls Per 90',         'player', 'passing',    false, true,  true,  216),
    ('FOOTBALL', 'long_balls_won_per_90',      'Long Balls Won Per 90',     'player', 'passing',    false, true,  true,  217),
    ('FOOTBALL', 'through_balls_per_90',       'Through Balls Per 90',      'player', 'passing',    false, true,  true,  218),
    ('FOOTBALL', 'through_balls_won_per_90',   'Through Balls Won Per 90',  'player', 'passing',    false, true,  true,  219),
    ('FOOTBALL', 'passes_in_final_third_per_90','Final-Third Passes/90',    'player', 'passing',    false, true,  true,  220),
    ('FOOTBALL', 'tackles_won_per_90',         'Tackles Won Per 90',        'player', 'defensive',  false, true,  true,  221),
    ('FOOTBALL', 'dribbled_past_per_90',       'Dribbled Past Per 90',      'player', 'defensive',  true,  true,  true,  222),
    ('FOOTBALL', 'dispossessed_per_90',        'Dispossessed Per 90',       'player', 'possession', true,  true,  true,  223),
    ('FOOTBALL', 'possession_lost_per_90',     'Possession Lost Per 90',    'player', 'possession', true,  true,  true,  224),
    ('FOOTBALL', 'turnovers_per_90',           'Turnovers Per 90',          'player', 'possession', true,  true,  true,  225),
    ('FOOTBALL', 'ball_recovery_per_90',       'Ball Recoveries Per 90',    'player', 'defensive',  false, true,  true,  226),
    ('FOOTBALL', 'aerials_per_90',             'Aerials Per 90',            'player', 'duels',      false, true,  true,  227),
    ('FOOTBALL', 'aeriels_won_per_90',         'Aerials Won Per 90',        'player', 'duels',      false, true,  true,  228),
    ('FOOTBALL', 'fouls_committed_per_90',     'Fouls Committed Per 90',    'player', 'discipline', true,  true,  true,  229),
    ('FOOTBALL', 'fouls_drawn_per_90',         'Fouls Drawn Per 90',        'player', 'discipline', false, true,  true,  230)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 3. NBA player derived-stats trigger — loop over all per-36 keys
-- ============================================================================

CREATE OR REPLACE FUNCTION nba.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC;
    s TEXT;
    v NUMERIC;
    pts NUMERIC; reb NUMERIC; ast NUMERIC; stl NUMERIC; blk NUMERIC;
    fga NUMERIC; fgm NUMERIC; fta NUMERIC; ftm NUMERIC; turnover NUMERIC;
    tsa NUMERIC;
    -- 'turnover' is intentionally excluded — legacy alias 'tov_per_36' below.
    per_36_keys TEXT[] := ARRAY[
        'pts','reb','ast','stl','blk','pf',
        'oreb','dreb','fgm','fga','fg3m','fg3a','ftm','fta'
    ];
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

    IF minutes IS NOT NULL AND minutes > 0 THEN
        FOREACH s IN ARRAY per_36_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_36', ROUND(v / minutes * 36, 1));
                END IF;
            END IF;
        END LOOP;
        IF turnover IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('tov_per_36', ROUND(turnover / minutes * 36, 1));
        END IF;
    END IF;

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL AND fta IS NOT NULL AND ftm IS NOT NULL AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    IF fga IS NOT NULL AND fga > 0 AND fgm IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efg_pct', ROUND((fgm + 0.5 * COALESCE((NEW.stats->>'fg3m')::numeric, 0)) / fga * 100, 1));
    END IF;

    IF turnover IS NOT NULL AND turnover > 0 AND ast IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('ast_to_tov', ROUND(ast / turnover, 2));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 4. Football player derived-stats trigger — loop over all per-90 keys
-- ============================================================================

CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC;
    s TEXT;
    v NUMERIC;
    per_90_keys TEXT[] := ARRAY[
        'goals','assists','expected_goals','shots_on_target','key_passes',
        'passes_total','passes_accurate','crosses_total','crosses_accurate',
        'clearances','blocks','duels_total','duels_won','dribbles_attempts',
        'dribbles_success','saves','goals_conceded','saves_insidebox',
        'chances_created','big_chances_created','long_balls','long_balls_won',
        'through_balls','through_balls_won','passes_in_final_third','tackles',
        'tackles_won','interceptions','dribbled_past','dispossessed',
        'possession_lost','turnovers','ball_recovery','aerials','aeriels_won',
        'fouls_committed','fouls_drawn'
    ];
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC;
    tackles NUMERIC; tackles_w NUMERIC;
    crosses_t NUMERIC; crosses_a NUMERIC;
    long_balls_t NUMERIC; long_balls_w NUMERIC;
    aerials_t NUMERIC; aerials_w NUMERIC;
    saves NUMERIC; conceded NUMERIC;
BEGIN
    minutes := (NEW.stats->>'minutes_played')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        FOREACH s IN ARRAY per_90_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_90', ROUND(v * 90 / minutes, 3));
                END IF;
            END IF;
        END LOOP;
        IF NEW.stats ? 'shots_total' THEN
            v := (NEW.stats->>'shots_total')::NUMERIC;
            IF v IS NOT NULL THEN
                NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(v * 90 / minutes, 3));
            END IF;
        END IF;
    END IF;

    shots_t      := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on     := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t     := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a     := (NEW.stats->>'passes_accurate')::NUMERIC;
    duels_t      := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w      := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a   := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s   := (NEW.stats->>'dribbles_success')::NUMERIC;
    tackles      := (NEW.stats->>'tackles')::NUMERIC;
    tackles_w    := (NEW.stats->>'tackles_won')::NUMERIC;
    crosses_t    := (NEW.stats->>'crosses_total')::NUMERIC;
    crosses_a    := (NEW.stats->>'crosses_accurate')::NUMERIC;
    long_balls_t := (NEW.stats->>'long_balls')::NUMERIC;
    long_balls_w := (NEW.stats->>'long_balls_won')::NUMERIC;
    aerials_t    := (NEW.stats->>'aerials')::NUMERIC;
    aerials_w    := (NEW.stats->>'aeriels_won')::NUMERIC;
    saves        := (NEW.stats->>'saves')::NUMERIC;
    conceded     := (NEW.stats->>'goals_conceded')::NUMERIC;

    IF shots_t IS NOT NULL AND shots_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1)); END IF;
    IF passes_t IS NOT NULL AND passes_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1)); END IF;
    IF duels_t IS NOT NULL AND duels_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1)); END IF;
    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1)); END IF;
    IF tackles IS NOT NULL AND tackles > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('tackles_won_percentage', ROUND(COALESCE(tackles_w, 0) / tackles * 100, 1)); END IF;
    IF crosses_t IS NOT NULL AND crosses_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('cross_accuracy', ROUND(COALESCE(crosses_a, 0) / crosses_t * 100, 1)); END IF;
    IF long_balls_t IS NOT NULL AND long_balls_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('long_ball_accuracy', ROUND(COALESCE(long_balls_w, 0) / long_balls_t * 100, 1)); END IF;
    IF aerials_t IS NOT NULL AND aerials_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('aerials_won_percentage', ROUND(COALESCE(aerials_w, 0) / aerials_t * 100, 1)); END IF;
    IF saves IS NOT NULL AND conceded IS NOT NULL AND (saves + conceded) > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('save_pct', ROUND(saves / (saves + conceded) * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 5. recalculate_percentiles — extended to also write scoped_percentiles
-- ============================================================================

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
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (sport-wide, partitioned by position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_positions AS (
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT ps.player_id, pp.position, sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ps.player_id = agg.player_id AND ps.sport = p_sport AND ps.season = p_season;
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Team percentiles (sport-wide, no position partition)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ts.team_id = agg.team_id AND ts.sport = p_sport AND ts.season = p_season;
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    -- Player scoped percentiles (position, league|conference)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_meta AS (
        SELECT ps.player_id, ps.league_id,
            COALESCE(p.position, 'Unknown') AS position,
            CASE WHEN p_sport = 'FOOTBALL' THEN 'league' ELSE 'conference' END AS scope_type,
            CASE WHEN p_sport = 'FOOTBALL' THEN ps.league_id::text
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN COALESCE(l.name, 'Unknown')
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_name
        FROM player_stats ps
        JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        LEFT JOIN teams t ON t.id = ps.team_id AND t.sport = ps.sport
        LEFT JOIN leagues l ON l.id = ps.league_id
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT pm.player_id, pm.league_id, pm.position, pm.scope_type, pm.scope_id, pm.scope_name,
            sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps
        JOIN player_meta pm ON pm.player_id = ps.player_id AND pm.league_id = ps.league_id
        CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
            AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, league_id, position, scope_type, scope_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, scope_type, scope_id, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, league_id, position, scope_type, scope_id, scope_name,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_position_group', position,
                    '_sample_size', max(sample_size),
                    'scope_type', scope_type,
                    'scope_id', scope_id,
                    'scope_name', scope_name
                ) AS scoped_json
        FROM ranked
        GROUP BY player_id, league_id, position, scope_type, scope_id, scope_name
    )
    UPDATE player_stats ps
    SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg
    WHERE ps.player_id = agg.player_id
      AND ps.league_id = agg.league_id
      AND ps.sport = p_sport AND ps.season = p_season;

    -- Team scoped percentiles (league|conference, no position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    team_meta AS (
        SELECT ts.team_id, ts.league_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN 'league' ELSE 'conference' END AS scope_type,
            CASE WHEN p_sport = 'FOOTBALL' THEN ts.league_id::text
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN COALESCE(l.name, 'Unknown')
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_name
        FROM team_stats ts
        JOIN teams t ON t.id = ts.team_id AND t.sport = ts.sport
        LEFT JOIN leagues l ON l.id = ts.league_id
        WHERE ts.sport = p_sport AND ts.season = p_season
    ),
    expanded AS (
        SELECT tm.team_id, tm.league_id, tm.scope_type, tm.scope_id, tm.scope_name,
            sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts
        JOIN team_meta tm ON tm.team_id = ts.team_id AND tm.league_id = ts.league_id
        CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season
            AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, league_id, scope_type, scope_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY scope_type, scope_id, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, league_id, scope_type, scope_id, scope_name,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_sample_size', max(sample_size),
                    'scope_type', scope_type,
                    'scope_id', scope_id,
                    'scope_name', scope_name
                ) AS scoped_json
        FROM ranked
        GROUP BY team_id, league_id, scope_type, scope_id, scope_name
    )
    UPDATE team_stats ts
    SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg
    WHERE ts.team_id = agg.team_id
      AND ts.league_id = agg.league_id
      AND ts.sport = p_sport AND ts.season = p_season;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 6. Views — expose scoped_percentiles + scoped_percentile_metadata
--
-- DROP+CREATE (not CREATE OR REPLACE) because we're inserting new columns
-- into the middle of the column list. Postgres' CREATE OR REPLACE VIEW only
-- allows appending columns at the end; reordering / mid-list inserts are
-- rejected as a column rename. No external views/matviews depend on these
-- six views (verified via pg_depend), so drop is safe.
-- ============================================================================

DROP VIEW IF EXISTS nba.player;
DROP VIEW IF EXISTS nba.team;
DROP VIEW IF EXISTS nfl.player;
DROP VIEW IF EXISTS nfl.team;
DROP VIEW IF EXISTS football.player;
DROP VIEW IF EXISTS football.team;

-- NBA player
CREATE VIEW nba.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city,
        'conference', t.conference, 'division', t.division
    ) END AS team,
    ps.season,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'NBA';

-- NBA team
CREATE VIEW nba.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division,
    t.venue_name, t.venue_capacity,
    ts.season,
    ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE
        WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0))
    END AS percentile_metadata,
    ts.scoped_percentiles - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ts.scoped_percentiles->>'scope_type',
            'scope_id', ts.scoped_percentiles->>'scope_id',
            'scope_name', ts.scoped_percentiles->>'scope_name',
            'sample_size', COALESCE((ts.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NBA';

-- NFL player
CREATE VIEW nfl.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city,
        'conference', t.conference, 'division', t.division
    ) END AS team,
    ps.season,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'NFL';

-- NFL team
CREATE VIEW nfl.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division,
    t.venue_name, t.venue_capacity,
    ts.season,
    ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE
        WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0))
    END AS percentile_metadata,
    ts.scoped_percentiles - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ts.scoped_percentiles->>'scope_type',
            'scope_id', ts.scoped_percentiles->>'scope_id',
            'scope_name', ts.scoped_percentiles->>'scope_name',
            'sample_size', COALESCE((ts.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NFL';

-- Football player
CREATE VIEW football.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city
    ) END AS team,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url
    ) END AS league,
    ps.season,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.leagues l ON l.id = p.league_id
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'FOOTBALL';

-- Football team
CREATE VIEW football.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.venue_name, t.venue_capacity,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url
    ) END AS league,
    ts.season,
    ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE
        WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0))
    END AS percentile_metadata,
    ts.scoped_percentiles - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ts.scoped_percentiles->>'scope_type',
            'scope_id', ts.scoped_percentiles->>'scope_id',
            'scope_name', ts.scoped_percentiles->>'scope_name',
            'sample_size', COALESCE((ts.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.leagues l ON l.id = t.league_id
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'FOOTBALL';

-- ============================================================================
-- 7. Backfill — refire derived triggers, then recompute percentiles per
--    (sport, season). The no-op UPDATE on `stats` triggers BEFORE INSERT OR
--    UPDATE on player_stats which writes the new per-36 / per-90 keys.
--    NOTIFY trigger keys off `percentiles` UPDATE, not `stats`, so this
--    backfill does not spam pg_notify.
-- ============================================================================

UPDATE player_stats SET stats = stats WHERE sport IN ('NBA', 'FOOTBALL');

DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT DISTINCT sport, season FROM player_stats
        UNION
        SELECT DISTINCT sport, season FROM team_stats
    LOOP
        PERFORM recalculate_percentiles(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;
