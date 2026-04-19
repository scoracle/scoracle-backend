-- Scoracle Data — Football (Soccer) Schema
-- Owner: Football product owner
-- Contains: Football-specific views, stat definitions, triggers, functions, grants
-- Depends on: sql/shared.sql (public tables must exist first)

CREATE SCHEMA IF NOT EXISTS football;

-- ============================================================================
-- 1. STAT DEFINITIONS
-- ============================================================================

-- Football player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'appearances',           'Appearances',          'player', 'general',    false, false, false,  1),
    ('FOOTBALL', 'lineups',               'Starting Lineups',     'player', 'general',    false, false, false,  2),
    ('FOOTBALL', 'minutes_played',        'Minutes Played',       'player', 'general',    false, false, true,   3),
    ('FOOTBALL', 'goals',                 'Goals',                'player', 'scoring',    false, false, true,  10),
    ('FOOTBALL', 'assists',               'Assists',              'player', 'scoring',    false, false, true,  11),
    ('FOOTBALL', 'expected_goals',        'Expected Goals (xG)',  'player', 'scoring',    false, false, true,  12),
    ('FOOTBALL', 'goals_per_90',          'Goals Per 90',         'player', 'scoring',    false, true,  true,  13),
    ('FOOTBALL', 'assists_per_90',        'Assists Per 90',       'player', 'scoring',    false, true,  true,  14),
    ('FOOTBALL', 'shots_total',           'Total Shots',          'player', 'shooting',   false, false, false, 20),
    ('FOOTBALL', 'shots_on_target',       'Shots on Target',      'player', 'shooting',   false, false, false, 21),
    ('FOOTBALL', 'shots_per_90',          'Shots Per 90',         'player', 'shooting',   false, true,  true,  22),
    ('FOOTBALL', 'shot_accuracy',         'Shot Accuracy %',      'player', 'shooting',   false, true,  true,  23),
    ('FOOTBALL', 'passes_total',          'Total Passes',         'player', 'passing',    false, false, false, 30),
    ('FOOTBALL', 'passes_accurate',       'Accurate Passes',      'player', 'passing',    false, false, false, 31),
    ('FOOTBALL', 'key_passes',            'Key Passes',           'player', 'passing',    false, false, true,  32),
    ('FOOTBALL', 'crosses_total',         'Total Crosses',        'player', 'passing',    false, false, false, 33),
    ('FOOTBALL', 'crosses_accurate',      'Accurate Crosses',     'player', 'passing',    false, false, false, 34),
    ('FOOTBALL', 'key_passes_per_90',     'Key Passes Per 90',    'player', 'passing',    false, true,  true,  35),
    ('FOOTBALL', 'pass_accuracy',         'Pass Accuracy %',      'player', 'passing',    false, true,  true,  36),
    ('FOOTBALL', 'tackles',               'Tackles',              'player', 'defensive',  false, false, true,  40),
    ('FOOTBALL', 'interceptions',         'Interceptions',        'player', 'defensive',  false, false, true,  41),
    ('FOOTBALL', 'clearances',            'Clearances',           'player', 'defensive',  false, false, false, 42),
    ('FOOTBALL', 'blocks',                'Blocks',               'player', 'defensive',  false, false, false, 43),
    ('FOOTBALL', 'tackles_per_90',        'Tackles Per 90',       'player', 'defensive',  false, true,  true,  44),
    ('FOOTBALL', 'interceptions_per_90',  'Interceptions/90',     'player', 'defensive',  false, true,  true,  45),
    ('FOOTBALL', 'duels_total',           'Total Duels',          'player', 'duels',      false, false, false, 50),
    ('FOOTBALL', 'duels_won',             'Duels Won',            'player', 'duels',      false, false, false, 51),
    ('FOOTBALL', 'duel_success_rate',     'Duel Success Rate %',  'player', 'duels',      false, true,  true,  52),
    ('FOOTBALL', 'dribbles_attempts',     'Dribble Attempts',     'player', 'dribbling',  false, false, false, 55),
    ('FOOTBALL', 'dribbles_success',      'Successful Dribbles',  'player', 'dribbling',  false, false, false, 56),
    ('FOOTBALL', 'dribble_success_rate',  'Dribble Success %',    'player', 'dribbling',  false, true,  true,  57),
    ('FOOTBALL', 'yellow_cards',          'Yellow Cards',         'player', 'discipline', true,  false, true,  60),
    ('FOOTBALL', 'red_cards',             'Red Cards',            'player', 'discipline', true,  false, true,  61),
    ('FOOTBALL', 'fouls_committed',       'Fouls Committed',      'player', 'discipline', true,  false, false, 62),
    ('FOOTBALL', 'fouls_drawn',           'Fouls Drawn',          'player', 'discipline', false, false, false, 63),
    ('FOOTBALL', 'saves',                 'Saves',                'player', 'goalkeeper', false, false, true,  70),
    ('FOOTBALL', 'goals_conceded',        'Goals Conceded',       'player', 'goalkeeper', true,  false, true,  71),
    ('FOOTBALL', 'goals_conceded_per_90', 'Goals Conceded/90',    'player', 'goalkeeper', true,  true,  true,  72),
    ('FOOTBALL', 'save_pct',              'Save Percentage %',    'player', 'goalkeeper', false, true,  true,  73),
    ('FOOTBALL', 'saves_insidebox',       'Saves Inside Box',     'player', 'goalkeeper', false, false, true,  74),
    ('FOOTBALL', 'punches',               'Punches',              'player', 'goalkeeper', false, false, false, 75),
    ('FOOTBALL', 'penalties_saved',       'Penalties Saved',      'player', 'goalkeeper', false, false, true,  76),
    ('FOOTBALL', 'good_high_claim',       'High Claims',          'player', 'goalkeeper', false, false, false, 77),
    ('FOOTBALL', 'penalty_goals',         'Penalty Goals',        'player', 'scoring',    false, false, false, 15),
    ('FOOTBALL', 'penalties_missed',      'Penalties Missed',     'player', 'scoring',    true,  false, false, 16),
    ('FOOTBALL', 'penalties_won',         'Penalties Won',        'player', 'scoring',    false, false, false, 17),
    ('FOOTBALL', 'penalties_committed',   'Penalties Committed',  'player', 'discipline', true,  false, false, 64),
    ('FOOTBALL', 'penalties_scored',      'Penalties Scored',     'player', 'scoring',    false, false, false, 18),
    ('FOOTBALL', 'own_goals',             'Own Goals',            'player', 'scoring',    true,  false, false, 19),
    ('FOOTBALL', 'yellowred_cards',       'Second Yellows',       'player', 'discipline', true,  false, false, 65),
    ('FOOTBALL', 'hit_woodwork',          'Hit Woodwork',         'player', 'shooting',   false, false, false, 24),
    ('FOOTBALL', 'shots_off_target',      'Shots off Target',     'player', 'shooting',   false, false, false, 25),
    ('FOOTBALL', 'shots_blocked',         'Shots Blocked',        'player', 'shooting',   true,  false, false, 26),
    ('FOOTBALL', 'chances_created',       'Chances Created',      'player', 'passing',    false, false, true,  37),
    ('FOOTBALL', 'big_chances_created',   'Big Chances Created',  'player', 'passing',    false, false, true,  38),
    ('FOOTBALL', 'big_chances_missed',    'Big Chances Missed',   'player', 'shooting',   true,  false, true,  27),
    ('FOOTBALL', 'long_balls',            'Long Balls',           'player', 'passing',    false, false, false, 39),
    ('FOOTBALL', 'long_balls_won',        'Long Balls Won',       'player', 'passing',    false, false, true,  46),
    ('FOOTBALL', 'long_ball_accuracy',    'Long Ball Accuracy %', 'player', 'passing',    false, true,  true,  47),
    ('FOOTBALL', 'through_balls',         'Through Balls',        'player', 'passing',    false, false, true,  48),
    ('FOOTBALL', 'through_balls_won',     'Through Balls Won',    'player', 'passing',    false, false, true,  49),
    ('FOOTBALL', 'backward_passes',       'Backward Passes',      'player', 'passing',    false, false, false, 66),
    ('FOOTBALL', 'passes_in_final_third', 'Passes in Final Third','player', 'passing',    false, false, true,  67),
    ('FOOTBALL', 'cross_accuracy',        'Cross Accuracy %',     'player', 'passing',    false, true,  true,  68),
    ('FOOTBALL', 'tackles_won',           'Tackles Won',          'player', 'defensive',  false, false, true,  78),
    ('FOOTBALL', 'tackles_won_percentage','Tackle Success %',     'player', 'defensive',  false, true,  true,  79),
    ('FOOTBALL', 'last_man_tackle',       'Last Man Tackles',     'player', 'defensive',  false, false, false, 80),
    ('FOOTBALL', 'clearance_offline',     'Goal-line Clearances', 'player', 'defensive',  false, false, false, 81),
    ('FOOTBALL', 'error_lead_to_shot',    'Errors Leading to Shot','player','defensive',  true,  false, false, 82),
    ('FOOTBALL', 'error_lead_to_goal',    'Errors Leading to Goal','player','defensive',  true,  false, false, 83),
    ('FOOTBALL', 'duels_lost',            'Duels Lost',           'player', 'duels',      true,  false, false, 53),
    ('FOOTBALL', 'aerials',               'Aerial Duels',         'player', 'duels',      false, false, false, 54),
    ('FOOTBALL', 'aeriels_won',           'Aerials Won',          'player', 'duels',      false, false, true,  58),
    ('FOOTBALL', 'aeriels_lost',          'Aerials Lost',         'player', 'duels',      true,  false, false, 59),
    ('FOOTBALL', 'aerials_won_percentage','Aerial Success %',     'player', 'duels',      false, true,  true,  69),
    ('FOOTBALL', 'dribbled_past',         'Dribbled Past',        'player', 'defensive',  true,  false, true,  84),
    ('FOOTBALL', 'dispossessed',          'Dispossessed',         'player', 'possession', true,  false, true,  85),
    ('FOOTBALL', 'possession_lost',       'Possession Lost',      'player', 'possession', true,  false, true,  86),
    ('FOOTBALL', 'turnovers',             'Turnovers',            'player', 'possession', true,  false, true,  87),
    ('FOOTBALL', 'touches',               'Touches',              'player', 'possession', false, false, false, 88),
    ('FOOTBALL', 'ball_recovery',         'Ball Recoveries',      'player', 'defensive',  false, false, true,  89),
    ('FOOTBALL', 'offsides',              'Offsides',             'player', 'discipline', true,  false, false, 90),
    ('FOOTBALL', 'offsides_provoked',     'Offsides Won',         'player', 'defensive',  false, false, false, 91),
    ('FOOTBALL', 'motm_awards',           'Man of the Match',     'player', 'general',    false, false, true,   4),
    ('FOOTBALL', 'rating_avg',            'Average Match Rating', 'player', 'general',    false, true,  true,   5)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Football team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'matches_played',  'Matches Played',       'team', 'standings', false, false, false,  1),
    ('FOOTBALL', 'wins',            'Wins',                 'team', 'standings', false, false, true,   2),
    ('FOOTBALL', 'draws',           'Draws',                'team', 'standings', false, false, false,  3),
    ('FOOTBALL', 'losses',          'Losses',               'team', 'standings', true,  false, true,   4),
    ('FOOTBALL', 'goals_for',       'Goals For',            'team', 'scoring',   false, false, true,   5),
    ('FOOTBALL', 'goals_against',   'Goals Against',        'team', 'scoring',   true,  false, true,   6),
    ('FOOTBALL', 'goal_difference', 'Goal Difference',      'team', 'scoring',   false, false, true,   7),
    ('FOOTBALL', 'points',          'Points',               'team', 'standings', false, false, true,   8),
    ('FOOTBALL', 'overall_points',  'Overall Points',       'team', 'standings', false, false, false,  9),
    ('FOOTBALL', 'position',        'League Position',      'team', 'standings', false, false, false, 10),
    ('FOOTBALL', 'home_played',     'Home Matches',         'team', 'home',      false, false, false, 20),
    ('FOOTBALL', 'home_won',        'Home Wins',            'team', 'home',      false, false, false, 21),
    ('FOOTBALL', 'home_draw',       'Home Draws',           'team', 'home',      false, false, false, 22),
    ('FOOTBALL', 'home_lost',       'Home Losses',          'team', 'home',      false, false, false, 23),
    ('FOOTBALL', 'home_scored',     'Home Goals Scored',    'team', 'home',      false, false, false, 24),
    ('FOOTBALL', 'home_conceded',   'Home Goals Conceded',  'team', 'home',      false, false, false, 25),
    ('FOOTBALL', 'home_points',     'Home Points',          'team', 'home',      false, false, false, 26),
    ('FOOTBALL', 'away_played',     'Away Matches',         'team', 'away',      false, false, false, 30),
    ('FOOTBALL', 'away_won',        'Away Wins',            'team', 'away',      false, false, false, 31),
    ('FOOTBALL', 'away_draw',       'Away Draws',           'team', 'away',      false, false, false, 32),
    ('FOOTBALL', 'away_lost',       'Away Losses',          'team', 'away',      false, false, false, 33),
    ('FOOTBALL', 'away_scored',     'Away Goals Scored',    'team', 'away',      false, false, false, 34),
    ('FOOTBALL', 'away_conceded',   'Away Goals Conceded',  'team', 'away',      false, false, false, 35),
    ('FOOTBALL', 'away_points',     'Away Points',          'team', 'away',      false, false, false, 36),
    ('FOOTBALL', 'fouls_committed',       'Fouls Committed',          'team', 'discipline', true,  false, true,   40),
    ('FOOTBALL', 'yellow_cards_total',    'Yellow Cards',             'team', 'discipline', true,  false, true,   41),
    ('FOOTBALL', 'red_cards_total',       'Red Cards',                'team', 'discipline', true,  false, true,   42),
    ('FOOTBALL', 'fouls_drawn',           'Fouls Drawn',              'team', 'discipline', false, false, true,   43),
    ('FOOTBALL', 'tackles',               'Tackles',                  'team', 'defensive',  false, false, true,   50),
    ('FOOTBALL', 'tackles_won',           'Tackles Won',              'team', 'defensive',  false, false, true,   51),
    ('FOOTBALL', 'tackles_won_percentage','Tackle Success %',         'team', 'defensive',  false, true,  true,   52),
    ('FOOTBALL', 'interceptions',         'Interceptions',            'team', 'defensive',  false, false, true,   53),
    ('FOOTBALL', 'clearances',            'Clearances',               'team', 'defensive',  false, false, true,   54),
    ('FOOTBALL', 'blocked_shots',         'Blocked Shots (Defensive)','team', 'defensive',  false, false, true,   55),
    ('FOOTBALL', 'ball_recovery',         'Ball Recoveries',          'team', 'defensive',  false, false, true,   56),
    ('FOOTBALL', 'dispossessed',          'Dispossessed',             'team', 'possession', true,  false, true,   57),
    ('FOOTBALL', 'possession_lost',       'Possession Lost',          'team', 'possession', true,  false, true,   58),
    ('FOOTBALL', 'dribbled_past',         'Dribbled Past',            'team', 'defensive',  true,  false, true,   59),
    ('FOOTBALL', 'passes',                'Total Passes',             'team', 'passing',    false, false, true,   60),
    ('FOOTBALL', 'accurate_passes',       'Accurate Passes',          'team', 'passing',    false, false, true,   61),
    ('FOOTBALL', 'pass_accuracy',         'Pass Accuracy %',          'team', 'passing',    false, true,  true,   62),
    ('FOOTBALL', 'key_passes',            'Key Passes',               'team', 'passing',    false, false, true,   63),
    ('FOOTBALL', 'backward_passes',       'Backward Passes',          'team', 'passing',    false, false, false,  64),
    ('FOOTBALL', 'passes_final_third',    'Passes in Final Third',    'team', 'passing',    false, false, true,   65),
    ('FOOTBALL', 'long_balls',            'Long Balls',               'team', 'passing',    false, false, false,  66),
    ('FOOTBALL', 'long_balls_won',        'Long Balls Won',           'team', 'passing',    false, false, true,   67),
    ('FOOTBALL', 'long_ball_accuracy',    'Long Ball Accuracy %',     'team', 'passing',    false, true,  true,   68),
    ('FOOTBALL', 'through_balls',         'Through Balls',            'team', 'passing',    false, false, true,   69),
    ('FOOTBALL', 'total_crosses',         'Total Crosses',            'team', 'passing',    false, false, false,  70),
    ('FOOTBALL', 'accurate_crosses',      'Accurate Crosses',         'team', 'passing',    false, false, true,   71),
    ('FOOTBALL', 'cross_accuracy',        'Cross Accuracy %',         'team', 'passing',    false, true,  true,   72),
    ('FOOTBALL', 'shots_total',           'Total Shots',              'team', 'shooting',   false, false, true,   80),
    ('FOOTBALL', 'shots_on_target',       'Shots on Target',          'team', 'shooting',   false, false, true,   81),
    ('FOOTBALL', 'shots_off_target',      'Shots off Target',         'team', 'shooting',   false, false, false,  82),
    ('FOOTBALL', 'shot_accuracy',         'Shot Accuracy %',          'team', 'shooting',   false, true,  true,   83),
    ('FOOTBALL', 'shots_blocked_by_opp',  'Shots Blocked by Opponent','team', 'shooting',   true,  false, false,  84),
    ('FOOTBALL', 'chances_created',       'Chances Created',          'team', 'attacking',  false, false, true,   85),
    ('FOOTBALL', 'big_chances_created',   'Big Chances Created',      'team', 'attacking',  false, false, true,   86),
    ('FOOTBALL', 'big_chances_missed',    'Big Chances Missed',       'team', 'attacking',  true,  false, true,   87),
    ('FOOTBALL', 'dribble_attempts',      'Dribble Attempts',         'team', 'attacking',  false, false, false,  88),
    ('FOOTBALL', 'successful_dribbles',   'Successful Dribbles',      'team', 'attacking',  false, false, true,   89),
    ('FOOTBALL', 'dribble_success_rate',  'Dribble Success %',        'team', 'attacking',  false, true,  true,   90),
    ('FOOTBALL', 'total_duels',           'Total Duels',              'team', 'duels',      false, false, false, 100),
    ('FOOTBALL', 'duels_won',             'Duels Won',                'team', 'duels',      false, false, true,  101),
    ('FOOTBALL', 'duels_lost',            'Duels Lost',               'team', 'duels',      true,  false, false, 102),
    ('FOOTBALL', 'duels_won_percentage',  'Duel Success %',           'team', 'duels',      false, true,  true,  103),
    ('FOOTBALL', 'aerials_total',         'Aerials',                  'team', 'duels',      false, false, false, 104),
    ('FOOTBALL', 'aerials_won',           'Aerials Won',              'team', 'duels',      false, false, true,  105),
    ('FOOTBALL', 'aerials_lost',          'Aerials Lost',             'team', 'duels',      true,  false, false, 106),
    ('FOOTBALL', 'aerials_won_percentage','Aerial Success %',         'team', 'duels',      false, true,  true,  107),
    ('FOOTBALL', 'touches',               'Touches',                  'team', 'possession', false, false, false, 110),
    ('FOOTBALL', 'turnovers',             'Turnovers',                'team', 'possession', true,  false, true,  111),
    ('FOOTBALL', 'offsides',              'Offsides',                 'team', 'attacking',  true,  false, false, 112),
    ('FOOTBALL', 'offsides_provoked',     'Offsides Won',             'team', 'defensive',  false, false, false, 113),
    ('FOOTBALL', 'saves',                 'Saves',                    'team', 'goalkeeper', false, false, true,  120),
    ('FOOTBALL', 'saves_insidebox',       'Saves Inside Box',         'team', 'goalkeeper', false, false, true,  121),
    ('FOOTBALL', 'good_high_claim',       'High Claims',              'team', 'goalkeeper', false, false, false, 122)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2. DERIVED STATS TRIGGERS
