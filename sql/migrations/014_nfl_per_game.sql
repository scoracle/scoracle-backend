-- 014_nfl_per_game.sql
--
-- Per-game derived stats for NFL players, mirroring NBA's per-36 and
-- Football's per-90 expansions in migration 012. NFL doesn't have a
-- per-snap denominator (BDL doesn't ship snap counts), so games_played
-- is the rate normalizer: every counting/volume stat gets a *_per_game
-- sibling, computed by the player-stats BEFORE trigger.
--
-- Three yardage-per-game keys (passing_, rushing_, receiving_yards_per_game)
-- already exist as bespoke outputs of nfl.aggregate_player_season — they're
-- listed in the loop too so the trigger overwrites with an identical value;
-- no new stat_definitions row needed for them.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/014_nfl_per_game.sql

BEGIN;

-- ============================================================================
-- 1. STAT DEFINITIONS — new derived per-game entries
-- ============================================================================

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    -- Passing
    ('NFL', 'passing_completions_per_game',   'Completions/Game',        'player', 'passing',   false, true, true, 100),
    ('NFL', 'passing_attempts_per_game',      'Pass Attempts/Game',      'player', 'passing',   false, true, true, 101),
    ('NFL', 'passing_touchdowns_per_game',    'Passing TDs/Game',        'player', 'passing',   false, true, true, 102),
    ('NFL', 'passing_interceptions_per_game', 'INTs Thrown/Game',        'player', 'passing',   true,  true, true, 103),
    ('NFL', 'sacks_taken_per_game',           'Sacks Taken/Game',        'player', 'passing',   true,  true, true, 104),
    ('NFL', 'sack_yards_lost_per_game',       'Sack Yards Lost/Game',    'player', 'passing',   true,  true, true, 105),
    -- Rushing
    ('NFL', 'rushing_attempts_per_game',      'Rush Attempts/Game',      'player', 'rushing',   false, true, true, 110),
    ('NFL', 'rushing_touchdowns_per_game',    'Rush TDs/Game',           'player', 'rushing',   false, true, true, 111),
    ('NFL', 'rushing_first_downs_per_game',   'Rush First Downs/Game',   'player', 'rushing',   false, true, true, 112),
    -- Receiving
    ('NFL', 'receptions_per_game',            'Receptions/Game',         'player', 'receiving', false, true, true, 120),
    ('NFL', 'receiving_targets_per_game',     'Targets/Game',            'player', 'receiving', false, true, true, 121),
    ('NFL', 'receiving_touchdowns_per_game',  'Receiving TDs/Game',      'player', 'receiving', false, true, true, 122),
    ('NFL', 'receiving_first_downs_per_game', 'Rec First Downs/Game',    'player', 'receiving', false, true, true, 123),
    -- Defense
    ('NFL', 'total_tackles_per_game',         'Total Tackles/Game',      'player', 'defensive', false, true, true, 130),
    ('NFL', 'solo_tackles_per_game',          'Solo Tackles/Game',       'player', 'defensive', false, true, true, 131),
    ('NFL', 'assist_tackles_per_game',        'Assist Tackles/Game',     'player', 'defensive', false, true, true, 132),
    ('NFL', 'defensive_sacks_per_game',       'Sacks/Game',              'player', 'defensive', false, true, true, 133),
    ('NFL', 'defensive_sack_yards_per_game',  'Sack Yards/Game',         'player', 'defensive', false, true, true, 134),
    ('NFL', 'defensive_interceptions_per_game','INTs/Game',              'player', 'defensive', false, true, true, 135),
    ('NFL', 'interception_touchdowns_per_game','INT Return TDs/Game',    'player', 'defensive', false, true, true, 136),
    ('NFL', 'interception_yards_per_game',    'INT Return Yards/Game',   'player', 'defensive', false, true, true, 137),
    ('NFL', 'fumbles_forced_per_game',        'Forced Fumbles/Game',     'player', 'defensive', false, true, true, 138),
    ('NFL', 'fumbles_recovered_per_game',     'Fumbles Recovered/Game',  'player', 'defensive', false, true, true, 139),
    ('NFL', 'fumbles_touchdowns_per_game',    'Fumble Return TDs/Game',  'player', 'defensive', false, true, true, 140),
    ('NFL', 'tackles_for_loss_per_game',      'TFL/Game',                'player', 'defensive', false, true, true, 141),
    ('NFL', 'passes_defended_per_game',       'Passes Defended/Game',    'player', 'defensive', false, true, true, 142),
    ('NFL', 'qb_hits_per_game',               'QB Hits/Game',            'player', 'defensive', false, true, true, 143),
    -- Ball security
    ('NFL', 'fumbles_per_game',               'Fumbles/Game',            'player', 'general',   true,  true, true, 150),
    ('NFL', 'fumbles_lost_per_game',          'Fumbles Lost/Game',       'player', 'general',   true,  true, true, 151),
    -- Kicking
    ('NFL', 'field_goal_attempts_per_game',   'FG Attempts/Game',        'player', 'kicking',   false, true, true, 160),
    ('NFL', 'field_goals_made_per_game',      'FG Made/Game',            'player', 'kicking',   false, true, true, 161),
    ('NFL', 'extra_points_made_per_game',     'XP Made/Game',            'player', 'kicking',   false, true, true, 162),
    ('NFL', 'total_points_per_game',          'Points/Game',             'player', 'kicking',   false, true, true, 163),
    ('NFL', 'touchbacks_per_game',            'Touchbacks/Game',         'player', 'kicking',   false, true, true, 164),
    -- Special teams
    ('NFL', 'punts_per_game',                 'Punts/Game',              'player', 'special',   false, true, true, 170),
    ('NFL', 'punt_yards_per_game',            'Punt Yards/Game',         'player', 'special',   false, true, true, 171),
    ('NFL', 'punts_inside_20_per_game',       'Punts Inside 20/Game',    'player', 'special',   false, true, true, 172),
    ('NFL', 'kick_returns_per_game',          'Kick Returns/Game',       'player', 'special',   false, true, true, 173),
    ('NFL', 'kick_return_yards_per_game',     'Kick Return Yards/Game',  'player', 'special',   false, true, true, 174),
    ('NFL', 'kick_return_touchdowns_per_game','Kick Return TDs/Game',    'player', 'special',   false, true, true, 175),
    ('NFL', 'punt_returner_returns_per_game', 'Punt Returns/Game',       'player', 'special',   false, true, true, 176),
    ('NFL', 'punt_returner_return_yards_per_game','Punt Return Yards/Game','player','special',  false, true, true, 177),
    ('NFL', 'punt_return_touchdowns_per_game','Punt Return TDs/Game',    'player', 'special',   false, true, true, 178)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2. NFL player derived-stats trigger — loop over all per-game keys
-- ============================================================================

CREATE OR REPLACE FUNCTION nfl.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    gp NUMERIC;
    s TEXT;
    v NUMERIC;
    pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
    per_game_keys TEXT[] := ARRAY[
        -- Passing
        'passing_completions','passing_attempts','passing_yards',
        'passing_touchdowns','passing_interceptions','sacks_taken','sack_yards_lost',
        -- Rushing
        'rushing_attempts','rushing_yards','rushing_touchdowns','rushing_first_downs',
        -- Receiving
        'receptions','receiving_targets','receiving_yards','receiving_touchdowns','receiving_first_downs',
        -- Defense
        'total_tackles','solo_tackles','assist_tackles','defensive_sacks','defensive_sack_yards',
        'defensive_interceptions','interception_touchdowns','interception_yards',
        'fumbles_forced','fumbles_recovered','fumbles_touchdowns',
        'tackles_for_loss','passes_defended','qb_hits',
        -- Ball security
        'fumbles','fumbles_lost',
        -- Kicking
        'field_goal_attempts','field_goals_made','extra_points_made','total_points','touchbacks',
        -- Special teams
        'punts','punt_yards','punts_inside_20',
        'kick_returns','kick_return_yards','kick_return_touchdowns',
        'punt_returner_returns','punt_returner_return_yards','punt_return_touchdowns'
    ];
BEGIN
    gp       := (NEW.stats->>'games_played')::NUMERIC;
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;

    IF gp IS NOT NULL AND gp > 0 THEN
        FOREACH s IN ARRAY per_game_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_game', ROUND(v / gp, 2));
                END IF;
            END IF;
        END LOOP;
    END IF;

    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('catch_pct', ROUND(rec / targets * 100, 1));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMIT;
