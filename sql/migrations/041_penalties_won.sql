-- ============================================================================
-- 041_penalties_won.sql
-- FOOTBALL "Penalties Won" (drawing a penalty) enters the rating:
--   PLAYER: Specialist-only (in_spec, NOT in_comp). penalties_won is sparse
--           (~9% of player-seasons nonzero) → by gate-2 a sparse spike belongs in
--           the peak, not the breadth sum. Player composite stays byte-identical;
--           it just becomes a leadable specialty (Ouattara, Vini, Mbappé…).
--   TEAM:   offense composite (+z). Team-grain is denser (avg 4.3/sd 2.2) and
--           distinct (corr ≤0.36 vs goals/SoT/key passes) — gate-checked.
--
-- Player penalties_won was already aggregated (aggregate_player_season); only the
-- team aggregate gains it. rating_datapoints (+player) / rating_datapoints_team
-- (+team) copied from 037/040 with the new datapoint. Football ratings recomputed.
-- ============================================================================

BEGIN;

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
        SUM(COALESCE((ets.stats->>'penalties_committed')::numeric, 0))    AS penalties_committed,
        SUM(COALESCE((ets.stats->>'penalties_won')::numeric, 0))          AS penalties_won,
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
        SUM(COALESCE((ets.stats->>'good_high_claim')::numeric, 0))        AS good_high_claim,
        -- Fixture-level team statistics (SportMonks `statistics` include)
        AVG(NULLIF((ets.stats->>'possession_pct')::numeric, 0))           AS possession_pct,
        SUM(COALESCE((ets.stats->>'assists')::numeric, 0))                AS team_assists,
        SUM(COALESCE((ets.stats->>'goal_attempts')::numeric, 0))          AS goal_attempts,
        SUM(COALESCE((ets.stats->>'hit_woodwork')::numeric, 0))           AS hit_woodwork,
        SUM(COALESCE((ets.stats->>'shots_insidebox')::numeric, 0))        AS shots_insidebox,
        SUM(COALESCE((ets.stats->>'shots_outsidebox')::numeric, 0))       AS shots_outsidebox,
        SUM(COALESCE((ets.stats->>'successful_headers')::numeric, 0))     AS successful_headers,
        SUM(COALESCE((ets.stats->>'corners')::numeric, 0))                AS corners,
        SUM(COALESCE((ets.stats->>'attacks')::numeric, 0))                AS attacks,
        SUM(COALESCE((ets.stats->>'dangerous_attacks')::numeric, 0))      AS dangerous_attacks,
        SUM(COALESCE((ets.stats->>'ball_safe')::numeric, 0))              AS ball_safe,
        SUM(COALESCE((ets.stats->>'goal_kicks')::numeric, 0))             AS goal_kicks,
        SUM(COALESCE((ets.stats->>'free_kicks')::numeric, 0))             AS free_kicks,
        SUM(COALESCE((ets.stats->>'throw_ins')::numeric, 0))              AS throw_ins,
        SUM(COALESCE((ets.stats->>'penalties')::numeric, 0))              AS penalties,
        SUM(COALESCE((ets.stats->>'injuries')::numeric, 0))               AS injuries,
        SUM(COALESCE((ets.stats->>'substitutions')::numeric, 0))          AS substitutions,
        -- Opponent production allowed (other team's box score, same fixture) → defensive suppression.
        SUM(COALESCE((opp.stats->>'shots_on_target')::numeric, 0))        AS opp_sot_sum,
        SUM(COALESCE((opp.stats->>'shots_total')::numeric, 0))            AS opp_shots_sum,
        SUM(COALESCE((opp.stats->>'big_chances_created')::numeric, 0))    AS opp_big_chances_sum,
        AVG(NULLIF((opp.stats->>'possession_pct')::numeric, 0))           AS opp_possession_pct
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
            'penalties_committed', penalties_committed::int,
            'penalties_won', penalties_won::int,
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
        ) || jsonb_build_object(
            'possession_pct', CASE WHEN possession_pct IS NOT NULL THEN ROUND(possession_pct, 2) END,
            'assists', team_assists::int,
            'goal_attempts', goal_attempts::int,
            'hit_woodwork', hit_woodwork::int,
            'shots_insidebox', shots_insidebox::int,
            'shots_outsidebox', shots_outsidebox::int,
            'successful_headers', successful_headers::int,
            'corners', corners::int,
            'attacks', attacks::int,
            'dangerous_attacks', dangerous_attacks::int,
            'ball_safe', ball_safe::int,
            'goal_kicks', goal_kicks::int,
            'free_kicks', free_kicks::int,
            'throw_ins', throw_ins::int,
            'penalties', penalties::int,
            'injuries', injuries::int,
            'substitutions', substitutions::int,
            -- Opponent-allowed (defensive suppression, derived from opponent box scores).
            -- shots_on_target_allowed is the composite −z term (gate-checked distinct, corr ≤0.59
            -- vs the defensive-action terms); shots_allowed / big_chances_allowed / opp possession
            -- are display-only.
            'shots_on_target_allowed', opp_sot_sum::int,
            'shots_allowed', opp_shots_sum::int,
            'big_chances_allowed', opp_big_chances_sum::int,
            'opp_possession_pct', CASE WHEN opp_possession_pct IS NOT NULL THEN ROUND(opp_possession_pct, 2) END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION rating_datapoints(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA (flat-z): pts, reb, ast, stl, blk, fg3m, +plus_minus, -turnover, -pf; +fta (spec-only)
    SELECT * FROM (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all'),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'

    UNION ALL
    -- FOOTBALL (flat-z): top-5 leagues pooled, GK in the same positionless pool.
    SELECT * FROM (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           TRUE, TRUE,   1, 'all'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all'),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL (category-balanced: offense / defense / special facets). Unchanged.
    SELECT * FROM (VALUES
        ('Total Yards',      COALESCE((p_stats->>'passing_yards')::numeric,0)
                           + COALESCE((p_stats->>'rushing_yards')::numeric,0)
                           + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                           + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                           + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0),
                                                                                  TRUE, TRUE,   1, 'offense'),
        ('Touchdowns',       COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0),
                                                                                  TRUE, TRUE,   1, 'offense'),
        ('Receiving',        NULLIF(p_stats->>'receptions','')::numeric,         TRUE, TRUE,   1, 'offense'),
        ('Giveaways',        COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                           + COALESCE((p_stats->>'fumbles_lost')::numeric,0),    TRUE, FALSE, -1, 'offense'),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,      TRUE, TRUE,   1, 'defense'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,   TRUE, TRUE,   1, 'defense'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense'),
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL';
$$;

DROP FUNCTION IF EXISTS rating_datapoints_team(TEXT, JSONB);

CREATE FUNCTION rating_datapoints_team(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA team — offense / defense. +Foul Drawing (fta, composite: team corr 0.37).
    -- Margin (point_differential) DROPPED. oreb/dreb + opp FG% are display-only.
    SELECT * FROM (VALUES
        ('Scoring',            NULLIF(p_stats->>'pts','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Playmaking',         NULLIF(p_stats->>'ast','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('3PT Shooting',       NULLIF(p_stats->>'fg3m','')::numeric,        TRUE,  TRUE,   1, 'offense'),
        ('Foul Drawing',       NULLIF(p_stats->>'fta','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Ball Security',      NULLIF(p_stats->>'turnover','')::numeric,    TRUE,  FALSE, -1, 'offense'),
        ('Offensive Rebounds', NULLIF(p_stats->>'oreb','')::numeric,        FALSE, FALSE,  1, 'offense'),
        ('Rim Protection',     NULLIF(p_stats->>'blk','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Steals',             NULLIF(p_stats->>'stl','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Rebounding',         NULLIF(p_stats->>'reb','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Defensive Rebounds', NULLIF(p_stats->>'dreb','')::numeric,        FALSE, FALSE,  1, 'defense'),
        ('Opp FG%',            NULLIF(p_stats->>'def_fg_pct','')::numeric,  FALSE, FALSE, -1, 'defense'),
        ('Opp 3PT%',           NULLIF(p_stats->>'def_fg3_pct','')::numeric, FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'

    UNION ALL
    -- NFL team — offense / defense. +Yards Allowed (composite -z). Margin DROPPED.
    SELECT * FROM (VALUES
        ('Total Yards',        NULLIF(p_stats->>'total_yards','')::numeric,                TRUE,  TRUE,   1, 'offense'),
        ('Giveaways',          NULLIF(p_stats->>'turnovers','')::numeric,                  TRUE,  FALSE, -1, 'offense'),
        ('Touchdowns',         COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                             + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0),      FALSE, FALSE,  1, 'offense'),
        ('First Downs',        NULLIF(p_stats->>'first_downs','')::numeric,                FALSE, FALSE,  1, 'offense'),
        ('Field Goals',        NULLIF(p_stats->>'field_goals_made','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Red Zone %',         NULLIF(p_stats->>'red_zone_pct','')::numeric,               FALSE, FALSE,  1, 'offense'),
        ('Third Down %',       NULLIF(p_stats->>'third_down_pct','')::numeric,             FALSE, FALSE,  1, 'offense'),
        ('Tackling',           NULLIF(p_stats->>'total_tackles','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('Sacks',              NULLIF(p_stats->>'defensive_sacks','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Pass Defense',       NULLIF(p_stats->>'passes_defended','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',      NULLIF(p_stats->>'defensive_interceptions','')::numeric,    TRUE,  TRUE,   1, 'defense'),
        ('Yards Allowed',      NULLIF(p_stats->>'yards_allowed','')::numeric,              TRUE,  FALSE, -1, 'defense'),
        ('Tackles For Loss',   NULLIF(p_stats->>'tackles_for_loss','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Takeaways',          NULLIF(p_stats->>'takeaways','')::numeric,                  FALSE, FALSE,  1, 'defense'),
        ('Red Zone Def %',     NULLIF(p_stats->>'red_zone_def_pct','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Third Down Def %',   NULLIF(p_stats->>'third_down_def_pct','')::numeric,         FALSE, FALSE, -1, 'defense'),
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense'),
        ('Penalty Yards For',     NULLIF(p_stats->>'penalty_yards_drawn','')::numeric,     TRUE,  FALSE,  1, 'discipline'),
        ('Penalty Yards Against', NULLIF(p_stats->>'penalty_yards','')::numeric,           TRUE,  FALSE, -1, 'discipline')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team — offense (attacking+possession) / defense. +SoT Allowed
    -- (composite -z). Margin (goal_difference) DROPPED. Cards/injuries = display.
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'key_passes','')::numeric,              TRUE,  TRUE,   1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           TRUE,  FALSE,  1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        ('Big Chances Created',  NULLIF(p_stats->>'big_chances_created','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Successful Dribbles',  NULLIF(p_stats->>'successful_dribbles','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Tackling',             NULLIF(p_stats->>'tackles','')::numeric,                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        NULLIF(p_stats->>'interceptions','')::numeric,           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE,  FALSE, -1, 'defense'),
        ('Penalties Conceded',   NULLIF(p_stats->>'penalties_committed','')::numeric,      TRUE,  FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Ball Recovery',        NULLIF(p_stats->>'ball_recovery','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Shots Allowed',        NULLIF(p_stats->>'shots_allowed','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      FALSE, FALSE, -1, 'defense'),
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       FALSE, FALSE, -1, 'discipline'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'discipline'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                FALSE, FALSE, -1, 'squad')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$$;

-- Additive backfill: splice penalties_won onto football team-seasons.
UPDATE team_stats ts
   SET stats = ts.stats || (
         SELECT COALESCE(jsonb_object_agg(e.k, e.v), '{}'::jsonb)
           FROM jsonb_each(football.aggregate_team_season(ts.team_id, ts.season, ts.league_id)) AS e(k, v)
          WHERE e.k = ANY (ARRAY['penalties_won'])
       ), updated_at = NOW()
 WHERE ts.sport = 'FOOTBALL';

-- Recompute football player + team ratings (player composite unchanged — Penalties
-- Won is specialist-only; team offense composite gains it).
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM player_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_rating('FOOTBALL', s);
    END LOOP;
    FOR s IN SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_team_rating('FOOTBALL', s);
    END LOOP;
END $$;

COMMIT;