-- ============================================================================

-- Football player: per-90 metrics, accuracy rates, GK stats
CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC; goals NUMERIC; assists NUMERIC; key_passes NUMERIC;
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    tackles NUMERIC; tackles_w NUMERIC; intercepts NUMERIC;
    duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC; saves NUMERIC; conceded NUMERIC;
    crosses_t NUMERIC; crosses_a NUMERIC;
    long_balls_t NUMERIC; long_balls_w NUMERIC;
    aerials_t NUMERIC; aerials_w NUMERIC;
BEGIN
    minutes      := (NEW.stats->>'minutes_played')::NUMERIC;
    goals        := (NEW.stats->>'goals')::NUMERIC;
    assists      := (NEW.stats->>'assists')::NUMERIC;
    key_passes   := (NEW.stats->>'key_passes')::NUMERIC;
    shots_t      := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on     := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t     := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a     := (NEW.stats->>'passes_accurate')::NUMERIC;
    tackles      := (NEW.stats->>'tackles')::NUMERIC;
    tackles_w    := (NEW.stats->>'tackles_won')::NUMERIC;
    intercepts   := (NEW.stats->>'interceptions')::NUMERIC;
    duels_t      := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w      := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a   := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s   := (NEW.stats->>'dribbles_success')::NUMERIC;
    saves        := (NEW.stats->>'saves')::NUMERIC;
    conceded     := (NEW.stats->>'goals_conceded')::NUMERIC;
    crosses_t    := (NEW.stats->>'crosses_total')::NUMERIC;
    crosses_a    := (NEW.stats->>'crosses_accurate')::NUMERIC;
    long_balls_t := (NEW.stats->>'long_balls')::NUMERIC;
    long_balls_w := (NEW.stats->>'long_balls_won')::NUMERIC;
    aerials_t    := (NEW.stats->>'aerials')::NUMERIC;
    aerials_w    := (NEW.stats->>'aeriels_won')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF goals IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_per_90', ROUND(goals * 90 / minutes, 3)); END IF;
        IF assists IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('assists_per_90', ROUND(assists * 90 / minutes, 3)); END IF;
        IF key_passes IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('key_passes_per_90', ROUND(key_passes * 90 / minutes, 3)); END IF;
        IF shots_t IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(shots_t * 90 / minutes, 3)); END IF;
        IF tackles IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('tackles_per_90', ROUND(tackles * 90 / minutes, 3)); END IF;
        IF intercepts IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('interceptions_per_90', ROUND(intercepts * 90 / minutes, 3)); END IF;
        IF conceded IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_conceded_per_90', ROUND(conceded * 90 / minutes, 3)); END IF;
    END IF;

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

