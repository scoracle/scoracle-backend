-- ============================================================================
-- 040_penalties_rating.sql
-- Penalties enter the team z-score (bottom-up from the box score):
--   FOOTBALL: Penalties Conceded (penalties_committed) → negative z, DEFENSE.
--   NFL: Penalty Yards For (opponent's penalty_yards, DERIVED via the opp
--        self-join) → +z, and Penalty Yards Against (own penalty_yards) → -z,
--        in a `discipline` facet ("the penalty battle, both ways").
--
-- Gate-checked (2025): football penalties_committed corr ≤0.22 vs the defensive
-- terms (distinct), spread avg 5.9/sd 2.5; NFL penalty-yard margin reads true
-- (Rams +340, Bucs +257). penalties_committed is omitted-when-zero at event grain
-- but for a discrete event absence reliably = 0, so the SUM is truthful.
--
-- aggregate_team_season (football: penalties_committed; nfl: penalty_yards_drawn)
-- copied verbatim from sql/{football,nfl}.sql; additive backfill splices the new
-- keys; rating_datapoints_team gains the 3 datapoints; team ratings recomputed
-- (football defense + NFL composites move — intended).
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

CREATE OR REPLACE FUNCTION nfl.aggregate_team_season(
    p_team_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score = opp.score THEN 1 ELSE 0 END)::numeric AS ties,
        SUM(COALESCE(ets.score, 0))::numeric AS pf_sum,
        SUM(COALESCE(opp.score, 0))::numeric AS pa_sum,
        -- Offense (passing)
        SUM(COALESCE((ets.stats->>'passing_yards')::numeric, 0))        AS pass_yds_sum,
        SUM(COALESCE((ets.stats->>'passing_touchdowns')::numeric, 0))   AS pass_td_sum,
        SUM(COALESCE((ets.stats->>'passing_attempts')::numeric, 0))     AS pass_att_sum,
        SUM(COALESCE((ets.stats->>'passing_completions')::numeric, 0))  AS pass_cmp_sum,
        SUM(COALESCE((ets.stats->>'passing_interceptions')::numeric, 0))AS pass_int_sum,
        AVG(NULLIF((ets.stats->>'qbr')::numeric, NULL))                 AS qbr_avg,
        AVG(NULLIF((ets.stats->>'qb_rating')::numeric, NULL))           AS qb_rating_avg,
        -- Offense (rushing)
        SUM(COALESCE((ets.stats->>'rushing_yards')::numeric, 0))        AS rush_yds_sum,
        SUM(COALESCE((ets.stats->>'rushing_touchdowns')::numeric, 0))   AS rush_td_sum,
        SUM(COALESCE((ets.stats->>'rushing_attempts')::numeric, 0))     AS rush_att_sum,
        -- Defense
        SUM(COALESCE((ets.stats->>'defensive_sacks')::numeric, 0))      AS sacks_sum,
        SUM(COALESCE((ets.stats->>'defensive_interceptions')::numeric, 0)) AS int_def_sum,
        SUM(COALESCE((ets.stats->>'interception_touchdowns')::numeric, 0)) AS int_td_sum,
        SUM(COALESCE((ets.stats->>'total_tackles')::numeric, 0))        AS tackles_sum,
        SUM(COALESCE((ets.stats->>'solo_tackles')::numeric, 0))         AS solo_tackles_sum,
        SUM(COALESCE((ets.stats->>'passes_defended')::numeric, 0))      AS pd_sum,
        SUM(COALESCE((ets.stats->>'tackles_for_loss')::numeric, 0))     AS tfl_sum,
        SUM(COALESCE((ets.stats->>'qb_hits')::numeric, 0))              AS qbh_sum,
        SUM(COALESCE((ets.stats->>'fumbles_recovered')::numeric, 0))    AS fum_rec_sum,
        SUM(COALESCE((ets.stats->>'fumbles_touchdowns')::numeric, 0))   AS fum_td_sum,
        -- Turnovers
        SUM(COALESCE((ets.stats->>'fumbles')::numeric, 0))              AS fum_sum,
        SUM(COALESCE((ets.stats->>'fumbles_lost')::numeric, 0))         AS fum_lost_sum,
        SUM(COALESCE((opp.stats->>'fumbles_lost')::numeric, 0))         AS opp_fum_lost_sum,
        SUM(COALESCE((opp.stats->>'passing_interceptions')::numeric, 0))AS opp_pass_int_sum,
        -- Kicking
        SUM(COALESCE((ets.stats->>'field_goals_made')::numeric, 0))     AS fg_made_sum,
        SUM(COALESCE((ets.stats->>'field_goal_attempts')::numeric, 0))  AS fg_att_sum,
        SUM(COALESCE((ets.stats->>'extra_points_made')::numeric, 0))    AS xp_sum,
        -- Special teams
        SUM(COALESCE((ets.stats->>'punts')::numeric, 0))                AS punts_sum,
        SUM(COALESCE((ets.stats->>'punt_yards')::numeric, 0))           AS punt_yds_sum,
        SUM(COALESCE((ets.stats->>'punts_inside_20')::numeric, 0))      AS punts_in20_sum,
        SUM(COALESCE((ets.stats->>'touchbacks')::numeric, 0))           AS touchback_sum,
        SUM(COALESCE((ets.stats->>'kick_returns')::numeric, 0))         AS kr_sum,
        SUM(COALESCE((ets.stats->>'kick_return_yards')::numeric, 0))    AS kr_yds_sum,
        SUM(COALESCE((ets.stats->>'kick_return_touchdowns')::numeric, 0)) AS kr_td_sum,
        SUM(COALESCE((ets.stats->>'punt_returns')::numeric, 0))         AS pr_sum,
        SUM(COALESCE((ets.stats->>'punt_return_yards')::numeric, 0))    AS pr_yds_sum,
        SUM(COALESCE((ets.stats->>'punt_return_touchdowns')::numeric, 0)) AS pr_td_sum,
        -- Team-only (BDL /nfl/v1/team_stats)
        SUM(COALESCE((ets.stats->>'first_downs')::numeric, 0))                AS first_downs_sum,
        SUM(COALESCE((ets.stats->>'first_downs_passing')::numeric, 0))        AS first_downs_pass_sum,
        SUM(COALESCE((ets.stats->>'first_downs_rushing')::numeric, 0))        AS first_downs_rush_sum,
        SUM(COALESCE((ets.stats->>'first_downs_penalty')::numeric, 0))        AS first_downs_pen_sum,
        SUM(COALESCE((ets.stats->>'third_down_attempts')::numeric, 0))        AS third_att_sum,
        SUM(COALESCE((ets.stats->>'third_down_conversions')::numeric, 0))     AS third_conv_sum,
        SUM(COALESCE((ets.stats->>'fourth_down_attempts')::numeric, 0))       AS fourth_att_sum,
        SUM(COALESCE((ets.stats->>'fourth_down_conversions')::numeric, 0))    AS fourth_conv_sum,
        SUM(COALESCE((ets.stats->>'red_zone_attempts')::numeric, 0))          AS rz_att_sum,
        SUM(COALESCE((ets.stats->>'red_zone_scores')::numeric, 0))            AS rz_score_sum,
        SUM(COALESCE((ets.stats->>'total_drives')::numeric, 0))               AS drives_sum,
        SUM(COALESCE((ets.stats->>'total_offensive_plays')::numeric, 0))      AS plays_sum,
        SUM(COALESCE((ets.stats->>'net_passing_yards')::numeric, 0))          AS net_pass_yds_sum,
        SUM(COALESCE((ets.stats->>'sack_yards_lost')::numeric, 0))            AS sack_yds_lost_sum,
        SUM(COALESCE((ets.stats->>'possession_time_seconds')::numeric, 0))    AS poss_seconds_sum,
        SUM(COALESCE((ets.stats->>'penalties')::numeric, 0))                  AS penalties_sum,
        SUM(COALESCE((ets.stats->>'penalty_yards')::numeric, 0))              AS penalty_yds_sum,
        SUM(COALESCE((ets.stats->>'defensive_touchdowns')::numeric, 0))       AS def_td_sum,
        -- Opponent production allowed (other team's box score, same fixture) → defensive suppression.
        SUM(COALESCE((opp.stats->>'total_yards')::numeric, 0))                AS opp_yards_sum,
        SUM(COALESCE((opp.stats->>'penalty_yards')::numeric, 0))              AS opp_penalty_yds_sum,
        SUM(COALESCE((opp.stats->>'first_downs')::numeric, 0))                AS opp_first_downs_sum,
        SUM(COALESCE((opp.stats->>'red_zone_scores')::numeric, 0))            AS opp_rz_score_sum,
        SUM(COALESCE((opp.stats->>'red_zone_attempts')::numeric, 0))          AS opp_rz_att_sum,
        SUM(COALESCE((opp.stats->>'third_down_conversions')::numeric, 0))     AS opp_third_conv_sum,
        SUM(COALESCE((opp.stats->>'third_down_attempts')::numeric, 0))        AS opp_third_att_sum,
        SUM(COALESCE((opp.stats->>'total_offensive_plays')::numeric, 0))      AS opp_plays_sum
    FROM public.event_team_stats ets
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'NFL'
      AND ets.season = p_season
      AND ets.league_id = p_league_id
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'wins', wins::int,
            'losses', losses::int,
            'ties', ties::int,
            'points_for', pf_sum::int,
            'points_against', pa_sum::int,
            'point_differential', (pf_sum - pa_sum)::int,
            'points_per_game', ROUND(pf_sum / gp, 1),
            'points_allowed_per_game', ROUND(pa_sum / gp, 1),
            -- Offense
            'passing_yards', pass_yds_sum::int,
            'passing_touchdowns', pass_td_sum::int,
            'passing_attempts', pass_att_sum::int,
            'passing_completions', pass_cmp_sum::int,
            'passing_interceptions', pass_int_sum::int,
            'passing_completion_pct', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_cmp_sum / pass_att_sum * 100, 1) END,
            'yards_per_pass_attempt', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_yds_sum / pass_att_sum, 2) END,
            'qbr', ROUND(qbr_avg, 1),
            'qb_rating', ROUND(qb_rating_avg, 1),
            'rushing_yards', rush_yds_sum::int,
            'rushing_touchdowns', rush_td_sum::int,
            'rushing_attempts', rush_att_sum::int,
            'yards_per_rush_attempt', CASE WHEN rush_att_sum > 0 THEN ROUND(rush_yds_sum / rush_att_sum, 2) END,
            'total_yards', (pass_yds_sum + rush_yds_sum)::int,
            'yards_per_game', ROUND((pass_yds_sum + rush_yds_sum) / gp, 1),
            -- Defense
            'defensive_sacks', ROUND(sacks_sum, 1),
            'defensive_interceptions', int_def_sum::int,
            'interception_touchdowns', int_td_sum::int,
            'total_tackles', tackles_sum::int,
            'solo_tackles', solo_tackles_sum::int,
            'tackles_for_loss', tfl_sum::int,
            'qb_hits', qbh_sum::int,
            'passes_defended', pd_sum::int,
            'fumbles_recovered', fum_rec_sum::int,
            'fumbles_touchdowns', fum_td_sum::int
        ) || jsonb_build_object(
            -- Turnovers
            'fumbles', fum_sum::int,
            'fumbles_lost', fum_lost_sum::int,
            'turnovers', (pass_int_sum + fum_lost_sum)::int,
            'takeaways', (opp_pass_int_sum + opp_fum_lost_sum)::int,
            'turnover_differential', ((opp_pass_int_sum + opp_fum_lost_sum) - (pass_int_sum + fum_lost_sum))::int,
            -- Kicking
            'field_goals_made', fg_made_sum::int,
            'field_goal_attempts', fg_att_sum::int,
            'field_goal_pct', CASE WHEN fg_att_sum > 0 THEN ROUND(fg_made_sum / fg_att_sum * 100, 1) END,
            'extra_points_made', xp_sum::int,
            -- Special teams
            'punts', punts_sum::int,
            'punt_yards', punt_yds_sum::int,
            'punts_inside_20', punts_in20_sum::int,
            'gross_avg_punt_yards', CASE WHEN punts_sum > 0 THEN ROUND(punt_yds_sum / punts_sum, 1) END,
            'touchbacks', touchback_sum::int,
            'kick_returns', kr_sum::int,
            'kick_return_yards', kr_yds_sum::int,
            'kick_return_touchdowns', kr_td_sum::int,
            'yards_per_kick_return', CASE WHEN kr_sum > 0 THEN ROUND(kr_yds_sum / kr_sum, 2) END,
            'punt_returns', pr_sum::int,
            'punt_return_yards', pr_yds_sum::int,
            'punt_return_touchdowns', pr_td_sum::int,
            'yards_per_punt_return', CASE WHEN pr_sum > 0 THEN ROUND(pr_yds_sum / pr_sum, 2) END
        ) || jsonb_build_object(
            -- Team-only aggregates (BDL /nfl/v1/team_stats)
            'first_downs', first_downs_sum::int,
            'first_downs_passing', first_downs_pass_sum::int,
            'first_downs_rushing', first_downs_rush_sum::int,
            'first_downs_penalty', first_downs_pen_sum::int,
            'third_down_attempts', third_att_sum::int,
            'third_down_conversions', third_conv_sum::int,
            'third_down_pct', CASE WHEN third_att_sum > 0 THEN ROUND(third_conv_sum / third_att_sum * 100, 1) END,
            'fourth_down_attempts', fourth_att_sum::int,
            'fourth_down_conversions', fourth_conv_sum::int,
            'fourth_down_pct', CASE WHEN fourth_att_sum > 0 THEN ROUND(fourth_conv_sum / fourth_att_sum * 100, 1) END,
            'red_zone_attempts', rz_att_sum::int,
            'red_zone_scores', rz_score_sum::int,
            'red_zone_pct', CASE WHEN rz_att_sum > 0 THEN ROUND(rz_score_sum / rz_att_sum * 100, 1) END,
            'total_drives', drives_sum::int,
            'total_offensive_plays', plays_sum::int,
            'yards_per_play', CASE WHEN plays_sum > 0 THEN ROUND((pass_yds_sum + rush_yds_sum) / plays_sum, 2) END,
            'net_passing_yards', net_pass_yds_sum::int,
            'sack_yards_lost', sack_yds_lost_sum::int,
            'possession_time_seconds', poss_seconds_sum::int,
            'avg_possession_seconds', CASE WHEN gp > 0 THEN ROUND(poss_seconds_sum / gp, 1) END,
            'penalties', penalties_sum::int,
            'penalty_yards', penalty_yds_sum::int,
            'penalty_yards_drawn', opp_penalty_yds_sum::int,
            'defensive_touchdowns', def_td_sum::int,
            -- Opponent-allowed (defensive suppression, derived from opponent box scores).
            -- yards_allowed is the composite −z term (gate-checked distinct, corr ≤0.34 vs the
            -- splash-play terms); first_downs_allowed (0.90 collinear) + the rates are display-only.
            'yards_allowed', opp_yards_sum::int,
            'first_downs_allowed', opp_first_downs_sum::int,
            'red_zone_def_pct', CASE WHEN opp_rz_att_sum > 0 THEN ROUND(opp_rz_score_sum / opp_rz_att_sum * 100, 1) END,
            'third_down_def_pct', CASE WHEN opp_third_att_sum > 0 THEN ROUND(opp_third_conv_sum / opp_third_att_sum * 100, 1) END,
            'yards_per_play_allowed', CASE WHEN opp_plays_sum > 0 THEN ROUND(opp_yards_sum / opp_plays_sum, 2) END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

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

-- Additive backfill: splice the new keys onto each team-season (preserves all else).
UPDATE team_stats ts
   SET stats = ts.stats || (
         SELECT COALESCE(jsonb_object_agg(e.k, e.v), '{}'::jsonb)
           FROM jsonb_each(football.aggregate_team_season(ts.team_id, ts.season, ts.league_id)) AS e(k, v)
          WHERE e.k = ANY (ARRAY['penalties_committed'])
       ), updated_at = NOW()
 WHERE ts.sport = 'FOOTBALL';

UPDATE team_stats ts
   SET stats = ts.stats || (
         SELECT COALESCE(jsonb_object_agg(e.k, e.v), '{}'::jsonb)
           FROM jsonb_each(nfl.aggregate_team_season(ts.team_id, ts.season, ts.league_id)) AS e(k, v)
          WHERE e.k = ANY (ARRAY['penalty_yards_drawn'])
       ), updated_at = NOW()
 WHERE ts.sport = 'NFL';

-- Recompute team ratings so the new penalty datapoints take effect.
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM team_stats WHERE sport IN ('FOOTBALL','NFL') ORDER BY sport, season LOOP
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;