-- Triggers on shared tables
DROP TRIGGER IF EXISTS trg_football_derived_stats ON player_stats;
CREATE TRIGGER trg_football_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW WHEN (NEW.sport = 'FOOTBALL')
    EXECUTE FUNCTION football.compute_derived_player_stats();

-- ============================================================================
-- 3. VIEWS (PostgREST surface)
-- ============================================================================

-- Drop legacy views from pre-consolidation (players, player_stats, teams, team_stats)
DROP VIEW IF EXISTS football.players;
DROP VIEW IF EXISTS football.player_stats;
DROP VIEW IF EXISTS football.teams;
DROP VIEW IF EXISTS football.team_stats;

-- Combined player profile + stats (with team and league context)
CREATE OR REPLACE VIEW football.player AS
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
        WHEN ps.percentiles IS NOT NULL
            AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.leagues l ON l.id = p.league_id
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'FOOTBALL';

COMMENT ON VIEW football.player IS
    'Football player profile with stats. Filter by id, season, league_id. Stats columns are NULL when no stats exist.';

-- Combined team profile + stats (with league context)
CREATE OR REPLACE VIEW football.team AS
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
        WHEN ts.percentiles IS NOT NULL
            AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object(
            'sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.leagues l ON l.id = t.league_id
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'FOOTBALL';

COMMENT ON VIEW football.team IS
    'Football team profile with stats. Filter by id, season, league_id. Stats columns are NULL when no stats exist.';

-- Standings (hardcoded Football sort: points, goal difference, goals for)
CREATE OR REPLACE VIEW football.standings AS
SELECT
    ts.team_id, ts.season, ts.league_id,
    t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
    l.name AS league_name, ts.stats,
    (ts.stats->>'points')::integer AS sort_points,
    (ts.stats->>'goal_difference')::integer AS sort_goal_diff
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
LEFT JOIN public.leagues l ON l.id = ts.league_id
WHERE ts.sport = 'FOOTBALL';

COMMENT ON VIEW football.standings IS
    'Football standings. Order by sort_points DESC, sort_goal_diff DESC. Filter by season, league_id.';

CREATE OR REPLACE VIEW football.stat_definitions AS
SELECT id, key_name, display_name, entity_type, category,
       is_inverse, is_derived, is_percentile_eligible, sort_order
FROM public.stat_definitions
WHERE sport = 'FOOTBALL';

COMMENT ON VIEW football.stat_definitions IS
    'Football stat registry. Filter by entity_type.';

-- Leagues (Football-specific)
CREATE OR REPLACE VIEW football.leagues AS
SELECT id, name, country, logo_url, is_benchmark, is_active, handicap, meta
FROM public.leagues
WHERE sport = 'FOOTBALL';

COMMENT ON VIEW football.leagues IS
    'Football league metadata. Filter by is_active, is_benchmark.';

-- ============================================================================
-- 4. MATERIALIZED VIEW — autofill/search
-- ============================================================================

DROP MATERIALIZED VIEW IF EXISTS football.autofill_entities;
CREATE MATERIALIZED VIEW football.autofill_entities AS
    -- Players (resolve league from latest player_stats)
    SELECT * FROM (
        SELECT DISTINCT ON (p.id)
            p.id,
            'player'::text AS type,
            p.name,
            p.first_name,
            p.last_name,
            p.position,
            p.detailed_position,
            p.nationality,
            p.date_of_birth::text AS date_of_birth,
            p.height,
            p.weight,
            p.photo_url,
            p.team_id,
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            jsonb_build_array(
                LOWER(p.first_name),
                LOWER(p.last_name),
                LOWER(REPLACE(p.name, ' ', '')),
                LOWER(COALESCE(t.short_code, '')),
                LOWER(COALESCE(t.name, '')),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(p.first_name)),
                unaccent(LOWER(p.last_name)),
                unaccent(LOWER(REPLACE(p.name, ' ', ''))),
                unaccent(LOWER(COALESCE(t.name, '')))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', p.name,
                'jersey_number', p.meta->>'jersey_number',
                'foot', p.meta->>'foot',
                'market_value', (p.meta->>'market_value')::bigint,
                'contract_until', p.meta->>'contract_until'
            ) AS meta
        FROM public.players p
        LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
        LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
        LEFT JOIN public.leagues l ON l.id = ps.league_id
        WHERE p.sport = 'FOOTBALL'
        ORDER BY p.id, ps.season DESC NULLS LAST
    ) football_players
UNION ALL
    -- Teams (resolve league from latest team_stats)
    SELECT * FROM (
        SELECT DISTINCT ON (t.id)
            t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            NULL::text AS position,
            NULL::text AS detailed_position,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::int AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            jsonb_build_array(
                LOWER(REPLACE(t.name, ' ', '')),
                LOWER(t.short_code),
                LOWER(t.city),
                LOWER(t.country),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(REPLACE(t.name, ' ', ''))),
                unaccent(LOWER(t.city))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', t.name,
                'abbreviation', t.short_code,
                'city', t.city,
                'country', t.country,
                'founded', t.founded,
                'venue_name', t.venue_name,
                'venue_capacity', t.venue_capacity
            ) AS meta
        FROM public.teams t
        LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
        LEFT JOIN public.leagues l ON l.id = ts.league_id
        WHERE t.sport = 'FOOTBALL'
        ORDER BY t.id, ts.season DESC NULLS LAST
    ) football_teams
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_football_autofill_pk
    ON football.autofill_entities (id, type);

-- ============================================================================
-- 5. RPC FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION football.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, name TEXT, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        p.id AS player_id, p.name, p.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    JOIN public.players p ON s.player_id = p.id AND s.sport = p.sport
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'FOOTBALL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION football.stat_leaders IS
    'Returns top N Football players by stat category with positional filtering.';

CREATE OR REPLACE FUNCTION football.health()
RETURNS json AS $$
    SELECT json_build_object('status', 'ok');
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 6. EVENT -> SEASON AGGREGATION FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION football.aggregate_player_season(
    p_player_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH raw AS (
    SELECT stats, minutes_played
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'FOOTBALL'
      AND season = p_season
      AND league_id = p_league_id
      AND COALESCE(minutes_played, 0) > 0
 ),
agg AS (
    SELECT
        COUNT(*)::numeric AS matches_played,
        SUM(COALESCE(minutes_played, 0)) AS minutes_sum,
        -- Scoring (from match events)
        SUM(COALESCE((stats->>'goals')::numeric, 0)) AS goals,
        SUM(COALESCE((stats->>'assists')::numeric, 0)) AS assists,
        SUM(COALESCE((stats->>'penalty_goals')::numeric, 0)) AS penalty_goals,
        SUM(COALESCE((stats->>'penalties_missed')::numeric, 0)) AS penalties_missed,
        SUM(COALESCE((stats->>'penalties_won')::numeric, 0)) AS penalties_won,
        SUM(COALESCE((stats->>'expected_goals')::numeric, 0)) AS expected_goals,
        SUM(COALESCE((stats->>'own_goals')::numeric, 0)) AS own_goals,
        -- Shooting
        SUM(COALESCE((stats->>'shots_total')::numeric, 0)) AS shots_total,
        SUM(COALESCE((stats->>'shots_on_target')::numeric, 0)) AS shots_on_target,
        SUM(COALESCE((stats->>'shots_off_target')::numeric, 0)) AS shots_off_target,
        SUM(COALESCE((stats->>'shots_blocked')::numeric, 0)) AS shots_blocked,
        SUM(COALESCE((stats->>'hit_woodwork')::numeric, 0)) AS hit_woodwork,
        SUM(COALESCE((stats->>'big_chances_missed')::numeric, 0)) AS big_chances_missed,
        -- Passing
        SUM(COALESCE((stats->>'passes_total')::numeric, 0)) AS passes_total,
        SUM(COALESCE((stats->>'passes_accurate')::numeric, 0)) AS passes_accurate,
        SUM(COALESCE((stats->>'key_passes')::numeric, 0)) AS key_passes,
        SUM(COALESCE((stats->>'big_chances_created')::numeric, 0)) AS big_chances_created,
        SUM(COALESCE((stats->>'chances_created')::numeric, 0)) AS chances_created,
        SUM(COALESCE((stats->>'crosses_total')::numeric, 0)) AS crosses_total,
        SUM(COALESCE((stats->>'crosses_accurate')::numeric, 0)) AS crosses_accurate,
        SUM(COALESCE((stats->>'long_balls')::numeric, 0)) AS long_balls,
        SUM(COALESCE((stats->>'long_balls_won')::numeric, 0)) AS long_balls_won,
        SUM(COALESCE((stats->>'through_balls')::numeric, 0)) AS through_balls,
        SUM(COALESCE((stats->>'backward_passes')::numeric, 0)) AS backward_passes,
        SUM(COALESCE((stats->>'passes_in_final_third')::numeric, 0)) AS passes_in_final_third,
        -- Defensive
        SUM(COALESCE((stats->>'tackles')::numeric, 0)) AS tackles,
        SUM(COALESCE((stats->>'tackles_won')::numeric, 0)) AS tackles_won,
        SUM(COALESCE((stats->>'interceptions')::numeric, 0)) AS interceptions,
        SUM(COALESCE((stats->>'clearances')::numeric, 0)) AS clearances,
        SUM(COALESCE((stats->>'blocks')::numeric, 0)) AS blocks,
        -- Duels & dribbling
        SUM(COALESCE((stats->>'duels_total')::numeric, 0)) AS duels_total,
        SUM(COALESCE((stats->>'duels_won')::numeric, 0)) AS duels_won,
        SUM(COALESCE((stats->>'duels_lost')::numeric, 0)) AS duels_lost,
        SUM(COALESCE((stats->>'aerials')::numeric, 0)) AS aerials,
        SUM(COALESCE((stats->>'aeriels_won')::numeric, 0)) AS aeriels_won,
        SUM(COALESCE((stats->>'aeriels_lost')::numeric, 0)) AS aeriels_lost,
        SUM(COALESCE((stats->>'dribbles_attempts')::numeric, 0)) AS dribbles_attempts,
        SUM(COALESCE((stats->>'dribbles_success')::numeric, 0)) AS dribbles_success,
        SUM(COALESCE((stats->>'dribbled_past')::numeric, 0)) AS dribbled_past,
        SUM(COALESCE((stats->>'dispossessed')::numeric, 0)) AS dispossessed,
        SUM(COALESCE((stats->>'possession_lost')::numeric, 0)) AS possession_lost,
        SUM(COALESCE((stats->>'turn_over')::numeric, 0)) AS turnovers,
        -- Through balls, errors, penalties, extras
        SUM(COALESCE((stats->>'through_balls_won')::numeric, 0)) AS through_balls_won,
        SUM(COALESCE((stats->>'error_lead_to_shot')::numeric, 0)) AS error_lead_to_shot,
        SUM(COALESCE((stats->>'error_lead_to_goal')::numeric, 0)) AS error_lead_to_goal,
        SUM(COALESCE((stats->>'last_man_tackle')::numeric, 0)) AS last_man_tackle,
        SUM(COALESCE((stats->>'clearance_offline')::numeric, 0)) AS clearance_offline,
        SUM(COALESCE((stats->>'penalties_committed')::numeric, 0)) AS penalties_committed,
        SUM(COALESCE((stats->>'penalties_scored')::numeric, 0)) AS penalties_scored,
        SUM(COALESCE((stats->>'offsides_provoked')::numeric, 0)) AS offsides_provoked,
        SUM(COALESCE((stats->>'yellowred_cards')::numeric, 0)) AS yellowred_cards,
        SUM(COALESCE((stats->>'man_of_match')::numeric, 0)) AS motm_awards,
        AVG(NULLIF((stats->>'rating')::numeric, 0)) AS rating_avg,
        -- General
        SUM(COALESCE((stats->>'touches')::numeric, 0)) AS touches,
        SUM(COALESCE((stats->>'ball_recovery')::numeric, 0)) AS ball_recovery,
        -- Discipline
        SUM(COALESCE((stats->>'yellow_cards')::numeric, 0)) AS yellow_cards,
        SUM(COALESCE((stats->>'red_cards')::numeric, 0)) AS red_cards,
        SUM(COALESCE((stats->>'fouls_committed')::numeric, 0)) AS fouls_committed,
        SUM(COALESCE((stats->>'fouls_drawn')::numeric, 0)) AS fouls_drawn,
        SUM(COALESCE((stats->>'offsides')::numeric, 0)) AS offsides,
        -- Goalkeeper
        SUM(COALESCE((stats->>'saves')::numeric, 0)) AS saves,
        SUM(COALESCE((stats->>'saves_insidebox')::numeric, 0)) AS saves_insidebox,
        SUM(COALESCE((stats->>'goals_conceded')::numeric, 0)) AS goals_conceded,
        SUM(COALESCE((stats->>'punches')::numeric, 0)) AS punches,
        SUM(COALESCE((stats->>'good_high_claim')::numeric, 0)) AS good_high_claim,
        SUM(COALESCE((stats->>'penalties_saved')::numeric, 0)) AS penalties_saved
    FROM raw
)
SELECT CASE
    WHEN matches_played = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'appearances', matches_played::int,
            'lineups', matches_played::int,
            'minutes_played', ROUND(minutes_sum, 1),
            'goals', goals::int,
            'assists', assists::int,
            'penalty_goals', CASE WHEN penalty_goals > 0 THEN penalty_goals::int END,
            'penalties_missed', CASE WHEN penalties_missed > 0 THEN penalties_missed::int END,
            'penalties_won', CASE WHEN penalties_won > 0 THEN penalties_won::int END,
            'expected_goals', ROUND(expected_goals, 2),
            'own_goals', CASE WHEN own_goals > 0 THEN own_goals::int END,
            'shots_total', shots_total::int,
            'shots_on_target', shots_on_target::int,
            'shots_off_target', shots_off_target::int,
            'shots_blocked', shots_blocked::int,
            'hit_woodwork', CASE WHEN hit_woodwork > 0 THEN hit_woodwork::int END,
            'big_chances_missed', CASE WHEN big_chances_missed > 0 THEN big_chances_missed::int END,
            'passes_total', passes_total::int,
            'passes_accurate', passes_accurate::int,
            'key_passes', key_passes::int,
            'big_chances_created', CASE WHEN big_chances_created > 0 THEN big_chances_created::int END,
            'chances_created', chances_created::int,
            'crosses_total', crosses_total::int,
            'crosses_accurate', crosses_accurate::int
        ) || jsonb_build_object(
            'long_balls', long_balls::int,
            'long_balls_won', long_balls_won::int,
            'through_balls', CASE WHEN through_balls > 0 THEN through_balls::int END,
            'backward_passes', backward_passes::int,
            'passes_in_final_third', passes_in_final_third::int,
            'tackles', tackles::int,
            'tackles_won', tackles_won::int,
            'interceptions', interceptions::int,
            'clearances', clearances::int,
            'blocks', blocks::int,
            'duels_total', duels_total::int,
            'duels_won', duels_won::int,
            'duels_lost', duels_lost::int,
            'aerials', aerials::int,
            'aeriels_won', aeriels_won::int,
            'aeriels_lost', aeriels_lost::int,
            'dribbles_attempts', dribbles_attempts::int,
            'dribbles_success', dribbles_success::int,
            'dribbled_past', dribbled_past::int,
            'dispossessed', dispossessed::int,
            'possession_lost', possession_lost::int,
            'turnovers', turnovers::int,
            'touches', touches::int,
            'ball_recovery', ball_recovery::int,
            'yellow_cards', yellow_cards::int,
            'red_cards', CASE WHEN red_cards > 0 THEN red_cards::int END,
            'yellowred_cards', CASE WHEN yellowred_cards > 0 THEN yellowred_cards::int END,
            'fouls_committed', fouls_committed::int,
            'fouls_drawn', fouls_drawn::int,
            'penalties_committed', CASE WHEN penalties_committed > 0 THEN penalties_committed::int END,
            'penalties_scored', CASE WHEN penalties_scored > 0 THEN penalties_scored::int END,
            'through_balls_won', CASE WHEN through_balls_won > 0 THEN through_balls_won::int END,
            'error_lead_to_shot', CASE WHEN error_lead_to_shot > 0 THEN error_lead_to_shot::int END,
            'error_lead_to_goal', CASE WHEN error_lead_to_goal > 0 THEN error_lead_to_goal::int END,
            'last_man_tackle', CASE WHEN last_man_tackle > 0 THEN last_man_tackle::int END,
            'clearance_offline', CASE WHEN clearance_offline > 0 THEN clearance_offline::int END,
            'offsides', offsides::int,
            'offsides_provoked', CASE WHEN offsides_provoked > 0 THEN offsides_provoked::int END,
            'motm_awards', CASE WHEN motm_awards > 0 THEN motm_awards::int END,
            'rating_avg', CASE WHEN rating_avg IS NOT NULL THEN ROUND(rating_avg, 2) END,
            'saves', CASE WHEN saves > 0 THEN saves::int END,
            'saves_insidebox', CASE WHEN saves_insidebox > 0 THEN saves_insidebox::int END,
            'goals_conceded', goals_conceded::int,
            'punches', CASE WHEN punches > 0 THEN punches::int END,
            'good_high_claim', CASE WHEN good_high_claim > 0 THEN good_high_claim::int END,
            'penalties_saved', CASE WHEN penalties_saved > 0 THEN penalties_saved::int END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION football.aggregate_team_season(
    p_team_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS matches_played,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score = opp.score THEN 1 ELSE 0 END)::numeric AS draws,
        SUM(COALESCE(ets.score, 0))::numeric AS gf_sum,
        SUM(COALESCE(opp.score, 0))::numeric AS ga_sum,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_won,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score = opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_draw,
        SUM(
            CASE
                WHEN f.home_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS home_lost,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_won,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score = opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_draw,
        SUM(
            CASE
                WHEN f.away_team_id = ets.team_id THEN CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END
                ELSE 0
            END
        )::numeric AS away_lost,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN COALESCE(ets.score, 0) ELSE 0 END)::numeric AS home_scored,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN COALESCE(opp.score, 0) ELSE 0 END)::numeric AS home_conceded,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN COALESCE(ets.score, 0) ELSE 0 END)::numeric AS away_scored,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN COALESCE(opp.score, 0) ELSE 0 END)::numeric AS away_conceded,
        SUM(CASE WHEN f.home_team_id = ets.team_id THEN 1 ELSE 0 END)::numeric AS home_played,
        SUM(CASE WHEN f.away_team_id = ets.team_id THEN 1 ELSE 0 END)::numeric AS away_played,
        SUM(COALESCE((ets.stats->>'fouls')::numeric, 0))                  AS fouls_committed,
        SUM(COALESCE((ets.stats->>'yellow_cards')::numeric, 0))           AS yellow_cards_total,
        SUM(COALESCE((ets.stats->>'red_cards')::numeric, 0))              AS red_cards_total,
        SUM(COALESCE((ets.stats->>'fouls_drawn')::numeric, 0))            AS fouls_drawn,
        SUM(COALESCE((ets.stats->>'tackles')::numeric, 0))                AS tackles,
        SUM(COALESCE((ets.stats->>'tackles_won')::numeric, 0))            AS tackles_won,
        SUM(COALESCE((ets.stats->>'interceptions')::numeric, 0))          AS interceptions,
        SUM(COALESCE((ets.stats->>'clearances')::numeric, 0))             AS clearances,
        SUM(COALESCE((ets.stats->>'blocked_shots')::numeric, 0))          AS blocked_shots,
        SUM(COALESCE((ets.stats->>'ball_recovery')::numeric, 0))          AS ball_recovery,
        SUM(COALESCE((ets.stats->>'dispossessed')::numeric, 0))           AS dispossessed,
        SUM(COALESCE((ets.stats->>'possession_lost')::numeric, 0))        AS possession_lost,
        SUM(COALESCE((ets.stats->>'dribbled_past')::numeric, 0))          AS dribbled_past,
        SUM(COALESCE((ets.stats->>'passes')::numeric, 0))                 AS passes,
        SUM(COALESCE((ets.stats->>'accurate_passes')::numeric, 0))        AS accurate_passes,
        SUM(COALESCE((ets.stats->>'key_passes')::numeric, 0))             AS key_passes,
        SUM(COALESCE((ets.stats->>'backward_passes')::numeric, 0))        AS backward_passes,
        SUM(COALESCE((ets.stats->>'passes_in_final_third')::numeric, 0))  AS passes_final_third,
        SUM(COALESCE((ets.stats->>'long_balls')::numeric, 0))             AS long_balls,
        SUM(COALESCE((ets.stats->>'long_balls_won')::numeric, 0))         AS long_balls_won,
        SUM(COALESCE((ets.stats->>'through_balls')::numeric, 0))          AS through_balls,
        SUM(COALESCE((ets.stats->>'total_crosses')::numeric, 0))          AS total_crosses,
        SUM(COALESCE((ets.stats->>'accurate_crosses')::numeric, 0))       AS accurate_crosses,
        SUM(COALESCE((ets.stats->>'shots_total')::numeric, 0))            AS shots_total,
        SUM(COALESCE((ets.stats->>'shots_on_target')::numeric, 0))        AS shots_on_target,
        SUM(COALESCE((ets.stats->>'shots_off_target')::numeric, 0))       AS shots_off_target,
        SUM(COALESCE((ets.stats->>'shots_blocked')::numeric, 0))          AS shots_blocked_by_opp,
        SUM(COALESCE((ets.stats->>'chances_created')::numeric, 0))        AS chances_created,
        SUM(COALESCE((ets.stats->>'big_chances_created')::numeric, 0))    AS big_chances_created,
        SUM(COALESCE((ets.stats->>'big_chances_missed')::numeric, 0))     AS big_chances_missed,
        SUM(COALESCE((ets.stats->>'dribble_attempts')::numeric, 0))       AS dribble_attempts,
        SUM(COALESCE((ets.stats->>'successful_dribbles')::numeric, 0))    AS successful_dribbles,
        SUM(COALESCE((ets.stats->>'total_duels')::numeric, 0))            AS total_duels,
        SUM(COALESCE((ets.stats->>'duels_won')::numeric, 0))              AS duels_won,
        SUM(COALESCE((ets.stats->>'duels_lost')::numeric, 0))             AS duels_lost,
        SUM(COALESCE((ets.stats->>'aerials')::numeric, 0))                AS aerials_total,
        SUM(COALESCE((ets.stats->>'aeriels_won')::numeric, 0))            AS aerials_won,
        SUM(COALESCE((ets.stats->>'aeriels_lost')::numeric, 0))           AS aerials_lost,
        SUM(COALESCE((ets.stats->>'touches')::numeric, 0))                AS touches,
        SUM(COALESCE((ets.stats->>'turn_over')::numeric, 0))              AS turnovers,
        SUM(COALESCE((ets.stats->>'offsides')::numeric, 0))               AS offsides,
        SUM(COALESCE((ets.stats->>'offsides_provoked')::numeric, 0))      AS offsides_provoked,
        SUM(COALESCE((ets.stats->>'saves')::numeric, 0))                  AS saves,
        SUM(COALESCE((ets.stats->>'saves_insidebox')::numeric, 0))        AS saves_insidebox,
        SUM(COALESCE((ets.stats->>'good_high_claim')::numeric, 0))        AS good_high_claim
    FROM public.event_team_stats ets
    JOIN public.fixtures f ON f.id = ets.fixture_id
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'FOOTBALL'
      AND ets.season = p_season
      AND ets.league_id = p_league_id
)
SELECT CASE
    WHEN matches_played = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'matches_played', matches_played::int,
            'wins', wins::int,
            'draws', draws::int,
            'losses', losses::int,
            'goals_for', gf_sum::int,
            'goals_against', ga_sum::int,
            'goal_difference', (gf_sum - ga_sum)::int,
            'points', (wins * 3 + draws)::int,
            'overall_points', (wins * 3 + draws)::int,
            'home_played', home_played::int,
            'home_won', home_won::int,
            'home_draw', home_draw::int,
            'home_lost', home_lost::int,
            'home_scored', home_scored::int,
            'home_conceded', home_conceded::int,
            'home_points', (home_won * 3 + home_draw)::int,
            'away_played', away_played::int,
            'away_won', away_won::int,
            'away_draw', away_draw::int,
            'away_lost', away_lost::int,
            'away_scored', away_scored::int,
            'away_conceded', away_conceded::int,
            'away_points', (away_won * 3 + away_draw)::int,
            'fouls_committed', fouls_committed::int,
            'yellow_cards_total', yellow_cards_total::int,
            'red_cards_total', red_cards_total::int,
            'fouls_drawn', fouls_drawn::int,
            'tackles', tackles::int,
            'tackles_won', tackles_won::int,
            'tackles_won_percentage', CASE WHEN tackles > 0 THEN ROUND(tackles_won / tackles * 100, 2) END,
            'interceptions', interceptions::int,
            'clearances', clearances::int,
            'blocked_shots', blocked_shots::int,
            'ball_recovery', ball_recovery::int,
            'dispossessed', dispossessed::int,
            'possession_lost', possession_lost::int,
            'dribbled_past', dribbled_past::int,
            'passes', passes::int,
            'accurate_passes', accurate_passes::int,
            'pass_accuracy', CASE WHEN passes > 0 THEN ROUND(accurate_passes / passes * 100, 2) END,
            'key_passes', key_passes::int,
            'backward_passes', backward_passes::int,
            'passes_final_third', passes_final_third::int,
            'long_balls', long_balls::int,
            'long_balls_won', long_balls_won::int,
            'long_ball_accuracy', CASE WHEN long_balls > 0 THEN ROUND(long_balls_won / long_balls * 100, 2) END,
            'through_balls', through_balls::int
        ) || jsonb_build_object(
            'total_crosses', total_crosses::int,
            'accurate_crosses', accurate_crosses::int,
            'cross_accuracy', CASE WHEN total_crosses > 0 THEN ROUND(accurate_crosses / total_crosses * 100, 2) END,
            'shots_total', shots_total::int,
            'shots_on_target', shots_on_target::int,
            'shots_off_target', shots_off_target::int,
            'shot_accuracy', CASE WHEN shots_total > 0 THEN ROUND(shots_on_target / shots_total * 100, 2) END,
            'shots_blocked_by_opp', shots_blocked_by_opp::int,
            'chances_created', chances_created::int,
            'big_chances_created', big_chances_created::int,
            'big_chances_missed', big_chances_missed::int,
            'dribble_attempts', dribble_attempts::int,
            'successful_dribbles', successful_dribbles::int,
            'dribble_success_rate', CASE WHEN dribble_attempts > 0 THEN ROUND(successful_dribbles / dribble_attempts * 100, 2) END,
            'total_duels', total_duels::int,
            'duels_won', duels_won::int,
            'duels_lost', duels_lost::int,
            'duels_won_percentage', CASE WHEN total_duels > 0 THEN ROUND(duels_won / total_duels * 100, 2) END,
            'aerials_total', aerials_total::int,
            'aerials_won', aerials_won::int,
            'aerials_lost', aerials_lost::int,
            'aerials_won_percentage', CASE WHEN aerials_total > 0 THEN ROUND(aerials_won / aerials_total * 100, 2) END,
            'touches', touches::int,
            'turnovers', turnovers::int,
            'offsides', offsides::int,
            'offsides_provoked', offsides_provoked::int,
            'saves', saves::int,
            'saves_insidebox', saves_insidebox::int,
            'good_high_claim', good_high_claim::int
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 7. GRANTS
-- ============================================================================

GRANT USAGE ON SCHEMA football TO web_anon, web_user;
GRANT SELECT ON ALL TABLES IN SCHEMA football TO web_anon, web_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA football TO web_anon, web_user;
