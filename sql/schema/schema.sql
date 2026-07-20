--
-- PostgreSQL database dump
--

\restrict D9NGRPSJLCYRat2c2f8pZ2ncPpPFHlNWZiIY2jFRWyEqaFY8cHXPs50qwEFsBJ9

-- Dumped from database version 18.4
-- Dumped by pg_dump version 18.4

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: football; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA football;


--
-- Name: nba; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA nba;


--
-- Name: nfl; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA nfl;


--
-- Name: unaccent; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;


--
-- Name: EXTENSION unaccent; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION unaccent IS 'text search dictionary that removes accents';


--
-- Name: aggregate_player_season(integer, integer, integer); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.aggregate_player_season(p_player_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
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
$$;


--
-- Name: aggregate_team_season(integer, integer, integer); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.aggregate_team_season(p_team_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
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
$$;


--
-- Name: compute_derived_player_stats(); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.compute_derived_player_stats() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC;
    tackles NUMERIC; tackles_w NUMERIC;
    crosses_t NUMERIC; crosses_a NUMERIC;
    long_balls_t NUMERIC; long_balls_w NUMERIC;
    aerials_t NUMERIC; aerials_w NUMERIC;
    saves NUMERIC; conceded NUMERIC;
BEGIN
    -- Fantasy points (FPL-style, season total) — before apply_rate_siblings so its
    -- per_90 / per_game siblings emit (fantasy_points is rate_sibling=TRUE).
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', football.fantasy_points(NEW.stats, NEW.position));

    -- Rate siblings (per_90, per_game) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('FOOTBALL', NEW.stats);

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
$$;


--
-- Name: fantasy_points(jsonb, text); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.fantasy_points(p jsonb, p_position text) RETURNS numeric
    LANGUAGE sql IMMUTABLE
    AS $$
    -- FPL-style on season totals. Goal value by position group; GK save/penalty
    -- credit + GK/DEF goals-conceded penalty; discipline deductions. Clean-sheet
    -- and bonus points omitted by design (per-match, unreconstructable from totals);
    -- appearance points approximated as 2/appearance. Unknown position group → ELSE
    -- branch (attacker-style goals, no GK/DEF credit).
    SELECT ROUND(
          2.0 * COALESCE((p->>'appearances')::numeric, 0)
        + (CASE public.position_group('FOOTBALL', p_position)
             WHEN 'goalkeeper' THEN 6.0
             WHEN 'defender'   THEN 6.0
             WHEN 'midfielder' THEN 5.0
             ELSE 4.0 END) * COALESCE((p->>'goals')::numeric, 0)
        + 3.0 * COALESCE((p->>'assists')::numeric, 0)
        - 2.0 * COALESCE((p->>'penalties_missed')::numeric, 0)
        - 1.0 * COALESCE((p->>'yellow_cards')::numeric, 0)
        - 3.0 * COALESCE((p->>'red_cards')::numeric, 0)
        - 2.0 * COALESCE((p->>'own_goals')::numeric, 0)
        + CASE WHEN public.position_group('FOOTBALL', p_position) IN ('goalkeeper','defender')
               THEN -1.0 * FLOOR(COALESCE((p->>'goals_conceded')::numeric, 0) / 2.0)
               ELSE 0 END
        + CASE WHEN public.position_group('FOOTBALL', p_position) = 'goalkeeper'
               THEN FLOOR(COALESCE((p->>'saves')::numeric, 0) / 3.0)
                  + 5.0 * COALESCE((p->>'penalties_saved')::numeric, 0)
               ELSE 0 END
    , 2);
$$;


--
-- Name: health(); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.health() RETURNS json
    LANGUAGE sql STABLE
    AS $$
    SELECT json_build_object('status', 'ok');
$$;


--
-- Name: stat_leaders(integer, text, integer, text, integer); Type: FUNCTION; Schema: football; Owner: -
--

CREATE FUNCTION football.stat_leaders(p_season integer, p_stat_name text, p_limit integer DEFAULT 25, p_position text DEFAULT NULL::text, p_league_id integer DEFAULT 0) RETURNS TABLE(rank bigint, player_id integer, "position" text, team_name text, stat_value numeric)
    LANGUAGE sql STABLE
    AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'FOOTBALL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$;


--
-- Name: aggregate_player_season(integer, integer, integer); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.aggregate_player_season(p_player_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        AVG(minutes_played) AS minutes_avg,
        AVG(NULLIF((stats->>'pts')::numeric, NULL)) AS pts_avg,
        AVG(NULLIF((stats->>'reb')::numeric, NULL)) AS reb_avg,
        AVG(NULLIF((stats->>'ast')::numeric, NULL)) AS ast_avg,
        AVG(NULLIF((stats->>'stl')::numeric, NULL)) AS stl_avg,
        AVG(NULLIF((stats->>'blk')::numeric, NULL)) AS blk_avg,
        AVG(NULLIF((stats->>'turnover')::numeric, NULL)) AS tov_avg,
        AVG(NULLIF((stats->>'pf')::numeric, NULL)) AS pf_avg,
        AVG(NULLIF((stats->>'plus_minus')::numeric, NULL)) AS pm_avg,
        AVG(NULLIF((stats->>'oreb')::numeric, NULL)) AS oreb_avg,
        AVG(NULLIF((stats->>'dreb')::numeric, NULL)) AS dreb_avg,
        AVG(NULLIF((stats->>'fgm')::numeric, NULL)) AS fgm_avg,
        AVG(NULLIF((stats->>'fga')::numeric, NULL)) AS fga_avg,
        AVG(NULLIF((stats->>'fg3m')::numeric, NULL)) AS fg3m_avg,
        AVG(NULLIF((stats->>'fg3a')::numeric, NULL)) AS fg3a_avg,
        AVG(NULLIF((stats->>'ftm')::numeric, NULL)) AS ftm_avg,
        AVG(NULLIF((stats->>'fta')::numeric, NULL)) AS fta_avg,
        SUM(COALESCE((stats->>'fgm')::numeric, 0)) AS fgm_sum,
        SUM(COALESCE((stats->>'fga')::numeric, 0)) AS fga_sum,
        SUM(COALESCE((stats->>'fg3m')::numeric, 0)) AS fg3m_sum,
        SUM(COALESCE((stats->>'fg3a')::numeric, 0)) AS fg3a_sum,
        SUM(COALESCE((stats->>'ftm')::numeric, 0)) AS ftm_sum,
        SUM(COALESCE((stats->>'fta')::numeric, 0)) AS fta_sum
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'NBA'
      AND season = p_season
      AND league_id = p_league_id
      AND COALESCE(minutes_played, 0) > 0
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'minutes', ROUND(minutes_avg, 1),
            'pts', ROUND(pts_avg, 1),
            'reb', ROUND(reb_avg, 1),
            'ast', ROUND(ast_avg, 1),
            'stl', ROUND(stl_avg, 1),
            'blk', ROUND(blk_avg, 1),
            'turnover', ROUND(tov_avg, 1),
            'pf', ROUND(pf_avg, 1),
            'plus_minus', ROUND(pm_avg, 1),
            'oreb', ROUND(oreb_avg, 1),
            'dreb', ROUND(dreb_avg, 1),
            'fgm', ROUND(fgm_avg, 1),
            'fga', ROUND(fga_avg, 1),
            'fg3m', ROUND(fg3m_avg, 1),
            'fg3a', ROUND(fg3a_avg, 1),
            'ftm', ROUND(ftm_avg, 1),
            'fta', ROUND(fta_avg, 1),
            'fg_pct', CASE WHEN fga_sum > 0 THEN ROUND((fgm_sum / fga_sum) * 100, 1) END,
            'fg3_pct', CASE WHEN fg3a_sum > 0 THEN ROUND((fg3m_sum / fg3a_sum) * 100, 1) END,
            'ft_pct', CASE WHEN fta_sum > 0 THEN ROUND((ftm_sum / fta_sum) * 100, 1) END
        )
    )
END
FROM agg;
$$;


--
-- Name: aggregate_team_season(integer, integer, integer); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.aggregate_team_season(p_team_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        AVG(NULLIF((ets.stats->>'pts')::numeric, NULL))       AS pts_avg,
        AVG(NULLIF((opp.stats->>'pts')::numeric, NULL))       AS pts_allowed_avg,
        AVG(NULLIF((ets.stats->>'reb')::numeric, NULL))       AS reb_avg,
        AVG(NULLIF((ets.stats->>'oreb')::numeric, NULL))      AS oreb_avg,
        AVG(NULLIF((ets.stats->>'dreb')::numeric, NULL))      AS dreb_avg,
        AVG(NULLIF((ets.stats->>'ast')::numeric, NULL))       AS ast_avg,
        AVG(NULLIF((ets.stats->>'stl')::numeric, NULL))       AS stl_avg,
        AVG(NULLIF((ets.stats->>'blk')::numeric, NULL))       AS blk_avg,
        AVG(NULLIF((ets.stats->>'turnover')::numeric, NULL))  AS tov_avg,
        AVG(NULLIF((ets.stats->>'pf')::numeric, NULL))        AS pf_avg,
        AVG(NULLIF((ets.stats->>'fgm')::numeric, NULL))       AS fgm_avg,
        AVG(NULLIF((ets.stats->>'fga')::numeric, NULL))       AS fga_avg,
        AVG(NULLIF((ets.stats->>'fg3m')::numeric, NULL))      AS fg3m_avg,
        AVG(NULLIF((ets.stats->>'fg3a')::numeric, NULL))      AS fg3a_avg,
        AVG(NULLIF((ets.stats->>'ftm')::numeric, NULL))       AS ftm_avg,
        AVG(NULLIF((ets.stats->>'fta')::numeric, NULL))       AS fta_avg,
        SUM(COALESCE((ets.stats->>'fgm')::numeric, 0))        AS fgm_sum,
        SUM(COALESCE((ets.stats->>'fga')::numeric, 0))        AS fga_sum,
        SUM(COALESCE((ets.stats->>'fg3m')::numeric, 0))       AS fg3m_sum,
        SUM(COALESCE((ets.stats->>'fg3a')::numeric, 0))       AS fg3a_sum,
        SUM(COALESCE((ets.stats->>'ftm')::numeric, 0))        AS ftm_sum,
        SUM(COALESCE((ets.stats->>'fta')::numeric, 0))        AS fta_sum,
        -- Opponent shooting allowed (other team's box score, same fixture) → shot suppression.
        SUM(COALESCE((opp.stats->>'fgm')::numeric, 0))        AS opp_fgm_sum,
        SUM(COALESCE((opp.stats->>'fga')::numeric, 0))        AS opp_fga_sum,
        SUM(COALESCE((opp.stats->>'fg3m')::numeric, 0))       AS opp_fg3m_sum,
        SUM(COALESCE((opp.stats->>'fg3a')::numeric, 0))       AS opp_fg3a_sum
    FROM public.event_team_stats ets
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'NBA'
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
            'pts', ROUND(pts_avg, 1),
            'pts_allowed', ROUND(pts_allowed_avg, 1),
            'reb', ROUND(reb_avg, 1),
            'oreb', ROUND(oreb_avg, 1),
            'dreb', ROUND(dreb_avg, 1),
            'ast', ROUND(ast_avg, 1),
            'stl', ROUND(stl_avg, 1),
            'blk', ROUND(blk_avg, 1),
            'turnover', ROUND(tov_avg, 1),
            'pf', ROUND(pf_avg, 1),
            'fgm', ROUND(fgm_avg, 1),
            'fga', ROUND(fga_avg, 1),
            'fg3m', ROUND(fg3m_avg, 1),
            'fg3a', ROUND(fg3a_avg, 1),
            'ftm', ROUND(ftm_avg, 1),
            'fta', ROUND(fta_avg, 1),
            'fg_pct', CASE WHEN fga_sum > 0 THEN ROUND((fgm_sum / fga_sum) * 100, 1) END,
            'fg3_pct', CASE WHEN fg3a_sum > 0 THEN ROUND((fg3m_sum / fg3a_sum) * 100, 1) END,
            'ft_pct', CASE WHEN fta_sum > 0 THEN ROUND((ftm_sum / fta_sum) * 100, 1) END,
            -- Opponent shooting allowed (defensive shot suppression). Display-only:
            -- rates → ~0 z, so not a composite term — shown as a defense percentile.
            'def_fg_pct',  CASE WHEN opp_fga_sum  > 0 THEN ROUND((opp_fgm_sum  / opp_fga_sum)  * 100, 1) END,
            'def_fg3_pct', CASE WHEN opp_fg3a_sum > 0 THEN ROUND((opp_fg3m_sum / opp_fg3a_sum) * 100, 1) END
        )
    )
END
FROM agg;
$$;


--
-- Name: compute_derived_player_stats(); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.compute_derived_player_stats() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    pts NUMERIC; reb NUMERIC; ast NUMERIC; stl NUMERIC; blk NUMERIC;
    fga NUMERIC; fgm NUMERIC; fta NUMERIC; ftm NUMERIC; turnover NUMERIC;
    tsa NUMERIC;
BEGIN
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

    -- Fantasy points (DraftKings, per-game) — computed before the sibling pass so the
    -- rate loops pick up its siblings.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nba.fantasy_points(NEW.stats));

    -- Rate siblings (per_36, per_season) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('NBA', NEW.stats);

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
$$;


--
-- Name: compute_derived_team_stats(); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.compute_derived_team_stats() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    wins NUMERIC; losses NUMERIC; total NUMERIC;
    pts NUMERIC; pts_allowed NUMERIC;
    fgm NUMERIC; fga NUMERIC; fg3m NUMERIC; fta NUMERIC; ftm NUMERIC;
    reb NUMERIC; ast NUMERIC; stl NUMERIC; blk NUMERIC; turnover NUMERIC;
    tsa NUMERIC;
BEGIN
    wins        := (NEW.stats->>'wins')::NUMERIC;
    losses      := (NEW.stats->>'losses')::NUMERIC;
    pts         := (NEW.stats->>'pts')::NUMERIC;
    pts_allowed := (NEW.stats->>'pts_allowed')::NUMERIC;
    fgm         := (NEW.stats->>'fgm')::NUMERIC;
    fga         := (NEW.stats->>'fga')::NUMERIC;
    fg3m        := (NEW.stats->>'fg3m')::NUMERIC;
    fta         := (NEW.stats->>'fta')::NUMERIC;
    ftm         := (NEW.stats->>'ftm')::NUMERIC;
    reb         := (NEW.stats->>'reb')::NUMERIC;
    ast         := (NEW.stats->>'ast')::NUMERIC;
    stl         := (NEW.stats->>'stl')::NUMERIC;
    blk         := (NEW.stats->>'blk')::NUMERIC;
    turnover    := (NEW.stats->>'turnover')::NUMERIC;

    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses;
        IF total > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3)); END IF;
    END IF;

    IF pts IS NOT NULL AND pts_allowed IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('point_differential', ROUND(pts - pts_allowed, 1));
    END IF;

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1)); END IF;
    END IF;

    IF fga IS NOT NULL AND fga > 0 AND fgm IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efg_pct', ROUND((fgm + 0.5 * COALESCE(fg3m, 0)) / fga * 100, 1));
    END IF;

    IF turnover IS NOT NULL AND turnover > 0 AND ast IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('ast_to_tov', ROUND(ast / turnover, 2));
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL AND fta IS NOT NULL AND ftm IS NOT NULL AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    RETURN NEW;
END;
$$;


--
-- Name: fantasy_points(jsonb); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.fantasy_points(p jsonb) RETURNS numeric
    LANGUAGE sql IMMUTABLE
    AS $$
    -- DraftKings NBA on per-game averages → per-game fantasy points. DD/TD bonus
    -- omitted (per-game, unreconstructable from season averages).
    SELECT ROUND(
          1.0  * COALESCE((p->>'pts')::numeric, 0)
        + 0.5  * COALESCE((p->>'fg3m')::numeric, 0)
        + 1.25 * COALESCE((p->>'reb')::numeric, 0)
        + 1.5  * COALESCE((p->>'ast')::numeric, 0)
        + 2.0  * COALESCE((p->>'stl')::numeric, 0)
        + 2.0  * COALESCE((p->>'blk')::numeric, 0)
        - 0.5  * COALESCE((p->>'turnover')::numeric, 0)
    , 2);
$$;


--
-- Name: health(); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.health() RETURNS json
    LANGUAGE sql STABLE
    AS $$
    SELECT json_build_object('status', 'ok');
$$;


--
-- Name: stat_leaders(integer, text, integer, text, integer); Type: FUNCTION; Schema: nba; Owner: -
--

CREATE FUNCTION nba.stat_leaders(p_season integer, p_stat_name text, p_limit integer DEFAULT 25, p_position text DEFAULT NULL::text, p_league_id integer DEFAULT 0) RETURNS TABLE(rank bigint, player_id integer, "position" text, team_name text, stat_value numeric)
    LANGUAGE sql STABLE
    AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NBA' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$;


--
-- Name: FUNCTION stat_leaders(p_season integer, p_stat_name text, p_limit integer, p_position text, p_league_id integer); Type: COMMENT; Schema: nba; Owner: -
--

COMMENT ON FUNCTION nba.stat_leaders(p_season integer, p_stat_name text, p_limit integer, p_position text, p_league_id integer) IS 'Returns top N NBA players by stat category with positional filtering. Position sourced from player_stats.';


--
-- Name: aggregate_player_season(integer, integer, integer); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.aggregate_player_season(p_player_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        -- Passing
        SUM(COALESCE((stats->>'passing_completions')::numeric, 0)) AS pass_cmp_sum,
        SUM(COALESCE((stats->>'passing_attempts')::numeric, 0)) AS pass_att_sum,
        SUM(COALESCE((stats->>'passing_yards')::numeric, 0)) AS pass_yds_sum,
        SUM(COALESCE((stats->>'passing_touchdowns')::numeric, 0)) AS pass_td_sum,
        SUM(COALESCE((stats->>'passing_interceptions')::numeric, 0)) AS pass_int_sum,
        SUM(COALESCE((stats->>'qbr')::numeric, 0)) AS qbr_sum,
        COUNT(*) FILTER (WHERE (stats->>'qbr') IS NOT NULL)::numeric AS qbr_games,
        SUM(COALESCE((stats->>'qb_rating')::numeric, 0)) AS qb_rating_sum,
        COUNT(*) FILTER (WHERE (stats->>'qb_rating') IS NOT NULL)::numeric AS qb_rating_games,
        SUM(COALESCE((stats->>'sacks')::numeric, 0)) AS sacks_taken_sum,
        SUM(COALESCE((stats->>'sacks_loss')::numeric, 0)) AS sack_yards_lost_sum,
        MAX(COALESCE((stats->>'long_pass')::numeric, 0)) AS long_pass_max,
        -- Rushing
        SUM(COALESCE((stats->>'rushing_attempts')::numeric, 0)) AS rush_att_sum,
        SUM(COALESCE((stats->>'rushing_yards')::numeric, 0)) AS rush_yds_sum,
        SUM(COALESCE((stats->>'rushing_touchdowns')::numeric, 0)) AS rush_td_sum,
        MAX(COALESCE((stats->>'long_rushing')::numeric, 0)) AS long_rushing_max,
        -- Receiving
        SUM(COALESCE((stats->>'receptions')::numeric, 0)) AS rec_sum,
        SUM(COALESCE((stats->>'receiving_targets')::numeric, 0)) AS tgt_sum,
        SUM(COALESCE((stats->>'receiving_yards')::numeric, 0)) AS rec_yds_sum,
        SUM(COALESCE((stats->>'receiving_touchdowns')::numeric, 0)) AS rec_td_sum,
        MAX(COALESCE((stats->>'long_reception')::numeric, 0)) AS long_reception_max,
        -- General ball-security
        SUM(COALESCE((stats->>'fumbles')::numeric, 0)) AS fum_sum,
        SUM(COALESCE((stats->>'fumbles_lost')::numeric, 0)) AS fum_lost_sum,
        -- Defense
        SUM(COALESCE((stats->>'total_tackles')::numeric, 0)) AS tackles_sum,
        SUM(COALESCE((stats->>'solo_tackles')::numeric, 0)) AS solo_tackles_sum,
        SUM(COALESCE((stats->>'defensive_sacks')::numeric, 0)) AS sacks_sum,
        SUM(COALESCE((stats->>'defensive_interceptions')::numeric, 0)) AS int_def_sum,
        SUM(COALESCE((stats->>'interception_touchdowns')::numeric, 0)) AS int_td_sum,
        SUM(COALESCE((stats->>'interception_yards')::numeric, 0)) AS int_yds_sum,
        SUM(COALESCE((stats->>'fumbles_recovered')::numeric, 0)) AS fum_rec_sum,
        SUM(COALESCE((stats->>'fumbles_touchdowns')::numeric, 0)) AS fum_td_sum,
        SUM(COALESCE((stats->>'tackles_for_loss')::numeric, 0)) AS tfl_sum,
        SUM(COALESCE((stats->>'passes_defended')::numeric, 0)) AS pd_sum,
        SUM(COALESCE((stats->>'qb_hits')::numeric, 0)) AS qbh_sum,
        -- Kicking
        SUM(COALESCE((stats->>'field_goal_attempts')::numeric, 0)) AS fg_att_sum,
        SUM(COALESCE((stats->>'field_goals_made')::numeric, 0)) AS fg_made_sum,
        SUM(COALESCE((stats->>'extra_points_made')::numeric, 0)) AS xp_sum,
        SUM(COALESCE((stats->>'total_points')::numeric, 0)) AS points_sum,
        SUM(COALESCE((stats->>'touchbacks')::numeric, 0)) AS touchback_sum,
        MAX(COALESCE((stats->>'long_field_goal_made')::numeric, 0)) AS long_fg_max,
        -- Special teams (BDL key → canonical key)
        SUM(COALESCE((stats->>'punts')::numeric, 0)) AS punts_sum,
        SUM(COALESCE((stats->>'punt_yards')::numeric, 0)) AS punt_yds_sum,
        SUM(COALESCE((stats->>'punts_inside_20')::numeric, 0)) AS punts_in20_sum,
        MAX(COALESCE((stats->>'long_punt')::numeric, 0)) AS long_punt_max,
        SUM(COALESCE((stats->>'kick_returns')::numeric, 0)) AS kr_sum,
        SUM(COALESCE((stats->>'kick_return_yards')::numeric, 0)) AS kr_yds_sum,
        SUM(COALESCE((stats->>'kick_return_touchdowns')::numeric, 0)) AS kr_td_sum,
        MAX(COALESCE((stats->>'long_kick_return')::numeric, 0)) AS long_kr_max,
        SUM(COALESCE((stats->>'punt_returns')::numeric, 0)) AS pr_sum,
        SUM(COALESCE((stats->>'punt_return_yards')::numeric, 0)) AS pr_yds_sum,
        SUM(COALESCE((stats->>'punt_return_touchdowns')::numeric, 0)) AS pr_td_sum,
        MAX(COALESCE((stats->>'long_punt_return')::numeric, 0)) AS long_pr_max
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'NFL'
      AND season = p_season
      AND league_id = p_league_id
      AND NOT (
          COALESCE((stats->>'passing_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'rushing_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'receiving_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'total_tackles')::numeric, 0) = 0
          AND COALESCE((stats->>'fumbles')::numeric, 0) = 0
      )
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'fumbles', fum_sum::int,
            'fumbles_lost', fum_lost_sum::int,
            -- Passing
            'passing_completions', pass_cmp_sum::int,
            'passing_attempts', pass_att_sum::int,
            'passing_yards', pass_yds_sum::int,
            'passing_touchdowns', pass_td_sum::int,
            'passing_interceptions', pass_int_sum::int,
            'passing_yards_per_game', ROUND(pass_yds_sum / gp, 1),
            'passing_completion_pct', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_cmp_sum / pass_att_sum * 100, 1) END,
            'yards_per_pass_attempt', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_yds_sum / pass_att_sum, 2) END,
            'qbr', CASE WHEN qbr_games > 0 THEN ROUND(qbr_sum / qbr_games, 1) END,
            'qb_rating', CASE WHEN qb_rating_games > 0 THEN ROUND(qb_rating_sum / qb_rating_games, 1) END,
            'sacks_taken', CASE WHEN sacks_taken_sum > 0 THEN sacks_taken_sum::int END,
            'sack_yards_lost', CASE WHEN sack_yards_lost_sum > 0 THEN sack_yards_lost_sum::int END,
            'long_pass', CASE WHEN long_pass_max > 0 THEN long_pass_max::int END,
            -- Rushing
            'rushing_attempts', rush_att_sum::int,
            'rushing_yards', rush_yds_sum::int,
            'rushing_touchdowns', rush_td_sum::int,
            'rushing_yards_per_game', ROUND(rush_yds_sum / gp, 1),
            'yards_per_rush_attempt', CASE WHEN rush_att_sum > 0 THEN ROUND(rush_yds_sum / rush_att_sum, 2) END,
            'long_rushing', CASE WHEN long_rushing_max > 0 THEN long_rushing_max::int END,
            -- Receiving
            'receptions', rec_sum::int,
            'receiving_targets', tgt_sum::int,
            'receiving_yards', rec_yds_sum::int,
            'receiving_touchdowns', rec_td_sum::int,
            'receiving_yards_per_game', ROUND(rec_yds_sum / gp, 1),
            'yards_per_reception', CASE WHEN rec_sum > 0 THEN ROUND(rec_yds_sum / rec_sum, 2) END,
            'long_reception', CASE WHEN long_reception_max > 0 THEN long_reception_max::int END,
            -- Defense
            'total_tackles', tackles_sum::int,
            'solo_tackles', solo_tackles_sum::int,
            'assist_tackles', GREATEST(tackles_sum - solo_tackles_sum, 0)::int,
            'defensive_sacks', ROUND(sacks_sum, 1),
            'defensive_interceptions', int_def_sum::int,
            'interception_touchdowns', int_td_sum::int,
            'interception_yards', CASE WHEN int_yds_sum > 0 THEN int_yds_sum::int END,
            'fumbles_recovered', fum_rec_sum::int,
            'fumbles_touchdowns', fum_td_sum::int,
            'tackles_for_loss', tfl_sum::int,
            'passes_defended', pd_sum::int,
            'qb_hits', qbh_sum::int
        ) || jsonb_build_object(
            -- Kicking
            'field_goal_attempts', fg_att_sum::int,
            'field_goals_made', fg_made_sum::int,
            'field_goal_pct', CASE WHEN fg_att_sum > 0 THEN ROUND(fg_made_sum / fg_att_sum * 100, 1) END,
            'long_field_goal_made', CASE WHEN long_fg_max > 0 THEN long_fg_max::int END,
            'extra_points_made', xp_sum::int,
            'total_points', points_sum::int,
            'touchbacks', touchback_sum::int,
            -- Special teams
            'punts', punts_sum::int,
            'punt_yards', punt_yds_sum::int,
            'punts_inside_20', punts_in20_sum::int,
            'avg_punt_yards', CASE WHEN punts_sum > 0 THEN ROUND(punt_yds_sum / punts_sum, 1) END,
            'long_punt', CASE WHEN long_punt_max > 0 THEN long_punt_max::int END,
            'kick_returns', kr_sum::int,
            'kick_return_yards', kr_yds_sum::int,
            'kick_return_touchdowns', kr_td_sum::int,
            'yards_per_kick_return', CASE WHEN kr_sum > 0 THEN ROUND(kr_yds_sum / kr_sum, 2) END,
            'long_kick_return', CASE WHEN long_kr_max > 0 THEN long_kr_max::int END,
            'punt_returner_returns', pr_sum::int,
            'punt_returner_return_yards', pr_yds_sum::int,
            'punt_return_touchdowns', pr_td_sum::int,
            'yards_per_punt_return', CASE WHEN pr_sum > 0 THEN ROUND(pr_yds_sum / pr_sum, 2) END,
            'long_punt_return', CASE WHEN long_pr_max > 0 THEN long_pr_max::int END
        )
    )
END
FROM agg;
$$;


--
-- Name: aggregate_team_season(integer, integer, integer); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.aggregate_team_season(p_team_id integer, p_season integer, p_league_id integer DEFAULT 0) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
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
$$;


--
-- Name: compute_derived_player_stats(); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.compute_derived_player_stats() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
BEGIN
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;

    -- Fantasy points (PPR, season total) — before the sibling pass so its sibling emits.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nfl.fantasy_points(NEW.stats));

    -- Rate siblings (per_game) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('NFL', NEW.stats);

    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('catch_pct', ROUND(rec / targets * 100, 1));
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: compute_derived_team_stats(); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.compute_derived_team_stats() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE wins NUMERIC; losses NUMERIC; ties NUMERIC; total NUMERIC;
BEGIN
    wins := (NEW.stats->>'wins')::NUMERIC; losses := (NEW.stats->>'losses')::NUMERIC;
    ties := COALESCE((NEW.stats->>'ties')::NUMERIC, 0);
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses + ties;
        IF total > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3)); END IF;
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: fantasy_points(jsonb); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.fantasy_points(p jsonb) RETURNS numeric
    LANGUAGE sql IMMUTABLE
    AS $$
    -- Standard PPR on season totals → season-total fantasy points.
    SELECT ROUND(
          0.04 * COALESCE((p->>'passing_yards')::numeric, 0)
        + 4.0  * COALESCE((p->>'passing_touchdowns')::numeric, 0)
        - 2.0  * COALESCE((p->>'passing_interceptions')::numeric, 0)
        + 0.1  * COALESCE((p->>'rushing_yards')::numeric, 0)
        + 6.0  * COALESCE((p->>'rushing_touchdowns')::numeric, 0)
        + 1.0  * COALESCE((p->>'receptions')::numeric, 0)
        + 0.1  * COALESCE((p->>'receiving_yards')::numeric, 0)
        + 6.0  * COALESCE((p->>'receiving_touchdowns')::numeric, 0)
        - 2.0  * COALESCE((p->>'fumbles_lost')::numeric, 0)
    , 2);
$$;


--
-- Name: health(); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.health() RETURNS json
    LANGUAGE sql STABLE
    AS $$
    SELECT json_build_object('status', 'ok');
$$;


--
-- Name: stat_leaders(integer, text, integer, text, integer); Type: FUNCTION; Schema: nfl; Owner: -
--

CREATE FUNCTION nfl.stat_leaders(p_season integer, p_stat_name text, p_limit integer DEFAULT 25, p_position text DEFAULT NULL::text, p_league_id integer DEFAULT 0) RETURNS TABLE(rank bigint, player_id integer, "position" text, team_name text, stat_value numeric)
    LANGUAGE sql STABLE
    AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NFL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$;


--
-- Name: _compute_rating_bundle(text, integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text) RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, composite_score numeric, specialist numeric, specialist_rank numeric, specialist_score numeric, specialty text, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
    LANGUAGE sql STABLE
    AS $$
    WITH lasp AS (
        SELECT CASE WHEN p_sport='FOOTBALL'
                    THEN round(avg(NULLIF(stats->>'save_pct','')::numeric), 4) END AS asp
        FROM player_stats
        WHERE sport='FOOTBALL' AND season=p_season AND position='Goalkeeper'
          AND (stats->>'appearances')::numeric >= 15
    ),
    dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               tm.conference, tm.division,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet,
               -- Phase 2: TAG eligibility instead of filtering it out. Sub-gate players
               -- still produce datapoints (for their breakdown); pop/ranks/scoped below
               -- use `WHERE is_ranked` so the rated cohort is unchanged.
               COALESCE((
                   SELECT bool_and(COALESCE((ps.stats->>rt.stat_key)::numeric, 0) >= rt.min_value)
                   FROM public.rating_thresholds rt WHERE rt.sport = p_sport
                 ), FALSE) AS is_ranked
        FROM player_stats ps
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        LEFT JOIN LATERAL (
            SELECT tts.stats->>'opp_possession_pct' AS opp
            FROM team_stats tts
            WHERE tts.team_id = ps.team_id AND tts.sport = p_sport AND tts.season = p_season
            LIMIT 1
        ) topp ON p_sport = 'FOOTBALL'
        CROSS JOIN lasp
        CROSS JOIN LATERAL rating_datapoints(
            p_sport,
            CASE WHEN p_sport = 'FOOTBALL'
                 THEN ps.stats || jsonb_strip_nulls(jsonb_build_object(
                          'team_opp_possession', topp.opp,
                          'league_avg_save_pct', lasp.asp))
                 ELSE ps.stats END,
            p_rate_mode, ps.position) d
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp WHERE is_ranked GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT player_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY player_id, league_id
    ),
    comp_facet AS (
        SELECT player_id, league_id, SUM(facet_mean) AS composite
        FROM (
            SELECT player_id, league_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY player_id, league_id, facet
        ) fm
        GROUP BY player_id, league_id
    ),
    comp AS (
        SELECT player_id, league_id, composite FROM comp_flat
    ),
    sp AS (
        SELECT DISTINCT ON (player_id, league_id)
               player_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC, label
    ),
    rk AS (
        SELECT DISTINCT player_id, league_id, is_ranked FROM z
    ),
    scored AS (
        -- Rated cohort: percent_rank over the rated set only → byte-identical to before.
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_position,
               CASE WHEN p_sport IN ('NFL','NBA') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, conference ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_conference,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, division ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_division,
               CASE WHEN p_sport='FOOTBALL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, league_id ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_league
        FROM z WHERE is_ranked
        UNION ALL
        -- Sub-gate players: per-stat percentile = standing within the RATED cohort for
        -- that stat (count-based, so it doesn't perturb the cohort's own percent_rank).
        -- Scope cuts omitted (they are unranked).
        SELECT u.player_id, u.league_id, u.label, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr,
               -- Sub-gate fill = the datapoint's standardized magnitude vs the rated
               -- cohort (50 + 10*z in the good direction, clamped 1-99) — the same scale
               -- as rating_score. Fast scalar; a true percentile-vs-cohort is O(n^2).
               ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (u.sign * u.zr)))::numeric, 1) AS pct,
               NULL::numeric AS pct_position, NULL::numeric AS pct_conference,
               NULL::numeric AS pct_division, NULL::numeric AS pct_league
        FROM z u WHERE NOT u.is_ranked
    ),
    bd AS (
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'is_specialty', (sp.specialty IS NOT DISTINCT FROM s.label),
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object(
                       'position', s.pct_position, 'conference', s.pct_conference,
                       'division', s.pct_division, 'league', s.pct_league))
               ) ORDER BY s.label) AS breakdown
        FROM scored s
        LEFT JOIN sp USING (player_id, league_id)
        GROUP BY s.player_id, s.league_id
    ),
    base AS (
        SELECT c.player_id, c.league_id,
               ROUND(c.composite, 4)  AS composite,
               ROUND(sp.specialist, 4) AS specialist,
               sp.specialty, bd.breakdown, rk.is_ranked
        FROM comp c
        JOIN sp USING (player_id, league_id)
        JOIN bd USING (player_id, league_id)
        JOIN rk USING (player_id, league_id)
    ),
    ranks AS (
        SELECT player_id, league_id, is_ranked,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY composite  ASC))::numeric * 100, 1) END AS composite_rank,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY specialist ASC))::numeric * 100, 1) END AS specialist_rank,
               CASE WHEN is_ranked THEN public.rating_score(composite,  AVG(composite)  OVER(PARTITION BY is_ranked), STDDEV_POP(composite)  OVER(PARTITION BY is_ranked)) END AS composite_score,
               CASE WHEN is_ranked THEN public.rating_score(specialist, AVG(specialist) OVER(PARTITION BY is_ranked), STDDEV_POP(specialist) OVER(PARTITION BY is_ranked)) END AS specialist_score
        FROM base
    ),
    scoped AS (
        SELECT b.player_id, b.league_id,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric*100,1) END AS pos_pct,
               CASE WHEN p_sport IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.conference ORDER BY b.composite ASC))::numeric*100,1) END AS conf_pct,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.division ORDER BY b.composite ASC))::numeric*100,1) END AS div_pct,
               CASE WHEN p_sport='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, ps.league_id ORDER BY b.composite ASC))::numeric*100,1) END AS league_pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position)) END AS pos_score,
               CASE WHEN p_sport IN ('NFL','NBA') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.conference), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.conference)) END AS conf_score,
               CASE WHEN p_sport='NFL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.division), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.division)) END AS div_score,
               CASE WHEN p_sport='FOOTBALL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, ps.league_id), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, ps.league_id)) END AS league_score
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        WHERE ps.position IS NOT NULL AND b.is_ranked
    )
    SELECT b.player_id, b.league_id,
           CASE WHEN b.is_ranked THEN b.composite END AS composite,
           r.composite_rank, r.composite_score,
           CASE WHEN b.is_ranked THEN b.specialist END AS specialist, r.specialist_rank, r.specialist_score,
           CASE WHEN b.is_ranked THEN b.specialty END AS specialty,
           b.breakdown,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_pct, 'conference', sc.conf_pct,
               'division', sc.div_pct, 'league', sc.league_pct)), '{}'::jsonb) AS scoped_ranks,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_score, 'conference', sc.conf_score,
               'division', sc.div_score, 'league', sc.league_score)), '{}'::jsonb) AS scoped_scores
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
$$;


--
-- Name: apply_rate_siblings(text, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.apply_rate_siblings(p_sport text, p_stats jsonb) RETURNS jsonb
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    result JSONB := p_stats;
    rm     RECORD;
    sk     RECORD;
    denom  NUMERIC;
    v      NUMERIC;
BEGIN
    FOR rm IN
        SELECT suffix, denom_key, formula, unit, round_digits
        FROM public.rate_modes WHERE sport = p_sport ORDER BY mode
    LOOP
        denom := (p_stats->>rm.denom_key)::NUMERIC;
        IF denom IS NOT NULL AND denom > 0 THEN
            FOR sk IN
                SELECT sd.key_name, COALESCE(sd.rate_base, sd.key_name) || rm.suffix AS out_key
                FROM public.stat_definitions sd
                WHERE sd.sport = p_sport AND sd.entity_type = 'player' AND sd.rate_sibling
            LOOP
                IF p_stats ? sk.key_name THEN
                    v := (p_stats->>sk.key_name)::NUMERIC;
                    IF v IS NOT NULL THEN
                        result := result || jsonb_build_object(sk.out_key,
                            CASE rm.formula
                                WHEN 'div_mul' THEN ROUND(v / denom * rm.unit, rm.round_digits)
                                WHEN 'mul_div' THEN ROUND(v * rm.unit / denom, rm.round_digits)
                                WHEN 'div'     THEN ROUND(v / denom, rm.round_digits)
                                WHEN 'mul'     THEN ROUND(v * denom, rm.round_digits)
                            END);
                    END IF;
                END IF;
            END LOOP;
        END IF;
    END LOOP;
    RETURN result;
END;
$$;


--
-- Name: apply_transfer_identity_candidate(text, integer, integer, integer, bigint, bigint, smallint, numeric, jsonb, text, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.apply_transfer_identity_candidate(p_sport text, p_player_id integer, p_old_team_id integer, p_new_team_id integer, p_source_rumor_id bigint, p_source_synthesis_id bigint, p_deterministic_heat smallint, p_deterministic_confidence numeric, p_adjudication jsonb, p_adjudication_raw text, p_adjudication_model_version text, p_adjudication_prompt_version text) RETURNS TABLE(application_id bigint, override_id bigint, status text, reason text)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_threshold public.transfer_identity_thresholds%ROWTYPE;
    v_current_team_id INTEGER;
    v_current_league_id INTEGER;
    v_new_league_id INTEGER;
    v_decision TEXT;
    v_event_type TEXT;
    v_conf NUMERIC;
    v_reason TEXT;
    v_adj_old_team_id INTEGER;
    v_adj_new_team_id INTEGER;
    v_status TEXT;
    v_existing BIGINT;
    v_app_id BIGINT;
    v_override_id BIGINT;
    v_threshold_json JSONB;
BEGIN
    SELECT * INTO v_threshold
    FROM public.transfer_identity_thresholds
    WHERE sport = p_sport;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'missing transfer identity threshold config for sport %', p_sport;
    END IF;

    SELECT team_id, league_id INTO v_current_team_id, v_current_league_id
    FROM public.player_current_identity
    WHERE sport = p_sport AND player_id = p_player_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'player %.% not found in player_current_identity', p_sport, p_player_id;
    END IF;

    SELECT league_id INTO v_new_league_id
    FROM public.teams
    WHERE sport = p_sport AND id = p_new_team_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'new team %.% not found', p_sport, p_new_team_id;
    END IF;

    v_threshold_json := to_jsonb(v_threshold);
    v_decision := p_adjudication->>'decision';
    v_event_type := p_adjudication->>'event_type';
    v_conf := NULLIF(p_adjudication->>'confidence', '')::numeric;
    v_reason := NULLIF(p_adjudication->>'reason', '');
    v_adj_old_team_id := NULLIF(p_adjudication->>'old_team_id', '')::integer;
    v_adj_new_team_id := NULLIF(p_adjudication->>'new_team_id', '')::integer;

    IF p_deterministic_heat < v_threshold.min_heat
       OR p_deterministic_confidence < v_threshold.min_deterministic_confidence THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'deterministic threshold not met');
    ELSIF v_decision = 'reject' THEN
        v_status := 'rejected';
        v_reason := COALESCE(v_reason, 'adjudicator rejected candidate');
    ELSIF v_decision <> 'apply' THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'invalid adjudication decision');
    ELSIF v_event_type IS NULL OR NOT (v_event_type = ANY(v_threshold.allowed_event_types)) THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'unsupported adjudication event_type');
    ELSIF v_adj_new_team_id IS DISTINCT FROM p_new_team_id
       OR v_adj_old_team_id IS DISTINCT FROM p_old_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'adjudication team IDs conflict with deterministic candidate');
    ELSIF v_current_team_id IS DISTINCT FROM p_old_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'current identity changed before apply');
    ELSIF p_old_team_id IS NOT NULL AND p_old_team_id = p_new_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'candidate destination is already current team');
    ELSE
        v_status := 'applied';
        v_reason := COALESCE(v_reason, 'adjudicator approved current identity update');
    END IF;

    IF v_status = 'applied' THEN
        SELECT a_existing.id INTO v_existing
        FROM public.transfer_identity_applications a_existing
        WHERE a_existing.sport = p_sport
          AND a_existing.player_id = p_player_id
          AND COALESCE(a_existing.old_team_id, 0) = COALESCE(p_old_team_id, 0)
          AND a_existing.new_team_id = p_new_team_id
          AND COALESCE(a_existing.source_rumor_id, 0) = COALESCE(p_source_rumor_id, 0)
          AND COALESCE(a_existing.source_synthesis_id, 0) = COALESCE(p_source_synthesis_id, 0)
          AND a_existing.status IN ('applied', 'reverted')
        ORDER BY a_existing.created_at DESC
        LIMIT 1;

        IF v_existing IS NOT NULL THEN
            RETURN QUERY
            SELECT a.id, a.override_id, a.status, 'idempotent: transition already recorded'::text
            FROM public.transfer_identity_applications a
            WHERE a.id = v_existing;
            RETURN;
        END IF;
    END IF;

    INSERT INTO public.transfer_identity_applications (
        sport, player_id, old_team_id, old_league_id, new_team_id, new_league_id,
        source_rumor_id, source_synthesis_id, deterministic_heat, deterministic_confidence,
        threshold_config, adjudication, adjudication_raw, adjudication_model_version,
        adjudication_prompt_version, decision, event_type, adjudication_confidence,
        status, reason, evidence, applied_at
    ) VALUES (
        p_sport, p_player_id, p_old_team_id, v_current_league_id, p_new_team_id, v_new_league_id,
        p_source_rumor_id, p_source_synthesis_id, p_deterministic_heat, p_deterministic_confidence,
        v_threshold_json, COALESCE(p_adjudication, '{}'::jsonb), p_adjudication_raw,
        p_adjudication_model_version, p_adjudication_prompt_version, COALESCE(v_decision, 'failed_closed'),
        v_event_type, v_conf, v_status, v_reason,
        jsonb_build_object('source', 'mistral_adjudication', 'raw', p_adjudication_raw),
        CASE WHEN v_status = 'applied' THEN NOW() ELSE NULL END
    )
    RETURNING id INTO v_app_id;

    IF v_status = 'applied' THEN
        INSERT INTO public.player_current_identity_overrides (
            sport, player_id, team_id, league_id, source, source_rumor_id, source_synthesis_id,
            confidence, reason, evidence, applied_by
        ) VALUES (
            p_sport, p_player_id, p_new_team_id, v_new_league_id, 'applied_transfer',
            p_source_rumor_id, p_source_synthesis_id, v_conf, v_reason,
            jsonb_build_object(
                'application_id', v_app_id,
                'old_team_id', p_old_team_id,
                'old_league_id', v_current_league_id,
                'threshold', v_threshold_json,
                'adjudication', COALESCE(p_adjudication, '{}'::jsonb)
            ),
            'transfer_identity_workflow'
        )
        RETURNING id INTO v_override_id;

        UPDATE public.transfer_identity_applications
        SET override_id = v_override_id
        WHERE id = v_app_id;

        PERFORM public.refresh_sport_autofill(p_sport, 'applied_transfer_identity');
    END IF;

    RETURN QUERY SELECT v_app_id, v_override_id, v_status, v_reason;
END;
$$;


--
-- Name: box_score_coverage_report(text, integer, integer, text[]); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.box_score_coverage_report(p_sport text, p_season integer, p_league_id integer DEFAULT 0, p_required_keys text[] DEFAULT ARRAY[]::text[]) RETURNS TABLE(fixture_count integer, player_row_count integer, team_row_count integer, missing_required_keys text[])
    LANGUAGE sql STABLE
    AS $$
WITH fixtures_in_scope AS (
    SELECT id
    FROM fixtures
    WHERE sport = p_sport
      AND season = p_season
      AND COALESCE(league_id, 0) = p_league_id
),
missing_keys AS (
    SELECT DISTINCT req.key_name
    FROM (
        SELECT unnest(p_required_keys) AS key_name
    ) req
    WHERE EXISTS (
        SELECT 1
        FROM event_box_scores ebs
        JOIN fixtures_in_scope fs ON fs.id = ebs.fixture_id
        WHERE ebs.sport = p_sport
          AND ebs.season = p_season
          AND ebs.league_id = p_league_id
          AND NOT (ebs.stats ? req.key_name)
    )
)
SELECT
    (SELECT COUNT(*)::int FROM fixtures_in_scope) AS fixture_count,
    (
        SELECT COUNT(*)::int
        FROM event_box_scores ebs
        JOIN fixtures_in_scope fs ON fs.id = ebs.fixture_id
        WHERE ebs.sport = p_sport
          AND ebs.season = p_season
          AND ebs.league_id = p_league_id
    ) AS player_row_count,
    (
        SELECT COUNT(*)::int
        FROM event_team_stats ets
        JOIN fixtures_in_scope fs ON fs.id = ets.fixture_id
        WHERE ets.sport = p_sport
          AND ets.season = p_season
          AND ets.league_id = p_league_id
    ) AS team_row_count,
    COALESCE((SELECT array_agg(key_name ORDER BY key_name) FROM missing_keys), ARRAY[]::TEXT[]) AS missing_required_keys;
$$;


--
-- Name: complete_sport_autofill_refresh(text, integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.complete_sport_autofill_refresh(p_sport text, p_total_entities integer, p_reason text DEFAULT NULL::text) RETURNS TABLE(sport text, version bigint, generated_at timestamp with time zone, total_entities integer, status text, reason text)
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    RETURN QUERY
    INSERT INTO public.sport_autofill_versions AS sav (
        sport, version, generated_at, total_entities, status, reason
    )
    VALUES (p_sport, 1, NOW(), p_total_entities, 'ready', p_reason)
    ON CONFLICT ON CONSTRAINT sport_autofill_versions_pkey DO UPDATE SET
        version = sav.version + 1,
        generated_at = NOW(),
        total_entities = EXCLUDED.total_entities,
        status = 'ready',
        reason = EXCLUDED.reason
    RETURNING sav.sport, sav.version, sav.generated_at, sav.total_entities, sav.status, sav.reason;
END;
$$;


--
-- Name: compute_event_starline(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.compute_event_starline(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_updated  INTEGER := 0;
    v_balanced BOOLEAN := FALSE;
BEGIN
    UPDATE event_box_scores
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL);

    DROP TABLE IF EXISTS _starline_dp;
    CREATE TEMP TABLE _starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Every participated event × the shared datapoint definitions (position-gated).
    INSERT INTO _starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM event_box_scores e
    CROSS JOIN LATERAL rating_datapoints(p_sport, e.stats, 'total', e.position) dp
    WHERE e.sport = p_sport AND e.season = p_season
      AND (e.minutes_played IS NULL OR e.minutes_played > 0);

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _starline_dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT event_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY event_id
    ),
    comp_facet AS (
        SELECT event_id, SUM(facet_mean) AS composite
        FROM (
            SELECT event_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY event_id, facet
        ) fm
        GROUP BY event_id
    ),
    composite AS (
        SELECT event_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT event_id, composite FROM comp_facet WHERE     v_balanced
    ),
    spec AS (
        SELECT DISTINCT ON (event_id)
               event_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY event_id, zr DESC
    )
    UPDATE event_box_scores e SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (event_id)
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$$;


--
-- Name: compute_rating(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.compute_rating(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    v_modes   TEXT[] := ARRAY['total'] || COALESCE(
        (SELECT array_agg(mode ORDER BY mode) FROM public.rate_modes WHERE sport = p_sport),
        ARRAY[]::TEXT[]);
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL OR rating_modes IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        IF v_mode = 'total' THEN
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, 'total')
            )
            UPDATE player_stats ps SET
                rating_composite       = b.composite,
                rating_specialist      = b.specialist,
                rating_specialty       = b.specialty,
                rating_composite_rank  = b.composite_rank,
                rating_specialist_rank = b.specialist_rank,
                rating_composite_score = b.composite_score,
                rating_specialist_score= b.specialist_score,
                rating_breakdown       = public.rating_breakdown_without_specialty(b.breakdown),
                rating_scoped_ranks    = b.scoped_ranks,
                rating_scoped_scores   = b.scoped_scores
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
            GET DIAGNOSTICS v_updated = ROW_COUNT;
        ELSE
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, v_mode)
            )
            UPDATE player_stats ps SET
                rating_modes = COALESCE(ps.rating_modes, '{}'::jsonb) || jsonb_build_object(
                    v_mode,
                    public.rating_mode_peak_payload(jsonb_build_object(
                        'composite',        b.composite,
                        'composite_rank',   b.composite_rank,
                        'composite_score',  b.composite_score,
                        'peak',             b.specialist,
                        'peak_rank',        b.specialist_rank,
                        'peak_score',       b.specialist_score,
                        'peak_label',       b.specialty,
                        'breakdown',        b.breakdown,
                        'scoped_ranks',     b.scoped_ranks,
                        'scoped_scores',    b.scoped_scores
                    )))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$$;


--
-- Name: compute_team_event_starline(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.compute_team_event_starline(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE event_team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL);

    DROP TABLE IF EXISTS _team_starline_dp;
    CREATE TEMP TABLE _team_starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER
    ) ON COMMIT DROP;

    INSERT INTO _team_starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign
    FROM event_team_stats e
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, e.stats) dp
    WHERE e.sport = p_sport AND e.season = p_season AND e.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.in_comp, d.in_spec, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_starline_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT event_id, SUM(sign * zr) AS composite FROM z WHERE in_comp GROUP BY event_id
    ),
    spec AS (
        SELECT DISTINCT ON (event_id) event_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec ORDER BY event_id, zr DESC
    )
    UPDATE event_team_stats e SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c JOIN spec s USING (event_id)
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$$;


--
-- Name: compute_team_rating(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.compute_team_rating(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL, rating_breakdown = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

    DROP TABLE IF EXISTS _team_dp;
    CREATE TEMP TABLE _team_dp (
        team_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    INSERT INTO _team_dp
    SELECT ts.team_id, COALESCE(ts.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM team_stats ts
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, ts.stats) dp
    WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.in_comp, d.in_spec, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT team_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY team_id, league_id
    ),
    spec AS (
        SELECT DISTINCT ON (team_id, league_id)
               team_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY team_id, league_id, zr DESC
    )
    UPDATE team_stats ts SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (team_id, league_id)
    WHERE ts.team_id = c.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    scored AS (
        SELECT team_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id AND ts.rating_composite IS NOT NULL;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank,
               public.rating_score(rating_composite,  AVG(rating_composite)  OVER(), STDDEV_POP(rating_composite)  OVER()) AS cscore,
               public.rating_score(rating_specialist, AVG(rating_specialist) OVER(), STDDEV_POP(rating_specialist) OVER()) AS sscore
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank,
                             rating_composite_score = r.cscore, rating_specialist_score = r.sscore
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$$;


--
-- Name: compute_transfer_heat(integer, integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.compute_transfer_heat(p_team_id integer, p_player_id integer, p_sport text, OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]) RETURNS record
    LANGUAGE sql STABLE
    AS $$
    WITH corpus AS (
        -- News articles linking BOTH the team and the player, in PROXIMITY and
        -- scrub-vetted on both sides. Wave 5 keeps NULL bucket rows during the
        -- transition, but confirmed non-transfer articles no longer contribute
        -- to transfer heat.
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at > NOW() - INTERVAL '14 days'
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= 50)
    ),
    agg AS (
        SELECT
            count(DISTINCT c.src) AS distinct_sources,
            count(*) FILTER (WHERE c.ts > NOW() - INTERVAL '3 days') AS recent3,
            count(*) AS total,
            max(c.ts) AS newest,
            COALESCE(MAX(st.weight), 0.3) AS tier_weight
        FROM corpus c
        LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = c.kind
    ),
    calc AS (
        SELECT *,
            EXTRACT(EPOCH FROM (NOW() - newest)) / 3600.0 AS age_hours,
            LEAST(1.0, distinct_sources::numeric / 5.0)   AS volume,
            recent3::numeric / GREATEST(total, 1)         AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *, exp(-age_hours / 72.0) AS recency FROM calc
    )
    SELECT
        CASE WHEN total = 0 THEN NULL
             ELSE GREATEST(0, LEAST(100,
                    round(100 * tier_weight * recency * (0.6 * volume + 0.4 * recent_frac))))::smallint
        END,
        CASE WHEN total = 0 THEN '{}'::jsonb
             ELSE jsonb_build_object(
                'distinct_sources', distinct_sources,
                'recent_3d', recent3,
                'total_14d', total,
                'newest_age_hours', round(age_hours::numeric, 1),
                'tier_weight', tier_weight,
                'volume', round(volume::numeric, 3),
                'recency', round(recency::numeric, 3),
                'recent_frac', round(recent_frac::numeric, 3))
        END,
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}')
    FROM fin;
$$;


--
-- Name: datapoints_block(text, jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.datapoints_block(p_sport text, p_stats jsonb, p_pct jsonb, p_scoped jsonb) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key', sd.key_name, 'label', sd.display_name,
               'value', COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',   (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', sd.category, 'sort', sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'player' AND sd.key_name = k.key
    WHERE NOT EXISTS (SELECT 1 FROM public.rate_modes rm
                      WHERE rm.sport = upper(p_sport) AND right(k.key, length(rm.suffix)) = rm.suffix);
$$;


--
-- Name: detect_team_change(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.detect_team_change() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: FUNCTION detect_team_change(); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.detect_team_change() IS 'Trigger function that detects when a player appears in box scores for a different team and queues metadata refresh.';


--
-- Name: enqueue_derive_on_vetted(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.enqueue_derive_on_vetted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_count  integer;
    v_fp     text;
    v_ver    text;
    v_stages text[] := ARRAY['narratives', 'vibe'];
BEGIN
    -- Membership fingerprint over this entity's vetted, in-lookback (72h) links. Doubles as the
    -- mig-103 freshness gate: a stale backlog primary with no fresh vetted corpus yields count=0
    -- and enqueues nothing. One link row per (article, entity) by PK, so no DISTINCT needed;
    -- ORDER BY makes the aggregate deterministic.
    SELECT count(*),
           md5(string_agg(nae.article_id::text, ',' ORDER BY nae.article_id))
      INTO v_count, v_fp
      FROM public.news_article_entities nae
      JOIN public.news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_fp;

    IF NEW.entity_type = 'team' THEN
        v_stages := ARRAY['transfers', 'narratives', 'vibe'];
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT st, NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW()
      FROM unnest(v_stages) AS st
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Redundant with the mig-133 statement-level pipeline_work notify trigger, but kept for the
    -- minimal diff: pg_notify de-dups identical (channel,payload) within a txn, so this costs
    -- nothing extra.
    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$;


--
-- Name: fail_sport_autofill_refresh(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fail_sport_autofill_refresh(p_sport text, p_reason text) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    INSERT INTO public.sport_autofill_versions (sport, status, reason, generated_at)
    VALUES (p_sport, 'failed', p_reason, NOW())
    ON CONFLICT (sport) DO UPDATE SET
        status = 'failed',
        reason = EXCLUDED.reason,
        generated_at = NOW();
END;
$$;


--
-- Name: fantasy_block(jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fantasy_block(p_stats jsonb, p_pct jsonb, p_scoped jsonb) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
    SELECT CASE WHEN p_stats ? 'fantasy_points' THEN jsonb_object_agg(m.mode, m.blk) END
    FROM (
        SELECT v.mode,
            CASE WHEN p_stats ? v.key THEN jsonb_build_object(
                'points', (p_stats->>v.key)::numeric,
                'rank',   (p_pct->>v.key)::numeric,
                'scoped_ranks', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>v.key)::numeric)
                                  FILTER (WHERE s.keys ? v.key), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                )
            ) END AS blk
        FROM (
            SELECT 'default'::text AS mode, 'fantasy_points'::text AS key
            UNION
            SELECT DISTINCT rm.mode, 'fantasy_points' || rm.suffix FROM public.rate_modes rm
        ) v
    ) m
    WHERE m.blk IS NOT NULL;
$$;


--
-- Name: finalize_fixture(integer, boolean); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.finalize_fixture(p_fixture_id integer, p_recompute boolean DEFAULT true) RETURNS TABLE(players_updated integer, teams_updated integer)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_sport TEXT;
    v_season INTEGER;
    v_league_id INTEGER;
    v_home_team_id INTEGER;
    v_away_team_id INTEGER;
    v_home_score INTEGER;
    v_away_score INTEGER;
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    -- Look up fixture details
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    -- Reaggregate impacted player season rows from event_box_scores (per-fixture; cheap)
    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NBA',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'NBA',
            v_season,
            v_league_id,
            COALESCE(nba.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();

    ELSIF v_sport = 'NFL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NFL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'NFL',
            v_season,
            v_league_id,
            COALESCE(nfl.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();

    ELSIF v_sport = 'FOOTBALL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'FOOTBALL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            COALESCE(
                NULLIF((array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE NULLIF(e.position, '') IS NOT NULL))[1], ''),
                (SELECT NULLIF(pl.meta->>'position_abbreviation', '') FROM players pl WHERE pl.id = e.player_id AND pl.sport = v_sport)
            ) AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(NULLIF(EXCLUDED.position, ''), player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id,
            'FOOTBALL',
            v_season,
            v_league_id,
            COALESCE(football.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION
            SELECT DISTINCT home_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
            UNION
            SELECT DISTINCT away_team_id AS team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            updated_at = NOW();
    END IF;

    -- Whole-season recompute — gated. Default TRUE = today's live per-fixture
    -- freshness. Bulk historical backfill passes FALSE and runs recompute_season
    -- once at the end (O(M) instead of O(M^2)). Concluded seasons stay FROZEN
    -- until a deliberate recompute. (Cross-season all-time ranks run separately
    -- on the maintenance ticker.)
    IF p_recompute THEN
        SELECT rs.players_updated, rs.teams_updated
        INTO v_players, v_teams
        FROM recompute_season(v_sport, v_season) rs;
    END IF;

    -- Look up final score for each team from event_team_stats.
    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    -- Mark the fixture as seeded (always — keeps get_pending resume state correct
    -- even in deferred mode).
    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$$;


--
-- Name: get_metadata_queue_batch(integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.get_metadata_queue_batch(batch_size integer DEFAULT 10, p_sport text DEFAULT NULL::text) RETURNS TABLE(id integer, player_id integer, sport text, season integer, reason text, priority integer)
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: get_metadata_queue_status(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.get_metadata_queue_status(p_sport text DEFAULT NULL::text) RETURNS TABLE(total_pending bigint, high_priority bigint, normal_priority bigint, failed_items bigint, oldest_request timestamp with time zone)
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: get_pending_fixtures(text, integer, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.get_pending_fixtures(p_sport text DEFAULT NULL::text, p_limit integer DEFAULT 50, p_max_retries integer DEFAULT 3) RETURNS TABLE(id integer, sport text, league_id integer, season integer, home_team_id integer, away_team_id integer, start_time timestamp with time zone, seed_delay_hours integer, seed_attempts integer, external_id integer)
    LANGUAGE sql STABLE
    AS $$
    SELECT f.id, f.sport, f.league_id, f.season,
           f.home_team_id, f.away_team_id, f.start_time,
           f.seed_delay_hours, f.seed_attempts, f.external_id
    FROM fixtures f
    WHERE (f.status = 'scheduled' OR f.status = 'completed')
      AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
      AND f.seed_attempts < p_max_retries
      AND (p_sport IS NULL OR f.sport = p_sport)
    ORDER BY f.start_time ASC
    LIMIT p_limit;
$$;


--
-- Name: mark_fixture_seeded(integer, integer, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.mark_fixture_seeded(p_fixture_id integer, p_home_score integer DEFAULT NULL::integer, p_away_score integer DEFAULT NULL::integer) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    UPDATE fixtures SET
        status = 'seeded', seeded_at = NOW(),
        home_score = COALESCE(p_home_score, home_score),
        away_score = COALESCE(p_away_score, away_score),
        last_seed_error = NULL,
        last_incomplete_reason = NULL,
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$;


--
-- Name: mark_metadata_processed(integer, boolean, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.mark_metadata_processed(p_queue_id integer, p_success boolean DEFAULT true, p_error_message text DEFAULT NULL::text) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    UPDATE metadata_refresh_queue
    SET 
        processed_at = NOW(),
        error_message = CASE WHEN p_success THEN NULL ELSE p_error_message END,
        retry_count = CASE WHEN p_success THEN retry_count ELSE retry_count + 1 END
    WHERE id = p_queue_id;
END;
$$;


--
-- Name: mark_momentum_refresh_from_event_rating(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.mark_momentum_refresh_from_event_rating() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.rating_composite_pct IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' OR OLD.rating_composite_pct IS DISTINCT FROM NEW.rating_composite_pct THEN
        PERFORM public.mark_momentum_refresh_needed(NEW.sport, 'rating');
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: mark_momentum_refresh_from_vibe(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.mark_momentum_refresh_from_vibe() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NEW.sentiment IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' OR OLD.sentiment IS DISTINCT FROM NEW.sentiment THEN
        PERFORM public.mark_momentum_refresh_needed(NEW.sport, 'vibe');
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: mark_momentum_refresh_needed(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.mark_momentum_refresh_needed(p_sport text, p_reason text DEFAULT NULL::text) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_sport TEXT := upper(NULLIF(p_sport, ''));
BEGIN
    IF v_sport IS NULL THEN
        RETURN;
    END IF;

    INSERT INTO public.momentum_refresh_needed (sport, reason, first_marked_at, last_marked_at)
    VALUES (v_sport, p_reason, NOW(), NOW())
    ON CONFLICT (sport) DO UPDATE SET
        reason = COALESCE(EXCLUDED.reason, public.momentum_refresh_needed.reason),
        last_marked_at = NOW();

    PERFORM pg_notify('momentum_refresh_ready', v_sport);
END;
$$;


--
-- Name: narrative_persons_track_affiliation(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.narrative_persons_track_affiliation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.team_id IS NOT DISTINCT FROM OLD.team_id THEN
        RETURN NEW;
    END IF;
    UPDATE narrative_person_affiliations
       SET is_current = FALSE, valid_until = now()
     WHERE person_id = NEW.id
       AND entity_type = 'team'
       AND is_current
       AND (NEW.team_id IS NULL OR entity_id <> NEW.team_id);
    IF NEW.team_id IS NOT NULL THEN
        INSERT INTO narrative_person_affiliations
            (person_id, sport, entity_type, entity_id, role, season, evidence)
        SELECT NEW.id, NEW.sport, 'team', NEW.team_id, NEW.kind, s.current_season,
               jsonb_build_object('source', 'narrative_persons.team_id')
        FROM sports s WHERE s.id = NEW.sport
        ON CONFLICT (person_id, entity_type, entity_id, role) WHERE is_current
        DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;


--
-- Name: notify_percentile_changed(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.notify_percentile_changed() RETURNS trigger
    LANGUAGE plpgsql
    AS $_$
DECLARE
    stat_key TEXT;
    new_val NUMERIC;
    old_val NUMERIC;
    entity_type TEXT;
    entity_id INTEGER;
BEGIN
    IF TG_TABLE_NAME = 'player_stats' THEN
        entity_type := 'player';
        entity_id := NEW.player_id;
    ELSIF TG_TABLE_NAME = 'team_stats' THEN
        entity_type := 'team';
        entity_id := NEW.team_id;
    ELSE
        RETURN NEW;
    END IF;

    IF OLD.percentiles IS NOT DISTINCT FROM NEW.percentiles THEN
        RETURN NEW;
    END IF;

    FOR stat_key IN
        SELECT key FROM jsonb_each(NEW.percentiles)
        WHERE key NOT LIKE '\_%'
          -- skip per-rate siblings; the suffix family lives in rate_modes
          AND NOT EXISTS (SELECT 1 FROM public.rate_modes rm WHERE key ~ (rm.suffix || '$'))
          AND jsonb_typeof(value) = 'number'
    LOOP
        new_val := (NEW.percentiles ->> stat_key)::numeric;
        old_val := COALESCE((OLD.percentiles ->> stat_key)::numeric, 0);

        IF (new_val >= 90 AND old_val < 90)
        OR (new_val >= 95 AND old_val < 95)
        OR (new_val >= 99 AND old_val < 99)
        OR (new_val - old_val >= 10) THEN
            PERFORM pg_notify('percentile_changed', json_build_object(
                'entity_type', entity_type,
                'entity_id', entity_id,
                'sport', NEW.sport,
                'season', NEW.season,
                'stat_key', stat_key,
                'old_percentile', old_val,
                'new_percentile', new_val,
                'ts', extract(epoch from now())::bigint
            )::text);
        END IF;
    END LOOP;

    RETURN NEW;
END;
$_$;


--
-- Name: notify_pipeline_work_ready(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.notify_pipeline_work_ready() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- `new_rows` is the statement's transition table (REFERENCING NEW TABLE below).
    IF EXISTS (
        SELECT 1 FROM new_rows
        WHERE status = 'pending' AND available_at <= NOW()
    ) THEN
        PERFORM pg_notify('pipeline_work_ready', '');
    END IF;
    RETURN NULL;  -- AFTER STATEMENT trigger: return value is ignored.
END;
$$;


--
-- Name: position_group(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.position_group(p_sport text, p_position text) RETURNS text
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA' THEN 'ALL'  -- one Fantasy-mode template for all NBA players
        WHEN 'NFL' THEN CASE p_position
            WHEN 'Quarterback' THEN 'quarterback'
            WHEN 'QB'          THEN 'quarterback'
            WHEN 'Running Back' THEN 'running-back'
            WHEN 'Fullback'     THEN 'running-back'
            WHEN 'RB'           THEN 'running-back'
            WHEN 'FB'           THEN 'running-back'
            WHEN 'HB'           THEN 'running-back'
            WHEN 'Wide Receiver' THEN 'receiver'
            WHEN 'Tight End'     THEN 'receiver'
            WHEN 'WR'            THEN 'receiver'
            WHEN 'TE'            THEN 'receiver'
            ELSE NULL
        END
        WHEN 'FOOTBALL' THEN CASE p_position
            WHEN 'Goalkeeper' THEN 'goalkeeper'
            WHEN 'Defender'   THEN 'defender'
            WHEN 'Midfielder' THEN 'midfielder'
            WHEN 'Attacker'   THEN 'attacker'
            ELSE NULL
        END
        ELSE NULL
    END;
$$;


--
-- Name: rating_breakdown_without_specialty(jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.rating_breakdown_without_specialty(p_breakdown jsonb) RETURNS jsonb
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE
        WHEN p_breakdown IS NULL THEN NULL
        ELSE COALESCE((
            SELECT jsonb_agg(e - 'is_specialty' ORDER BY ord)
            FROM jsonb_array_elements(p_breakdown) WITH ORDINALITY AS x(e, ord)
        ), '[]'::jsonb)
    END;
$$;


--
-- Name: rating_datapoints(text, jsonb, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL::text) RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NBA' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all', 'pts'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all', 'reb'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all', 'ast'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all', 'stl'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all', 'blk'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all', 'fg3m'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all', NULL),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all', 'tov'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all', 'pf'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all', 'fta')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NBA'

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'FOOTBALL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals',           'out'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists',         'out'),
        -- Shooting: blended shots + on-target (rate-aware; rate_base NULL = handled inline).
        ('Shooting',        NULLIF(p_stats->>'shots_on_target','')::numeric,  TRUE, TRUE,   1, 'all', 'shots_on_target', 'out'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate', 'out'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes',      'out'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success','out'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        FALSE, FALSE, 1, 'all', 'duels_won',       'out'),
        ('Tackling',        round(COALESCE(NULLIF(p_stats->>('tackles' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'tackles','')::numeric)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Interceptions',   round(COALESCE(NULLIF(p_stats->>('interceptions' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'interceptions','')::numeric)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       FALSE, FALSE, 1, 'all', 'clearances',      'out'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           FALSE, FALSE, 1, 'all', 'blocks',          'out'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    FALSE, FALSE, 1, 'all', 'ball_recovery',   'out'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      FALSE, FALSE, 1, 'all', 'fouls_drawn',     'out'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL,              'out'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost','out'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            FALSE, FALSE, 1, 'all', 'saves',           'gk'),
        ('Goals Prevented', NULLIF(p_stats->>'saves','')::numeric
                            - (COALESCE(NULLIF(p_stats->>'saves','')::numeric,0) + COALESCE(NULLIF(p_stats->>'goals_conceded','')::numeric,0))
                              * NULLIF(p_stats->>'league_avg_save_pct','')::numeric / 100.0,
                                                                              TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Distribution',       NULLIF(p_stats->>'pass_accuracy','')::numeric,      TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Long-Ball Accuracy', NULLIF(p_stats->>'long_ball_accuracy','')::numeric, TRUE, TRUE, 1, 'all', NULL, 'gk')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base, pos_class)
    WHERE p_sport = 'FOOTBALL'
      AND (CASE WHEN p_position = 'Goalkeeper' THEN v.pos_class = 'gk'
                ELSE v.pos_class = 'out' END)

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NFL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Air Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>'interception_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_yards' || rs.suffix))::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>('receiving_yards' || rs.suffix))::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>('kick_return_yards' || rs.suffix))::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_returner_return_yards' || rs.suffix))::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_yards' || rs.suffix))::numeric,(p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>('interception_yards' || rs.suffix))::numeric,(p_stats->>'interception_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Ground Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'rushing_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('rushing_yards' || rs.suffix))::numeric,(p_stats->>'rushing_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', 'rushing_yards'),
        ('Points Responsible For',
            CASE WHEN p_rate_mode = 'total' THEN
                  6 * (
                      COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>'extra_points_made')::numeric,0)
            ELSE
                  6 * (
                      COALESCE((p_stats->>('passing_touchdowns' || rs.suffix))::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('rushing_touchdowns' || rs.suffix))::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('receiving_touchdowns' || rs.suffix))::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('kick_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('punt_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('interception_touchdowns' || rs.suffix))::numeric,(p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('fumbles_touchdowns' || rs.suffix))::numeric,(p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>('field_goals_made' || rs.suffix))::numeric,(p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>('extra_points_made' || rs.suffix))::numeric,(p_stats->>'extra_points_made')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>('fumbles_lost' || rs.suffix))::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss',
            CASE WHEN p_rate_mode = 'total' THEN
                  GREATEST(
                      COALESCE((p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>'defensive_sacks')::numeric,0)
                  )
            ELSE
                  GREATEST(
                      COALESCE((p_stats->>('tackles_for_loss' || rs.suffix))::numeric,(p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>('defensive_sacks' || rs.suffix))::numeric,(p_stats->>'defensive_sacks')::numeric,0)
                  )
            END,                                                                  TRUE, TRUE,   1, 'defense', NULL),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NFL';
$$;


--
-- Name: rating_datapoints_team(text, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.rating_datapoints_team(p_sport text, p_stats jsonb) RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
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
        ('Points Allowed',     NULLIF(p_stats->>'pts_allowed','')::numeric, TRUE,  FALSE, -1, 'defense'),
        ('Defensive Rebounds', NULLIF(p_stats->>'dreb','')::numeric,        FALSE, FALSE,  1, 'defense'),
        ('Opp FG%',            NULLIF(p_stats->>'def_fg_pct','')::numeric,  FALSE, FALSE, -1, 'defense'),
        ('Opp 3PT%',           NULLIF(p_stats->>'def_fg3_pct','')::numeric, FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'
    UNION ALL
    SELECT * FROM (VALUES
        ('Points Scored',      NULLIF(p_stats->>'points_for','')::numeric,                 TRUE,  TRUE,   1, 'offense'),
        ('Total Yards',        NULLIF(p_stats->>'total_yards','')::numeric,                TRUE,  TRUE,   1, 'offense'),
        ('Giveaways',          NULLIF(p_stats->>'turnovers','')::numeric,                  TRUE,  FALSE, -1, 'offense'),
        ('Touchdowns',         COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                             + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0),      FALSE, FALSE,  1, 'offense'),
        ('First Downs',        NULLIF(p_stats->>'first_downs','')::numeric,                FALSE, FALSE,  1, 'offense'),
        ('Field Goals',        NULLIF(p_stats->>'field_goals_made','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Red Zone %',         NULLIF(p_stats->>'red_zone_pct','')::numeric,               FALSE, FALSE,  1, 'offense'),
        ('Third Down %',       NULLIF(p_stats->>'third_down_pct','')::numeric,             FALSE, FALSE,  1, 'offense'),
        ('Penalty Yards For',  NULLIF(p_stats->>'penalty_yards_drawn','')::numeric,        TRUE,  FALSE,  1, 'offense'),
        ('Tackling',           NULLIF(p_stats->>'total_tackles','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('Sacks',              NULLIF(p_stats->>'defensive_sacks','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Pass Defense',       NULLIF(p_stats->>'passes_defended','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',      NULLIF(p_stats->>'defensive_interceptions','')::numeric,    TRUE,  TRUE,   1, 'defense'),
        ('Points Allowed',     NULLIF(p_stats->>'points_against','')::numeric,             TRUE,  FALSE, -1, 'defense'),
        ('Yards Allowed',      NULLIF(p_stats->>'yards_allowed','')::numeric,              TRUE,  FALSE, -1, 'defense'),
        ('Penalty Yards Against', NULLIF(p_stats->>'penalty_yards','')::numeric,           TRUE,  FALSE, -1, 'defense'),
        ('Tackles For Loss',   NULLIF(p_stats->>'tackles_for_loss','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Takeaways',          NULLIF(p_stats->>'takeaways','')::numeric,                  FALSE, FALSE,  1, 'defense'),
        ('Red Zone Def %',     NULLIF(p_stats->>'red_zone_def_pct','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Third Down Def %',   NULLIF(p_stats->>'third_down_def_pct','')::numeric,         FALSE, FALSE, -1, 'defense'),
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'
    UNION ALL
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'big_chances_created','')::numeric,      TRUE,  TRUE,   1, 'offense'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                TRUE,  FALSE, -1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Fouls Won',            NULLIF(p_stats->>'fouls_drawn','')::numeric,              FALSE, FALSE,  1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        ('Progression',          COALESCE(NULLIF(p_stats->>'passes_final_third','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'successful_dribbles','')::numeric,0),  TRUE, FALSE,  1, 'offense'),
        ('Tackling',             round(NULLIF(p_stats->>'tackles','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        round(NULLIF(p_stats->>'interceptions','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              FALSE, FALSE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE, FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      TRUE, FALSE, -1, 'defense'),
        ('Goals Against',        NULLIF(p_stats->>'goals_against','')::numeric,           TRUE, FALSE, -1, 'defense'),
        ('Fouls Committed',      NULLIF(p_stats->>'fouls_committed','')::numeric,          FALSE, FALSE, -1, 'defense'),
        ('Cards',                COALESCE(NULLIF(p_stats->>'yellow_cards_total','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'red_cards_total','')::numeric,0),   TRUE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$$;


--
-- Name: rating_mode_peak_payload(jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.rating_mode_peak_payload(p_mode jsonb) RETURNS jsonb
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE
        WHEN p_mode IS NULL THEN NULL
        ELSE (p_mode - 'specialist' - 'specialist_rank' - 'specialist_score' - 'specialty' - 'breakdown')
             || jsonb_strip_nulls(jsonb_build_object(
                    'peak',       COALESCE(p_mode->'peak',       p_mode->'specialist'),
                    'peak_rank',  COALESCE(p_mode->'peak_rank',  p_mode->'specialist_rank'),
                    'peak_score', COALESCE(p_mode->'peak_score', p_mode->'specialist_score'),
                    'peak_label', COALESCE(p_mode->'peak_label', p_mode->'specialty'),
                    'breakdown',  public.rating_breakdown_without_specialty(p_mode->'breakdown')
                ))
    END;
$$;


--
-- Name: rating_score(numeric, numeric, numeric); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.rating_score(p_value numeric, p_mean numeric, p_sd numeric) RETURNS numeric
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE WHEN p_value IS NULL OR p_sd IS NULL OR p_sd = 0 THEN NULL
                ELSE ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (p_value - p_mean) / p_sd))::numeric, 1) END;
$$;


--
-- Name: recalculate_alltime_ranks(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recalculate_alltime_ranks(p_sport text, p_season integer DEFAULT NULL::integer) RETURNS TABLE(players_updated integer, teams_updated integer)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    -- Null out rows that lost their composite.
    UPDATE player_stats SET season_composite_rank_alltime = NULL, season_composite_rank_alltime_absolute = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND (season_composite_rank_alltime IS NOT NULL OR season_composite_rank_alltime_absolute IS NOT NULL)
          AND (p_season IS NULL OR season = p_season);
    UPDATE team_stats SET season_composite_rank_alltime = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND season_composite_rank_alltime IS NOT NULL
          AND (p_season IS NULL OR season = p_season);

    -- Players: position-partitioned all-time rank (Layer 4)
    UPDATE player_stats ps SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT player_id, season, league_id,
                   ROUND((percent_rank() OVER (
                       PARTITION BY COALESCE(position, 'Unknown')
                       ORDER BY season_composite_score ASC
                   ))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = r.season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ps.season = p_season);
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Players: cross-position absolute all-time rank (Layer 4 absolute, NEW)
    UPDATE player_stats ps SET season_composite_rank_alltime_absolute = r.rnk
        FROM (
            SELECT player_id, season, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = r.season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ps.season = p_season);

    -- Teams: sport-wide all-time rank (unchanged from mig 024)
    UPDATE team_stats ts SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT team_id, season, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = r.season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ts.season = p_season);
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$;


--
-- Name: recalculate_event_percentiles(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recalculate_event_percentiles(p_sport text, p_season integer) RETURNS TABLE(player_events_updated integer, team_events_updated integer)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_player_events INTEGER := 0;
    v_team_events INTEGER := 0;
    v_window INTEGER := public.season_bridge_window(p_sport);
BEGIN
    UPDATE event_box_scores SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;
    UPDATE event_team_stats SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;

    -- PLAYER EVENTS (Layer 1)
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'player' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, COALESCE(e.position, ps.position, 'Unknown') AS position,
               ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_box_scores e CROSS JOIN eligible ek
        LEFT JOIN player_stats ps ON ps.player_id = e.player_id AND ps.sport = e.sport
              AND ps.season = e.season AND COALESCE(ps.league_id, 0) = COALESCE(e.league_id, 0)
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, position, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, MAX(position) AS position_group, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank() OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_box_scores ebs
        SET percentiles = nm.pct_only || jsonb_build_object('_position_group', nm.position_group, '_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ebs.id = nm.event_id;
    GET DIAGNOSTICS v_player_events = ROW_COUNT;

    -- TEAM EVENTS (Layer 1)
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'team' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_team_stats e CROSS JOIN eligible ek
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, pct_only, sample_size,
               ROUND((percent_rank() OVER (ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_team_stats ets
        SET percentiles = nm.pct_only || jsonb_build_object('_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ets.id = nm.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- Layer 2: season_composite_score
    UPDATE player_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;
    UPDATE team_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;

    UPDATE player_stats ps SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ps2.player_id, ps2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM player_stats ps2
            CROSS JOIN LATERAL jsonb_each(ps2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ps2.sport AND sd.entity_type = 'player' AND sd.key_name = p.key
            WHERE ps2.sport = p_sport AND ps2.season = p_season
              AND ps2.percentiles IS NOT NULL AND ps2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ps2.player_id, ps2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ps.player_id = sub.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(sub.league_id, 0);

    UPDATE team_stats ts SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ts2.team_id, ts2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM team_stats ts2
            CROSS JOIN LATERAL jsonb_each(ts2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ts2.sport AND sd.entity_type = 'team' AND sd.key_name = p.key
            WHERE ts2.sport = p_sport AND ts2.season = p_season
              AND ts2.percentiles IS NOT NULL AND ts2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ts2.team_id, ts2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ts.team_id = sub.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(sub.league_id, 0);

    -- Layer 2.5: Cold-start guard (migration 025)
    WITH cold_start_players AS (
        SELECT ps.player_id, ps.league_id,
            (SELECT COUNT(*) FROM event_box_scores e
             WHERE e.player_id = ps.player_id AND e.sport = ps.sport AND e.season = ps.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM player_stats prev
                 WHERE prev.player_id = ps.player_id AND prev.sport = ps.sport
                   AND prev.season = ps.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ps.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM player_stats prev
                 WHERE prev.sport = ps.sport AND prev.season = ps.season - 1
                   AND COALESCE(prev.position, 'Unknown') = COALESCE(ps.position, 'Unknown')
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ps.season_composite_score AS current_score
        FROM player_stats ps
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.season_composite_score IS NOT NULL
    )
    UPDATE player_stats ps SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_players cs
    WHERE ps.player_id = cs.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    WITH cold_start_teams AS (
        SELECT ts.team_id, ts.league_id,
            (SELECT COUNT(*) FROM event_team_stats e
             WHERE e.team_id = ts.team_id AND e.sport = ts.sport AND e.season = ts.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM team_stats prev
                 WHERE prev.team_id = ts.team_id AND prev.sport = ts.sport
                   AND prev.season = ts.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ts.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM team_stats prev
                 WHERE prev.sport = ts.sport AND prev.season = ts.season - 1
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ts.season_composite_score AS current_score
        FROM team_stats ts
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.season_composite_score IS NOT NULL
    )
    UPDATE team_stats ts SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_teams cs
    WHERE ts.team_id = cs.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    -- Layer 3: season_composite_rank (within-position for players)
    UPDATE player_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;
    UPDATE team_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (PARTITION BY COALESCE(position, 'Unknown') ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    UPDATE team_stats ts SET season_composite_rank = r.rnk
        FROM (
            SELECT team_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0);

    -- ============================================================
    -- Layer 3 ABSOLUTE: cross-position rank for players (NEW, mig 026)
    -- No PARTITION BY position — ranks players across ALL positions in
    -- the (sport, season). Teams have no equivalent (already sport-wide).
    -- ============================================================
    UPDATE player_stats SET season_composite_rank_absolute = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank_absolute IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank_absolute = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$$;


--
-- Name: recalculate_event_rating_pct(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recalculate_event_rating_pct(p_sport text, p_season integer) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Player events: clear stale pct, then percent_rank the live z's.
    UPDATE event_box_scores
       SET rating_composite_pct = NULL, rating_specialist_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite_pct IS NOT NULL OR rating_specialist_pct IS NOT NULL);

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS cpct,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS spct
        FROM event_box_scores
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE event_box_scores e
       SET rating_composite_pct  = r.cpct,
           rating_specialist_pct = r.spct
    FROM ranked r WHERE e.id = r.id;

    -- Team events (same, no position dimension to begin with — already flat).
    UPDATE event_team_stats
       SET rating_composite_pct = NULL, rating_specialist_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite_pct IS NOT NULL OR rating_specialist_pct IS NOT NULL);

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS cpct,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS spct
        FROM event_team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE event_team_stats e
       SET rating_composite_pct  = r.cpct,
           rating_specialist_pct = r.spct
    FROM ranked r WHERE e.id = r.id;
END;
$$;


--
-- Name: recalculate_percentiles(text, integer, text[]); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recalculate_percentiles(p_sport text, p_season integer, p_inverse_stats text[] DEFAULT ARRAY[]::text[]) RETURNS TABLE(players_updated integer, teams_updated integer)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (sport-wide, partitioned by ps.position) — UNCHANGED
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ps.player_id, COALESCE(ps.position, 'Unknown') AS position,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
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

    -- Team percentiles (no position partitioning) — UNCHANGED
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

    -- Player scoped percentiles — MULTI-SCOPE nested {scope: {key: pct}}
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_scope AS (
        SELECT ps.player_id, ps.league_id, sc.scope_name, sc.cohort_key
        FROM player_stats ps
        LEFT JOIN teams t ON t.id = ps.team_id AND t.sport = ps.sport
        CROSS JOIN LATERAL (VALUES
            ('position',   CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN COALESCE(ps.position,'Unknown') END),
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport='NFL' THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.division,'Unknown') END),
            ('league',     CASE WHEN p_sport='FOOTBALL' THEN COALESCE(ps.position,'Unknown')||'|'||ps.league_id::text END),
            -- positionless baseline for the "All" option on every pizza (the template's
            -- base pct is within-position, so 'all' must carry the sport-wide rank).
            ('all',        'ALL')
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT psc.player_id, psc.league_id, psc.scope_name, psc.cohort_key,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_scope psc
        JOIN player_stats ps ON ps.player_id=psc.player_id AND ps.league_id=psc.league_id
                            AND ps.sport=p_sport AND ps.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT player_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY player_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT player_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY player_id, league_id
    )
    UPDATE player_stats ps SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ps.player_id=agg.player_id AND ps.league_id=agg.league_id
                          AND ps.sport=p_sport AND ps.season=p_season;

    -- Team scoped percentiles — MULTI-SCOPE nested {scope: {key: pct}}
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    team_scope AS (
        SELECT ts.team_id, ts.league_id, sc.scope_name, sc.cohort_key
        FROM team_stats ts
        JOIN teams t ON t.id = ts.team_id AND t.sport = ts.sport
        CROSS JOIN LATERAL (VALUES
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.division,'Unknown') END),
            ('league',     ts.league_id::text)
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ts.sport = p_sport AND ts.season = p_season
    ),
    expanded AS (
        SELECT tsc.team_id, tsc.league_id, tsc.scope_name, tsc.cohort_key,
               sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_scope tsc
        JOIN team_stats ts ON ts.team_id=tsc.team_id AND ts.league_id=tsc.league_id
                          AND ts.sport=p_sport AND ts.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT team_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY team_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT team_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ts.team_id=agg.team_id AND ts.league_id=agg.league_id
                          AND ts.sport=p_sport AND ts.season=p_season;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$;


--
-- Name: recompute_entity_tiers(text, integer, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recompute_entity_tiers(p_sport text, p_season integer, p_headliner_limit integer DEFAULT 150) RETURNS TABLE(tier text, entity_count bigint)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_starter_min_apps INTEGER;
    v_starter_min_minutes_per_app NUMERIC;
BEGIN
    -- Per-sport starter thresholds. Mirror the scoracle-seed news
    -- backfill heuristics so the two systems agree on who's a starter.
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
$$;


--
-- Name: recompute_season(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.recompute_season(p_sport text, p_season integer) RETURNS TABLE(players_updated integer, teams_updated integer)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams   INTEGER := 0;
BEGIN
    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(p_sport, p_season) rp;

    PERFORM recalculate_event_percentiles(p_sport, p_season);
    PERFORM compute_rating(p_sport, p_season);
    PERFORM compute_team_rating(p_sport, p_season);
    PERFORM compute_event_starline(p_sport, p_season);
    PERFORM compute_team_event_starline(p_sport, p_season);
    PERFORM recalculate_event_rating_pct(p_sport, p_season);

    IF    p_sport = 'NBA'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF p_sport = 'NFL'      THEN REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF p_sport = 'FOOTBALL' THEN REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$;


--
-- Name: record_transfer_identity_adjudication_failure(text, integer, integer, integer, bigint, bigint, smallint, numeric, text, text, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.record_transfer_identity_adjudication_failure(p_sport text, p_player_id integer, p_old_team_id integer, p_new_team_id integer, p_source_rumor_id bigint, p_source_synthesis_id bigint, p_deterministic_heat smallint, p_deterministic_confidence numeric, p_adjudication_raw text, p_adjudication_model_version text, p_adjudication_prompt_version text, p_reason text) RETURNS bigint
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_old_league_id INTEGER;
    v_new_league_id INTEGER;
    v_threshold public.transfer_identity_thresholds%ROWTYPE;
    v_id BIGINT;
BEGIN
    SELECT * INTO v_threshold
    FROM public.transfer_identity_thresholds
    WHERE sport = p_sport;

    SELECT league_id INTO v_old_league_id
    FROM public.player_current_identity
    WHERE sport = p_sport AND player_id = p_player_id;

    SELECT league_id INTO v_new_league_id
    FROM public.teams
    WHERE sport = p_sport AND id = p_new_team_id;

    INSERT INTO public.transfer_identity_applications (
        sport, player_id, old_team_id, old_league_id, new_team_id, new_league_id,
        source_rumor_id, source_synthesis_id, deterministic_heat, deterministic_confidence,
        threshold_config, adjudication_raw, adjudication_model_version, adjudication_prompt_version,
        decision, status, reason
    ) VALUES (
        p_sport, p_player_id, p_old_team_id, v_old_league_id, p_new_team_id, v_new_league_id,
        p_source_rumor_id, p_source_synthesis_id, p_deterministic_heat, p_deterministic_confidence,
        COALESCE(to_jsonb(v_threshold), '{}'::jsonb), p_adjudication_raw,
        p_adjudication_model_version, p_adjudication_prompt_version,
        'failed_closed', 'failed_closed', p_reason
    )
    RETURNING id INTO v_id;

    RETURN v_id;
END;
$$;


--
-- Name: refresh_co_mention_links(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_co_mention_links(p_sport text, p_window_days integer DEFAULT 90, OUT pairs_upserted integer, OUT pairs_zeroed integer) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run_started timestamptz := clock_timestamp();
BEGIN
    -- Pair corpus mirrors compute_transfer_heat's house rules: vetted both sides, title
    -- PROXIMITY within 50 when both positions are known, source-tier weighting with the
    -- 0.3 unknown-source default. Differences, deliberate: no bucket filter (all news
    -- counts toward association, not just transfer talk) and a 21-DAY recency half-life
    -- instead of transfer heat's 72-hour decay — a standing association fades in weeks,
    -- a trade window in days.
    WITH corpus AS (
        SELECT e1.entity_type AS subject_type, e1.entity_id AS subject_id,
               e2.entity_type AS object_type,  e2.entity_id AS object_id,
               a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities e1 ON e1.article_id = a.id
             AND e1.sport = p_sport AND e1.vetted IS TRUE
        JOIN news_article_entities e2 ON e2.article_id = a.id
             AND e2.sport = p_sport AND e2.vetted IS TRUE
        WHERE a.published_at > now() - make_interval(days => p_window_days)
          AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
          AND (e1.title_pos IS NULL OR e2.title_pos IS NULL
               OR abs(e1.title_pos - e2.title_pos) <= 50)
    ),
    agg AS (
        SELECT c.subject_type, c.subject_id, c.object_type, c.object_id,
               count(*) AS total,
               count(DISTINCT c.src) AS distinct_sources,
               count(*) FILTER (WHERE c.ts > now() - INTERVAL '14 days') AS recent14,
               max(c.ts) AS newest,
               min(c.ts) AS oldest,
               COALESCE(MAX(st.weight), 0.3) AS tier_weight
        FROM corpus c
        LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = 'news'
        GROUP BY 1, 2, 3, 4
    ),
    calc AS (
        SELECT *,
               EXTRACT(EPOCH FROM (now() - newest)) / 86400.0 AS age_days,
               LEAST(1.0, distinct_sources::numeric / 5.0) AS volume,
               recent14::numeric / GREATEST(total, 1) AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *,
               exp(-age_days / 21.0) AS recency,
               GREATEST(0, LEAST(100, round(
                   100 * tier_weight * exp(-age_days / 21.0)
                       * (0.6 * volume + 0.4 * recent_frac))))::smallint AS strength
        FROM calc
    )
    INSERT INTO narrative_links AS l
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         strength, components, article_count, distinct_sources,
         first_seen_at, last_event_at, trajectory, trajectory_components, updated_at)
    SELECT p_sport, 'co_mention', f.subject_type, f.subject_id, f.object_type, f.object_id,
           f.strength,
           jsonb_build_object(
               'article_count', f.total,
               'distinct_sources', f.distinct_sources,
               'recent_14d', f.recent14,
               'newest_age_days', round(f.age_days::numeric, 1),
               'tier_weight', f.tier_weight,
               'volume', round(f.volume::numeric, 3),
               'recency', round(f.recency::numeric, 3),
               'recent_frac', round(f.recent_frac::numeric, 3),
               'window_days', p_window_days,
               'window_oldest', f.oldest),
           f.total, f.distinct_sources,
           f.oldest, f.newest,
           'developing_story',
           jsonb_build_object('previous', NULL, 'current', f.strength,
                              'delta', NULL, 'direction', 'new_or_unmatched'),
           v_run_started
    FROM fin f
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
    DO UPDATE SET
        strength = EXCLUDED.strength,
        components = EXCLUDED.components,
        article_count = EXCLUDED.article_count,
        distinct_sources = EXCLUDED.distinct_sources,
        first_seen_at = LEAST(l.first_seen_at, EXCLUDED.first_seen_at),
        last_event_at = EXCLUDED.last_event_at,
        -- The shared ±10-bucket trajectory vocabulary (rust/src/trajectory.rs
        -- classify_delta / go/internal/db/db.go): keep all three homes in step.
        trajectory = CASE
            WHEN l.strength IS NULL THEN 'developing_story'
            WHEN EXCLUDED.strength - l.strength >= 10 THEN 'heating_up'
            WHEN EXCLUDED.strength - l.strength <= -10 THEN 'cooling_off'
            ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength,
            'current', EXCLUDED.strength,
            'delta', CASE WHEN l.strength IS NULL THEN NULL
                          ELSE EXCLUDED.strength - l.strength END,
            'direction', CASE
                WHEN l.strength IS NULL THEN 'new_or_unmatched'
                WHEN EXCLUDED.strength - l.strength >= 10 THEN 'up'
                WHEN EXCLUDED.strength - l.strength <= -10 THEN 'down'
                ELSE 'stable' END),
        updated_at = v_run_started;

    GET DIAGNOSTICS pairs_upserted = ROW_COUNT;

    -- Pairs that fell out of the window entirely: decay to 0 rather than delete, so
    -- first_seen/trajectory history survives a quiet spell. (A periodic prune of
    -- long-dead zero rows can come later if volume ever warrants it.)
    UPDATE narrative_links l
    SET trajectory = CASE WHEN l.strength >= 10 THEN 'cooling_off'
                          ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength, 'current', 0,
            'delta', -l.strength,
            'direction', CASE WHEN l.strength >= 10 THEN 'down' ELSE 'stable' END),
        strength = 0,
        components = l.components || '{"decayed_out_of_window": true}'::jsonb,
        updated_at = v_run_started
    WHERE l.sport = p_sport
      AND l.link_type = 'co_mention'
      AND l.updated_at < v_run_started
      AND l.strength IS DISTINCT FROM 0;

    GET DIAGNOSTICS pairs_zeroed = ROW_COUNT;
END;
$$;


--
-- Name: FUNCTION refresh_co_mention_links(p_sport text, p_window_days integer, OUT pairs_upserted integer, OUT pairs_zeroed integer); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.refresh_co_mention_links(p_sport text, p_window_days integer, OUT pairs_upserted integer, OUT pairs_zeroed integer) IS 'Recomputes all co_mention narrative_links for a sport from the vetted news rail — idempotent, set-based, no model calls. v1 reads news_article_entities only; extend with a narrative_person_mentions union once the extraction stage populates persons.';


--
-- Name: refresh_latest_momentum_scores_per_entity(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_latest_momentum_scores_per_entity() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    REFRESH MATERIALIZED VIEW public.latest_momentum_scores_per_entity;
    RETURN NULL;
END;
$$;


--
-- Name: FUNCTION refresh_latest_momentum_scores_per_entity(); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.refresh_latest_momentum_scores_per_entity() IS 'Statement trigger helper for latest_momentum_scores_per_entity. The source history remains append-only; this refresh moves the current-row projection cost to writes/cleanup instead of every hot leaderboard read.';


--
-- Name: refresh_momentum_scores(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_momentum_scores(p_sport text DEFAULT NULL::text) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    inserted_count INTEGER;
BEGIN
    -- Single-flight: the NOTIFY listener and the catch-up ticker can race a
    -- drain for the same sport. The loser returns NULL (NOT 0) — the Go drain
    -- leaves the dirty marker in place on NULL so the refresh is retried,
    -- never double-appended and never silently lost.
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN
        RETURN NULL;
    END IF;

    WITH target_sports AS (
        SELECT id AS sport, current_season
        FROM public.sports
        WHERE p_sport IS NULL OR id = upper(p_sport)
    ),
    -- Vibe window: a plain 21 calendar days. News sentiment flows through the
    -- offseason, so this clock never pauses with the fixture calendar.
    vibe AS (
        SELECT entity_type, entity_id, sport,
               ((array_agg(sentiment ORDER BY generated_at DESC))[1]
                - (array_agg(sentiment ORDER BY generated_at ASC))[1])::numeric AS vibe_slope,
               count(*)::int AS vibe_samples,
               min(generated_at) AS vibe_window_start,
               max(generated_at) AS vibe_window_end
        FROM public.vibe_scores
        WHERE sentiment IS NOT NULL
          AND generated_at > NOW() - INTERVAL '21 days'
          AND sport IN (SELECT sport FROM target_sports)
        GROUP BY entity_type, entity_id, sport
        HAVING count(*) >= 3
    ),
    -- Rating lookback: the entity's last season_bridge_window(sport) rated
    -- games (~10% of the season — the shared mig-025 schedule), across
    -- (current, previous) seasons so the lookback closes at a season's end
    -- and resumes at the next season's first game. Game-count, not calendar:
    -- bye weeks and schedule gaps cannot starve it.
    player_ranked AS (
        SELECT e.player_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.player_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    team_ranked AS (
        SELECT e.team_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.team_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    player_rating AS (
        SELECT 'player'::text AS entity_type, pr.entity_id, pr.sport,
               max(pr.season) AS season,
               ((array_agg(pr.rating_composite_pct ORDER BY pr.start_time DESC))[1]
                - (array_agg(pr.rating_composite_pct ORDER BY pr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(pr.start_time) AS rating_window_start,
               max(pr.start_time) AS rating_window_end
        FROM player_ranked pr
        WHERE pr.rn <= public.season_bridge_window(pr.sport)
        GROUP BY pr.entity_id, pr.sport
        HAVING count(*) >= 2
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, tr.entity_id, tr.sport,
               max(tr.season) AS season,
               ((array_agg(tr.rating_composite_pct ORDER BY tr.start_time DESC))[1]
                - (array_agg(tr.rating_composite_pct ORDER BY tr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(tr.start_time) AS rating_window_start,
               max(tr.start_time) AS rating_window_end
        FROM team_ranked tr
        WHERE tr.rn <= public.season_bridge_window(tr.sport)
        GROUP BY tr.entity_id, tr.sport
        HAVING count(*) >= 2
    ),
    rating AS (
        SELECT * FROM player_rating
        UNION ALL
        SELECT * FROM team_rating
    ),
    entity_scores AS (
        SELECT COALESCE(v.entity_type, r.entity_type) AS entity_type,
               COALESCE(v.entity_id, r.entity_id) AS entity_id,
               COALESCE(v.sport, r.sport) AS sport,
               r.season,
               v.vibe_slope, COALESCE(v.vibe_samples, 0) AS vibe_samples,
               v.vibe_window_start, v.vibe_window_end,
               r.rating_slope, COALESCE(r.rating_samples, 0) AS rating_samples,
               r.rating_window_start, r.rating_window_end
        FROM vibe v
        FULL OUTER JOIN rating r
          ON r.entity_type = v.entity_type
         AND r.entity_id = v.entity_id
         AND r.sport = v.sport
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           -- SIGNED: the average of the present slopes, sign preserved. Falls
           -- are as much a historic datapoint as rises — this number is the
           -- durable per-snapshot momentum record, so it must not clamp
           -- downside.
           round((
               COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0)
           ) / NULLIF(
               (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
               + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
               0
           ), 3) AS momentum_score
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$$;


--
-- Name: refresh_peer_cohort_aggregates(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_peer_cohort_aggregates() RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    n integer;
BEGIN
    DELETE FROM public.peer_cohort_aggregate;

    INSERT INTO public.peer_cohort_aggregate
        (sport, season, league_id, entity_type, position,
         key_sums, key_cnts, score_sum, score_cnt, member_count, computed_at)
    WITH members AS (
        SELECT ps.sport, ps.season, ps.league_id, 'player'::text AS entity_type,
               ps.position, ps.stats, ps.season_composite_score
        FROM public.player_stats ps
        WHERE ps.position IS NOT NULL
        UNION ALL
        SELECT ts.sport, ts.season, ts.league_id, 'team'::text,
               ''::text AS position, ts.stats, ts.season_composite_score
        FROM public.team_stats ts
    ),
    exploded AS (
        SELECT m.sport, m.season, m.league_id, m.entity_type, m.position, kv.key,
               CASE
                   WHEN sd.unit = 'cumulative_total' THEN
                       (kv.value)::numeric
                       / NULLIF((m.stats->>(CASE WHEN m.sport = 'FOOTBALL'
                                                 THEN 'matches_played' ELSE 'games_played' END))::numeric, 0)
                   ELSE (kv.value)::numeric
               END AS val
        FROM members m
        CROSS JOIN LATERAL jsonb_each(m.stats) kv
        JOIN public.stat_definitions sd
          ON sd.sport = m.sport
         AND sd.entity_type = m.entity_type
         AND sd.key_name = kv.key
         AND sd.comparable = true
        WHERE jsonb_typeof(kv.value) = 'number'
    ),
    key_agg AS (
        SELECT sport, season, league_id, entity_type, position, key,
               SUM(val) AS sum_val, COUNT(*) AS cnt_val
        FROM exploded
        WHERE val IS NOT NULL          -- divisor=0 → NULL → excluded from sum AND count
        GROUP BY 1,2,3,4,5,6
    ),
    keyed AS (
        SELECT sport, season, league_id, entity_type, position,
               jsonb_object_agg(key, sum_val) AS key_sums,
               jsonb_object_agg(key, cnt_val) AS key_cnts
        FROM key_agg
        GROUP BY 1,2,3,4,5
    ),
    sized AS (
        SELECT sport, season, league_id, entity_type, position,
               COUNT(*) AS member_count,
               SUM(season_composite_score) AS score_sum,
               COUNT(season_composite_score) AS score_cnt
        FROM members
        GROUP BY 1,2,3,4,5
    )
    SELECT s.sport, s.season, s.league_id, s.entity_type, s.position,
           COALESCE(k.key_sums, '{}'::jsonb),
           COALESCE(k.key_cnts, '{}'::jsonb),
           s.score_sum, s.score_cnt, s.member_count, now()
    FROM sized s
    LEFT JOIN keyed k USING (sport, season, league_id, entity_type, position);

    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END
$$;


--
-- Name: refresh_sport_autofill(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_sport_autofill(p_sport text, p_reason text DEFAULT NULL::text) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    PERFORM public.request_sport_autofill_refresh(p_sport, p_reason);
END;
$$;


--
-- Name: FUNCTION refresh_sport_autofill(p_sport text, p_reason text); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.refresh_sport_autofill(p_sport text, p_reason text) IS 'Transaction-safe autofill invalidation shim. Marks sport_autofill_versions refreshing; callers must run REFRESH MATERIALIZED VIEW CONCURRENTLY outside the DB function and then call complete_sport_autofill_refresh().';


--
-- Name: refresh_stat_matchups(text, text, text, numeric, numeric, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_stat_matchups(p_sport text, p_stat_key text, p_scope text DEFAULT 'career'::text, p_k numeric DEFAULT 5, p_min_minutes numeric DEFAULT 10, p_min_baseline_games integer DEFAULT 10, OUT rows_written integer, OUT subjects_covered integer) RETURNS record
    LANGUAGE plpgsql
    AS $_$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    IF p_scope <> 'career' THEN
        RAISE EXCEPTION 'refresh_stat_matchups: only career scope is implemented (got %)', p_scope;
    END IF;

    DELETE FROM stat_matchups
    WHERE sport = p_sport AND scope = p_scope AND stat_key = p_stat_key
      AND subject_type = 'player' AND object_type = 'team';

    WITH per_game AS (
        -- One row per qualifying appearance. Absent count-stat key = real zero;
        -- minutes floor keeps DNPs/cameos out of baselines. Malformed values -> 0.
        SELECT b.player_id,
               b.season,
               CASE WHEN b.team_id = f.home_team_id THEN f.away_team_id
                    ELSE f.home_team_id END AS opponent_id,
               CASE WHEN b.stats->>p_stat_key ~ '^-?[0-9]+\.?[0-9]*$'
                    THEN (b.stats->>p_stat_key)::numeric
                    ELSE 0 END AS v
        FROM event_box_scores b
        JOIN fixtures f ON f.id = b.fixture_id AND f.sport = b.sport
        WHERE b.sport = p_sport
          AND b.minutes_played IS NOT NULL
          AND b.minutes_played >= p_min_minutes
    ),
    bounds AS (
        SELECT max(season) AS max_season, avg(v) AS league_avg FROM per_game
    ),
    opp_allow AS (
        -- What each team concedes on this stat, relative to league average.
        SELECT g.opponent_id,
               avg(g.v) / NULLIF((SELECT league_avg FROM bounds), 0) AS allow_ratio
        FROM per_game g
        GROUP BY 1
    ),
    baseline AS (
        SELECT player_id, avg(v) AS base_avg, count(*) AS n_baseline
        FROM per_game
        GROUP BY 1
        HAVING count(*) >= p_min_baseline_games
    ),
    matchup AS (
        SELECT g.player_id, g.opponent_id,
               count(*) AS n,
               avg(g.v) AS m_avg,
               min(g.season) AS first_season,
               max(g.season) AS last_season,
               count(*) FILTER (
                   WHERE g.season >= (SELECT max_season FROM bounds) - 1
               ) AS recent_n
        FROM per_game g
        GROUP BY 1, 2
    ),
    calc AS (
        SELECT m.*, b.base_avg, b.n_baseline,
               COALESCE(oa.allow_ratio, 1) AS allow_ratio,
               b.base_avg * COALESCE(oa.allow_ratio, 1) AS expected,
               m.m_avg - b.base_avg AS raw_delta,
               m.m_avg - b.base_avg * COALESCE(oa.allow_ratio, 1) AS adj_delta,
               m.n / (m.n + p_k) AS shrink,
               0.5 + 0.5 * (m.recent_n::numeric / m.n) AS recency_factor
        FROM matchup m
        JOIN baseline b USING (player_id)
        LEFT JOIN opp_allow oa ON oa.opponent_id = m.opponent_id
    ),
    dir AS (
        -- Direction-consistency: share of the pair's meetings on the same side of
        -- expected as the overall adjusted delta. n=1 trivially scores 1.0 — harmless,
        -- the shrink term already crushes single-meeting reliability.
        SELECT c.player_id, c.opponent_id,
               avg(CASE WHEN sign(g.v - c.expected) = sign(c.adj_delta)
                        THEN 1.0 ELSE 0.0 END) AS dir_share
        FROM calc c
        JOIN per_game g ON g.player_id = c.player_id AND g.opponent_id = c.opponent_id
        WHERE c.adj_delta <> 0
        GROUP BY 1, 2
    )
    INSERT INTO stat_matchups
        (sport, scope, stat_key, subject_type, subject_id, object_type, object_id,
         n_games, baseline_avg, matchup_avg, expected_avg,
         raw_delta, adj_delta, shrunk_delta, reliability, components,
         first_season, last_season, updated_at)
    SELECT p_sport, p_scope, p_stat_key, 'player', c.player_id, 'team', c.opponent_id,
           c.n,
           round(c.base_avg, 3), round(c.m_avg, 3), round(c.expected, 3),
           round(c.raw_delta, 3), round(c.adj_delta, 3),
           round(c.adj_delta * c.shrink, 3),
           GREATEST(0, LEAST(100, round(
               100 * c.shrink
                   * c.recency_factor
                   * (0.5 + 0.5 * COALESCE(d.dir_share, 0.5)))))::smallint,
           jsonb_build_object(
               'n', c.n,
               'n_baseline', c.n_baseline,
               'recent_n', c.recent_n,
               'shrink', round(c.shrink, 3),
               'recency_factor', round(c.recency_factor, 3),
               'dir_share', round(COALESCE(d.dir_share, 0.5), 3),
               'allow_ratio', round(c.allow_ratio, 3),
               'k', p_k,
               'min_minutes', p_min_minutes),
           c.first_season, c.last_season, v_run
    FROM calc c
    LEFT JOIN dir d ON d.player_id = c.player_id AND d.opponent_id = c.opponent_id;

    GET DIAGNOSTICS rows_written = ROW_COUNT;

    SELECT count(DISTINCT subject_id) INTO subjects_covered
    FROM stat_matchups
    WHERE sport = p_sport AND scope = p_scope AND stat_key = p_stat_key;
END;
$_$;


--
-- Name: FUNCTION refresh_stat_matchups(p_sport text, p_stat_key text, p_scope text, p_k numeric, p_min_minutes numeric, p_min_baseline_games integer, OUT rows_written integer, OUT subjects_covered integer); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.refresh_stat_matchups(p_sport text, p_stat_key text, p_scope text, p_k numeric, p_min_minutes numeric, p_min_baseline_games integer, OUT rows_written integer, OUT subjects_covered integer) IS 'Full recompute of player-vs-team matchup edges for one (sport, stat_key): opponent-adjusted deltas + shrinkage-based reliability, presented not gatekept. Count-stat semantics (absent key = 0). DELETE + INSERT, idempotent, no model calls.';


--
-- Name: request_sport_autofill_refresh(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.request_sport_autofill_refresh(p_sport text, p_reason text DEFAULT NULL::text) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    INSERT INTO public.sport_autofill_versions (sport, status, reason, generated_at)
    VALUES (p_sport, 'refreshing', p_reason, NOW())
    ON CONFLICT (sport) DO UPDATE SET
        status = 'refreshing',
        reason = EXCLUDED.reason,
        generated_at = NOW();
END;
$$;


--
-- Name: resolve_provider_fixture_id(integer, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.resolve_provider_fixture_id(p_fixture_id integer, p_provider text, p_sport text) RETURNS text
    LANGUAGE sql STABLE
    AS $$
    SELECT provider_fixture_id
    FROM provider_fixture_map
    WHERE fixture_id = p_fixture_id
      AND provider = p_provider
      AND sport = p_sport
    LIMIT 1;
$$;


--
-- Name: resolve_provider_season_id(integer, integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.resolve_provider_season_id(p_league_id integer, p_season_year integer, p_provider text DEFAULT 'sportmonks'::text) RETURNS integer
    LANGUAGE sql STABLE
    AS $$
    SELECT provider_season_id
    FROM provider_seasons
    WHERE league_id = p_league_id
      AND season_year = p_season_year
      AND provider = p_provider;
$$;


--
-- Name: revert_applied_transfer_identity(bigint, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.revert_applied_transfer_identity(p_application_id bigint, p_reverted_by text, p_revert_reason text) RETURNS TABLE(application_id bigint, override_id bigint, status text, reason text)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_app public.transfer_identity_applications%ROWTYPE;
BEGIN
    SELECT * INTO v_app
    FROM public.transfer_identity_applications
    WHERE id = p_application_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'transfer identity application % not found', p_application_id;
    END IF;

    IF v_app.status <> 'applied' OR v_app.override_id IS NULL THEN
        RETURN QUERY SELECT v_app.id, v_app.override_id, v_app.status, 'application is not an active applied override'::text;
        RETURN;
    END IF;

    UPDATE public.player_current_identity_overrides
    SET reverted_at = NOW(),
        reverted_by = p_reverted_by,
        revert_reason = p_revert_reason
    WHERE id = v_app.override_id
      AND reverted_at IS NULL;

    UPDATE public.transfer_identity_applications
    SET status = 'reverted',
        reverted_at = NOW(),
        reverted_by = p_reverted_by,
        revert_reason = p_revert_reason
    WHERE id = p_application_id;

    PERFORM public.refresh_sport_autofill(v_app.sport, 'revert_applied_transfer_identity');

    RETURN QUERY SELECT v_app.id, v_app.override_id, 'reverted'::text, COALESCE(p_revert_reason, 'reverted applied transfer identity');
END;
$$;


--
-- Name: roll_narrative_episodes(text, integer, integer, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.roll_narrative_episodes(p_sport text, p_open_threshold integer DEFAULT 40, p_close_threshold integer DEFAULT 15, p_quiet_days integer DEFAULT 14, OUT episodes_opened integer, OUT episodes_updated integer, OUT episodes_sealed integer) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    -- Open: a link crossing the notability threshold with no open episode starts one.
    -- started_at prefers the current window's oldest article (components.window_oldest,
    -- refresh v2) over the link's all-time first_seen_at, so a REOPENED association is
    -- dated from its new coverage. Slight overlap with a just-sealed episode's tail is
    -- accepted fuzz.
    INSERT INTO narrative_episodes
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         status, season, started_at, peaked_at, peak_strength, last_strength,
         article_count, distinct_sources, event_count, peak_components,
         created_at, updated_at)
    SELECT l.sport, l.link_type, l.subject_type, l.subject_id,
           l.object_type, l.object_id,
           'open', s.current_season,
           COALESCE((l.components->>'window_oldest')::timestamptz,
                    l.first_seen_at, v_run),
           v_run, l.strength, l.strength,
           l.article_count, l.distinct_sources, l.event_count, l.components,
           v_run, v_run
    FROM narrative_links l
    JOIN sports s ON s.id = l.sport
    WHERE l.sport = p_sport
      AND l.strength >= p_open_threshold
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
        WHERE status = 'open'
    DO NOTHING;

    GET DIAGNOSTICS episodes_opened = ROW_COUNT;

    -- Freshen every open episode from its link, whatever the current strength: track
    -- the peak, keep counters at their episode-high (the link's window slides, so
    -- GREATEST approximates "over the episode"), remember the latest strength.
    UPDATE narrative_episodes e
    SET last_strength = l.strength,
        article_count = GREATEST(e.article_count, l.article_count),
        distinct_sources = GREATEST(e.distinct_sources, l.distinct_sources),
        event_count = GREATEST(e.event_count, l.event_count),
        peak_strength = GREATEST(e.peak_strength, l.strength),
        peaked_at = CASE WHEN l.strength > e.peak_strength THEN v_run
                         ELSE e.peaked_at END,
        peak_components = CASE WHEN l.strength > e.peak_strength THEN l.components
                               ELSE e.peak_components END,
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id;

    GET DIAGNOSTICS episodes_updated = ROW_COUNT;

    -- Seal: weak AND quiet → the story is over; write the memory. The mechanical path
    -- only ever concludes 'fizzled' — 'confirmed' belongs to the event-driven path
    -- (a trade_confirmed narrative_event sealing its trade_rumor episode).
    UPDATE narrative_episodes e
    SET status = 'sealed',
        outcome = 'fizzled',
        ended_at = COALESCE(l.last_event_at, e.updated_at),
        last_strength = l.strength,
        evidence = e.evidence || jsonb_build_object('sealed', jsonb_build_object(
            'at', v_run,
            'reason', 'quiet',
            'final_strength', l.strength,
            'close_threshold', p_close_threshold,
            'quiet_days', p_quiet_days)),
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id
      AND l.strength < p_close_threshold
      AND (l.last_event_at IS NULL
           OR l.last_event_at < now() - make_interval(days => p_quiet_days));

    GET DIAGNOSTICS episodes_sealed = ROW_COUNT;
END;
$$;


--
-- Name: FUNCTION roll_narrative_episodes(p_sport text, p_open_threshold integer, p_close_threshold integer, p_quiet_days integer, OUT episodes_opened integer, OUT episodes_updated integer, OUT episodes_sealed integer); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.roll_narrative_episodes(p_sport text, p_open_threshold integer, p_close_threshold integer, p_quiet_days integer, OUT episodes_opened integer, OUT episodes_updated integer, OUT episodes_sealed integer) IS 'Episode lifecycle over narrative_links state: open at strength >= open_threshold, track peak while open, seal as fizzled when strength < close_threshold AND quiet for quiet_days. Link_type-generic. Run after refresh_co_mention_links each night (cron-narrative-links.sh); the thresholds are the notability dial.';


--
-- Name: season_bridge_window(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.season_bridge_window(p_sport text) RETURNS integer
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA'      THEN 8
        WHEN 'NFL'      THEN 2
        WHEN 'FOOTBALL' THEN 4
        ELSE 10
    END;
$$;


--
-- Name: FUNCTION season_bridge_window(p_sport text); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.season_bridge_window(p_sport text) IS 'THE season-end/season-start threshold window, in games (~10% of season length: NBA 8/82, NFL 2/17, FOOTBALL 4/38). Established by the migration-025 cold-start guard. Consumers: recalculate_event_percentiles blends season_composite_score with the prior-season anchor while current-season games < window; refresh_momentum_scores reads each entity''s rating slope over its last <window> rated games (a season-spanning lookback, mig 130). Tune it here and every season-boundary consumer moves together — do not re-introduce inline copies.';


--
-- Name: snapshot_rating_history(text, integer, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.snapshot_rating_history(p_sport text, p_season integer, p_trigger text DEFAULT 'recompute'::text) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_inserted INTEGER := 0;
    v_count    INTEGER;
BEGIN
    -- Players
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating_composite, rating_composite_score, rating_composite_rank,
        rating_specialist, rating_specialist_score, rating_specialist_rank, rating_specialty,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'player', ps.player_id, ps.sport, ps.season, ps.league_id,
        ps.rating_composite, ps.rating_composite_score, ps.rating_composite_rank,
        ps.rating_specialist, ps.rating_specialist_score, ps.rating_specialist_rank, ps.rating_specialty,
        ps.season_composite_rank_alltime, ps.rating_modes, p_trigger
    FROM player_stats ps
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND ps.rating_composite IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'player' AND rh.entity_id = ps.player_id
            AND rh.sport = ps.sport AND rh.season = ps.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'player' AND rh2.entity_id = ps.player_id
                  AND rh2.sport = ps.sport AND rh2.season = ps.season)
            AND rh.rating_composite        IS NOT DISTINCT FROM ps.rating_composite
            AND rh.rating_composite_score  IS NOT DISTINCT FROM ps.rating_composite_score
            AND rh.rating_specialist       IS NOT DISTINCT FROM ps.rating_specialist
            AND rh.rating_specialist_score IS NOT DISTINCT FROM ps.rating_specialist_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    -- Teams (team_stats has no rating_modes column → NULL)
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating_composite, rating_composite_score, rating_composite_rank,
        rating_specialist, rating_specialist_score, rating_specialist_rank, rating_specialty,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'team', ts.team_id, ts.sport, ts.season, ts.league_id,
        ts.rating_composite, ts.rating_composite_score, ts.rating_composite_rank,
        ts.rating_specialist, ts.rating_specialist_score, ts.rating_specialist_rank, ts.rating_specialty,
        ts.season_composite_rank_alltime, NULL::jsonb, p_trigger
    FROM team_stats ts
    WHERE ts.sport = p_sport AND ts.season = p_season
      AND ts.rating_composite IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'team' AND rh.entity_id = ts.team_id
            AND rh.sport = ts.sport AND rh.season = ts.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'team' AND rh2.entity_id = ts.team_id
                  AND rh2.sport = ts.sport AND rh2.season = ts.season)
            AND rh.rating_composite        IS NOT DISTINCT FROM ts.rating_composite
            AND rh.rating_composite_score  IS NOT DISTINCT FROM ts.rating_composite_score
            AND rh.rating_specialist       IS NOT DISTINCT FROM ts.rating_specialist
            AND rh.rating_specialist_score IS NOT DISTINCT FROM ts.rating_specialist_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    RETURN v_inserted;
END;
$$;


--
-- Name: team_datapoints_block(text, jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.team_datapoints_block(p_sport text, p_stats jsonb, p_pct jsonb, p_scoped jsonb) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key', sd.key_name, 'label', sd.display_name,
               'value', COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',   (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', sd.category, 'sort', sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'team' AND sd.key_name = k.key;
$$;


--
-- Name: team_template_block(text, jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.team_template_block(p_sport text, p_stats jsonb, p_pct jsonb, p_scoped jsonb) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
    SELECT jsonb_build_object('default', jsonb_agg(jsonb_build_object(
               'key', t.stat_key, 'label', COALESCE(sd.display_name, t.stat_key),
               'value', COALESCE((p_stats->>t.stat_key)::numeric, 0),
               'pct',   COALESCE((p_pct->>t.stat_key)::numeric, 0),
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>t.stat_key)::numeric)
                                 FILTER (WHERE s.keys ? t.stat_key), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', t.facet, 'sort', t.sort_order
           ) ORDER BY t.sort_order, t.stat_key))
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.sport = upper(p_sport) AND t.position_group = 'team'
    HAVING count(*) > 0;
$$;


--
-- Name: template_block(text, text, jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.template_block(p_sport text, p_position text, p_stats jsonb, p_pct jsonb, p_scoped jsonb) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
    WITH tmpl AS (
        SELECT t.stat_key, COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label, t.facet, t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport) AND t.position_group = public.position_group(p_sport, p_position)
    ),
    modes(mode, suffix) AS (
        SELECT 'default'::text, ''::text
        UNION SELECT DISTINCT rm.mode, rm.suffix FROM public.rate_modes rm
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_stats->>t.stat_key)::numeric, 0),
                'pct',   COALESCE((p_pct->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_pct->>t.stat_key)::numeric, 0),
                'scoped_pct', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric)
                                  FILTER (WHERE s.keys ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END)), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                ),
                'facet', t.facet,
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$$;


--
-- Name: update_sync_log_timestamp(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.update_sync_log_timestamp() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;


--
-- Name: upsert_fixture(integer, text, integer, integer, integer, integer, timestamp with time zone, text, text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.upsert_fixture(p_external_id integer, p_sport text, p_league_id integer, p_season integer, p_home_team_id integer, p_away_team_id integer, p_start_time timestamp with time zone, p_venue_name text DEFAULT NULL::text, p_round text DEFAULT NULL::text, p_seed_delay_hours integer DEFAULT 4) RETURNS integer
    LANGUAGE sql
    AS $$
    INSERT INTO fixtures (external_id, sport, league_id, season, home_team_id,
                          away_team_id, start_time, venue_name, round, seed_delay_hours)
    VALUES (p_external_id, p_sport, p_league_id, p_season, p_home_team_id,
            p_away_team_id, p_start_time, p_venue_name, p_round, p_seed_delay_hours)
    ON CONFLICT (sport, external_id) DO UPDATE SET
        league_id = COALESCE(EXCLUDED.league_id, fixtures.league_id),
        season = EXCLUDED.season,
        home_team_id = EXCLUDED.home_team_id,
        away_team_id = EXCLUDED.away_team_id,
        start_time = EXCLUDED.start_time,
        venue_name = COALESCE(EXCLUDED.venue_name, fixtures.venue_name),
        round = COALESCE(EXCLUDED.round, fixtures.round),
        seed_delay_hours = EXCLUDED.seed_delay_hours,
        updated_at = NOW()
    RETURNING id;
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: leagues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.leagues (
    id integer NOT NULL,
    sport text NOT NULL,
    name text NOT NULL,
    country text,
    logo_url text,
    sportmonks_id integer,
    is_benchmark boolean DEFAULT false,
    is_active boolean DEFAULT true,
    handicap numeric,
    meta jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: player_current_identity_overrides; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.player_current_identity_overrides (
    id bigint NOT NULL,
    sport text NOT NULL,
    player_id integer NOT NULL,
    team_id integer,
    league_id integer,
    source text DEFAULT 'manual'::text NOT NULL,
    source_rumor_id bigint,
    source_synthesis_id bigint,
    confidence numeric(4,3),
    reason text,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    applied_at timestamp with time zone DEFAULT now() NOT NULL,
    applied_by text,
    reverted_at timestamp with time zone,
    reverted_by text,
    revert_reason text
);


--
-- Name: player_stats; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.player_stats (
    player_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer DEFAULT 0 NOT NULL,
    team_id integer,
    stats jsonb DEFAULT '{}'::jsonb NOT NULL,
    percentiles jsonb DEFAULT '{}'::jsonb,
    raw_response jsonb,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    scoped_percentiles jsonb DEFAULT '{}'::jsonb,
    "position" text,
    season_composite_score numeric,
    season_composite_rank numeric,
    season_composite_rank_alltime numeric,
    season_composite_rank_absolute numeric,
    season_composite_rank_alltime_absolute numeric,
    rating_composite numeric,
    rating_specialist numeric,
    rating_specialty text,
    rating_composite_rank numeric,
    rating_specialist_rank numeric,
    rating_breakdown jsonb,
    rating_scoped_ranks jsonb,
    rating_modes jsonb,
    rating_composite_score numeric,
    rating_specialist_score numeric,
    rating_scoped_scores jsonb
);


--
-- Name: players; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.players (
    id integer NOT NULL,
    sport text NOT NULL,
    name text NOT NULL,
    first_name text,
    last_name text,
    nationality text,
    date_of_birth date,
    height text,
    weight text,
    photo_url text,
    team_id integer,
    league_id integer,
    search_aliases text[] DEFAULT '{}'::text[],
    meta jsonb DEFAULT '{}'::jsonb,
    raw_response jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    tier text DEFAULT 'inactive'::text NOT NULL,
    CONSTRAINT players_tier_check CHECK ((tier = ANY (ARRAY['headliner'::text, 'starter'::text, 'bench'::text, 'inactive'::text])))
);


--
-- Name: team_rosters; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.team_rosters (
    sport text NOT NULL,
    season integer NOT NULL,
    team_id integer NOT NULL,
    player_id integer NOT NULL,
    jersey_number text,
    "position" text,
    position_group text,
    is_active boolean DEFAULT true NOT NULL,
    source text,
    first_seen timestamp with time zone DEFAULT now() NOT NULL,
    last_seen timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: teams; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.teams (
    id integer NOT NULL,
    sport text NOT NULL,
    name text NOT NULL,
    short_code text,
    country text,
    city text,
    logo_url text,
    league_id integer,
    founded integer,
    venue_name text,
    venue_capacity integer,
    conference text,
    division text,
    search_aliases text[] DEFAULT '{}'::text[],
    meta jsonb DEFAULT '{}'::jsonb,
    raw_response jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    tier text DEFAULT 'headliner'::text NOT NULL,
    CONSTRAINT teams_tier_check CHECK ((tier = ANY (ARRAY['headliner'::text, 'starter'::text, 'bench'::text, 'inactive'::text])))
);


--
-- Name: player_current_identity; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.player_current_identity AS
 WITH active_override AS (
         SELECT DISTINCT ON (o.sport, o.player_id) o.sport,
            o.player_id,
            o.team_id,
            o.league_id,
            NULL::text AS "position",
            NULL::text AS position_group,
            NULL::text AS jersey_number,
            'override'::text AS source,
            o.applied_at AS source_updated_at
           FROM public.player_current_identity_overrides o
          WHERE (o.reverted_at IS NULL)
          ORDER BY o.sport, o.player_id, o.applied_at DESC, o.id DESC
        ), roster_current AS (
         SELECT DISTINCT ON (tr.sport, tr.player_id) tr.sport,
            tr.player_id,
            tr.team_id,
            t.league_id,
            tr."position",
            tr.position_group,
            tr.jersey_number,
            'roster'::text AS source,
            tr.last_seen AS source_updated_at
           FROM (public.team_rosters tr
             LEFT JOIN public.teams t ON (((t.id = tr.team_id) AND (t.sport = tr.sport))))
          WHERE tr.is_active
          ORDER BY tr.sport, tr.player_id, tr.season DESC, tr.last_seen DESC NULLS LAST
        ), stats_current AS (
         SELECT DISTINCT ON (ps.sport, ps.player_id) ps.sport,
            ps.player_id,
            ps.team_id,
            NULLIF(ps.league_id, 0) AS league_id,
            ps."position",
            public.position_group(ps.sport, ps."position") AS position_group,
            NULL::text AS jersey_number,
            'stats'::text AS source,
            ps.updated_at AS source_updated_at
           FROM public.player_stats ps
          WHERE (ps.team_id IS NOT NULL)
          ORDER BY ps.sport, ps.player_id, ps.season DESC NULLS LAST, ps.updated_at DESC NULLS LAST
        )
 SELECT p.sport,
    p.id AS player_id,
    COALESCE(ao.team_id, rc.team_id, sc.team_id, p.team_id) AS team_id,
    COALESCE(ao.league_id, rc.league_id, sc.league_id, p.league_id) AS league_id,
    COALESCE(ao."position", rc."position", sc."position") AS "position",
    COALESCE(ao.position_group, rc.position_group, sc.position_group) AS position_group,
    COALESCE(ao.jersey_number, rc.jersey_number) AS jersey_number,
        CASE
            WHEN (ao.player_id IS NOT NULL) THEN ao.source
            WHEN (rc.player_id IS NOT NULL) THEN rc.source
            WHEN (sc.player_id IS NOT NULL) THEN sc.source
            WHEN ((p.team_id IS NOT NULL) OR (p.league_id IS NOT NULL)) THEN 'legacy_player'::text
            ELSE NULL::text
        END AS source,
    COALESCE(ao.source_updated_at, rc.source_updated_at, sc.source_updated_at, p.updated_at) AS source_updated_at
   FROM (((public.players p
     LEFT JOIN active_override ao ON (((ao.sport = p.sport) AND (ao.player_id = p.id))))
     LEFT JOIN roster_current rc ON (((rc.sport = p.sport) AND (rc.player_id = p.id))))
     LEFT JOIN stats_current sc ON (((sc.sport = p.sport) AND (sc.player_id = p.id))));


--
-- Name: VIEW player_current_identity; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON VIEW public.player_current_identity IS 'Canonical current player identity: applied override, then active roster, then latest stats, then legacy players row. Use for meta, autofill, and transfer prompts.';


--
-- Name: player_current_team; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.player_current_team AS
 SELECT player_id,
    sport,
    team_id
   FROM public.player_current_identity
  WHERE (team_id IS NOT NULL);


--
-- Name: VIEW player_current_team; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON VIEW public.player_current_team IS 'Compatibility current-team view backed by public.player_current_identity; never read raw players.team_id for current team unless this view falls back to legacy_player.';


--
-- Name: team_stats; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.team_stats (
    team_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer DEFAULT 0 NOT NULL,
    stats jsonb DEFAULT '{}'::jsonb NOT NULL,
    percentiles jsonb DEFAULT '{}'::jsonb,
    raw_response jsonb,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    scoped_percentiles jsonb DEFAULT '{}'::jsonb,
    season_composite_score numeric,
    season_composite_rank numeric,
    season_composite_rank_alltime numeric,
    rating_composite numeric,
    rating_specialist numeric,
    rating_specialty text,
    rating_composite_rank numeric,
    rating_specialist_rank numeric,
    rating_breakdown jsonb,
    rating_categories jsonb,
    rating_scoped_ranks jsonb,
    rating_composite_score numeric,
    rating_specialist_score numeric,
    rating_scoped_scores jsonb
);


--
-- Name: autofill_entities; Type: MATERIALIZED VIEW; Schema: football; Owner: -
--

CREATE MATERIALIZED VIEW football.autofill_entities AS
 SELECT football_players.id,
    football_players.type,
    football_players.name,
    football_players.first_name,
    football_players.last_name,
    football_players.nationality,
    football_players.date_of_birth,
    football_players.height,
    football_players.weight,
    football_players.photo_url,
    football_players.team_id,
    football_players.league_id,
    football_players.league_name,
    football_players.team_abbr,
    football_players.team_name,
    football_players.team_logo_url,
    football_players."position",
    football_players.search_tokens,
    football_players.meta
   FROM ( SELECT DISTINCT ON (p.id) p.id,
            'player'::text AS type,
            p.name,
            p.first_name,
            p.last_name,
            p.nationality,
            (p.date_of_birth)::text AS date_of_birth,
            p.height,
            p.weight,
            p.photo_url,
            COALESCE(pct.team_id, p.team_id) AS team_id,
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            t.logo_url AS team_logo_url,
            NULLIF(ps."position", 'Unknown'::text) AS "position",
            jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), lower(COALESCE(l.name, ''::text)), public.unaccent(lower(p.first_name)), public.unaccent(lower(p.last_name)), public.unaccent(lower(replace(p.name, ' '::text, ''::text))), public.unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
            jsonb_build_object('display_name', p.name, 'jersey_number', (p.meta ->> 'jersey_number'::text), 'foot', (p.meta ->> 'foot'::text), 'market_value', ((p.meta ->> 'market_value'::text))::bigint, 'contract_until', (p.meta ->> 'contract_until'::text)) AS meta
           FROM ((((public.players p
             LEFT JOIN public.player_current_team pct ON (((pct.player_id = p.id) AND (pct.sport = p.sport))))
             LEFT JOIN public.teams t ON (((t.id = COALESCE(pct.team_id, p.team_id)) AND (t.sport = p.sport))))
             LEFT JOIN public.player_stats ps ON (((ps.player_id = p.id) AND (ps.sport = p.sport))))
             LEFT JOIN public.leagues l ON ((l.id = ps.league_id)))
          WHERE (p.sport = 'FOOTBALL'::text)
          ORDER BY p.id, ps.season DESC NULLS LAST) football_players
UNION ALL
 SELECT football_teams.id,
    football_teams.type,
    football_teams.name,
    football_teams.first_name,
    football_teams.last_name,
    football_teams.nationality,
    football_teams.date_of_birth,
    football_teams.height,
    football_teams.weight,
    football_teams.photo_url,
    football_teams.team_id,
    football_teams.league_id,
    football_teams.league_name,
    football_teams.team_abbr,
    football_teams.team_name,
    football_teams.team_logo_url,
    football_teams."position",
    football_teams.search_tokens,
    football_teams.meta
   FROM ( SELECT DISTINCT ON (t.id) t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::integer AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            NULL::text AS team_logo_url,
            NULL::text AS "position",
            jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), lower(COALESCE(l.name, ''::text)), public.unaccent(lower(replace(t.name, ' '::text, ''::text))), public.unaccent(lower(t.city))) AS search_tokens,
            jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
           FROM ((public.teams t
             LEFT JOIN public.team_stats ts ON (((ts.team_id = t.id) AND (ts.sport = t.sport))))
             LEFT JOIN public.leagues l ON ((l.id = ts.league_id)))
          WHERE (t.sport = 'FOOTBALL'::text)
          ORDER BY t.id, ts.season DESC NULLS LAST) football_teams
  WITH NO DATA;


--
-- Name: leagues; Type: VIEW; Schema: football; Owner: -
--

CREATE VIEW football.leagues AS
 SELECT id,
    name,
    country,
    logo_url,
    is_benchmark,
    is_active,
    handicap,
    meta
   FROM public.leagues
  WHERE (sport = 'FOOTBALL'::text);


--
-- Name: VIEW leagues; Type: COMMENT; Schema: football; Owner: -
--

COMMENT ON VIEW football.leagues IS 'Football league metadata. Filter by is_active, is_benchmark.';


--
-- Name: player; Type: VIEW; Schema: football; Owner: -
--

CREATE VIEW football.player AS
 SELECT player_id AS id,
    season,
    league_id,
    team_id,
    "position",
    stats,
    ((percentiles - '_position_group'::text) - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((percentiles IS NOT NULL) AND ((percentiles ->> '_position_group'::text) IS NOT NULL)) THEN jsonb_build_object('position_group', (percentiles ->> '_position_group'::text), 'sample_size', COALESCE(((percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    scoped_percentiles,
        CASE
            WHEN ((scoped_percentiles IS NOT NULL) AND (scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ps.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    updated_at AS stats_updated_at
   FROM public.player_stats ps
  WHERE (sport = 'FOOTBALL'::text);


--
-- Name: VIEW player; Type: COMMENT; Schema: football; Owner: -
--

COMMENT ON VIEW football.player IS 'Football player profile — stats only. Name and other meta come from the frontend local cache.';


--
-- Name: standings; Type: VIEW; Schema: football; Owner: -
--

CREATE VIEW football.standings AS
 SELECT ts.team_id,
    ts.season,
    ts.league_id,
    t.name AS team_name,
    t.short_code AS team_abbr,
    t.logo_url,
    l.name AS league_name,
    ts.stats,
    ((ts.stats ->> 'points'::text))::integer AS sort_points,
    ((ts.stats ->> 'goal_difference'::text))::integer AS sort_goal_diff
   FROM ((public.team_stats ts
     JOIN public.teams t ON (((t.id = ts.team_id) AND (t.sport = ts.sport))))
     LEFT JOIN public.leagues l ON ((l.id = ts.league_id)))
  WHERE (ts.sport = 'FOOTBALL'::text);


--
-- Name: VIEW standings; Type: COMMENT; Schema: football; Owner: -
--

COMMENT ON VIEW football.standings IS 'Football standings. Order by sort_points DESC, sort_goal_diff DESC. Filter by season, league_id.';


--
-- Name: stat_definitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stat_definitions (
    id integer NOT NULL,
    sport text NOT NULL,
    key_name text NOT NULL,
    display_name text NOT NULL,
    entity_type text NOT NULL,
    category text,
    is_inverse boolean DEFAULT false NOT NULL,
    is_derived boolean DEFAULT false NOT NULL,
    is_percentile_eligible boolean DEFAULT false NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    unit text,
    comparable boolean DEFAULT false NOT NULL,
    rate_sibling boolean DEFAULT false NOT NULL,
    rate_base text,
    CONSTRAINT stat_definitions_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: COLUMN stat_definitions.rate_sibling; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_definitions.rate_sibling IS 'TRUE = the derived-stats trigger emits <key or rate_base><rate_modes.suffix> siblings.';


--
-- Name: COLUMN stat_definitions.rate_base; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_definitions.rate_base IS 'Legacy sibling base name when it differs from key_name (turnover→tov, shots_total→shots).';


--
-- Name: stat_definitions; Type: VIEW; Schema: football; Owner: -
--

CREATE VIEW football.stat_definitions AS
 SELECT id,
    key_name,
    display_name,
    entity_type,
    category,
    is_inverse,
    is_derived,
    is_percentile_eligible,
    sort_order
   FROM public.stat_definitions
  WHERE (sport = 'FOOTBALL'::text);


--
-- Name: VIEW stat_definitions; Type: COMMENT; Schema: football; Owner: -
--

COMMENT ON VIEW football.stat_definitions IS 'Football stat registry. Filter by entity_type.';


--
-- Name: team; Type: VIEW; Schema: football; Owner: -
--

CREATE VIEW football.team AS
 SELECT t.id,
    t.name,
    t.short_code,
    t.logo_url,
    t.country,
    t.city,
    t.founded,
    t.league_id,
    t.venue_name,
    t.venue_capacity,
        CASE
            WHEN (l.id IS NOT NULL) THEN json_build_object('id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url)
            ELSE NULL::json
        END AS league,
    ts.season,
    ts.stats,
    (ts.percentiles - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((ts.percentiles IS NOT NULL) AND ((ts.percentiles ->> '_sample_size'::text) IS NOT NULL)) THEN jsonb_build_object('sample_size', COALESCE(((ts.percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    ts.scoped_percentiles,
        CASE
            WHEN ((ts.scoped_percentiles IS NOT NULL) AND (ts.scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ts.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
   FROM ((public.teams t
     LEFT JOIN public.leagues l ON ((l.id = t.league_id)))
     LEFT JOIN public.team_stats ts ON (((ts.team_id = t.id) AND (ts.sport = t.sport))))
  WHERE (t.sport = 'FOOTBALL'::text);


--
-- Name: sports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sports (
    id text NOT NULL,
    display_name text NOT NULL,
    api_base_url text,
    current_season integer NOT NULL,
    is_active boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: autofill_entities; Type: MATERIALIZED VIEW; Schema: nba; Owner: -
--

CREATE MATERIALIZED VIEW nba.autofill_entities AS
 SELECT p.id,
    'player'::text AS type,
    p.name,
    p.first_name,
    p.last_name,
    p.nationality,
    (p.date_of_birth)::text AS date_of_birth,
    p.height,
    p.weight,
    p.photo_url,
    COALESCE(pct.team_id, p.team_id) AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos."position", 'Unknown'::text) AS "position",
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), public.unaccent(lower(p.first_name)), public.unaccent(lower(p.last_name)), public.unaccent(lower(replace(p.name, ' '::text, ''::text))), public.unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    (COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name)) AS meta
   FROM (((public.players p
     LEFT JOIN public.player_current_team pct ON (((pct.player_id = p.id) AND (pct.sport = p.sport))))
     LEFT JOIN public.teams t ON (((t.id = COALESCE(pct.team_id, p.team_id)) AND (t.sport = p.sport))))
     LEFT JOIN LATERAL ( SELECT ps."position"
           FROM public.player_stats ps
          WHERE ((ps.player_id = p.id) AND (ps.sport = p.sport))
          ORDER BY ps.season DESC NULLS LAST
         LIMIT 1) pos ON (true))
  WHERE ((p.sport = 'NBA'::text) AND ((EXISTS ( SELECT 1
           FROM public.player_stats ps
          WHERE ((ps.player_id = p.id) AND (ps.sport = p.sport)))) OR (((p.meta ->> 'draft_year'::text))::integer = ( SELECT sports.current_season
           FROM public.sports
          WHERE (sports.id = 'NBA'::text)))))
UNION ALL
 SELECT t.id,
    'team'::text AS type,
    t.name,
    NULL::text AS first_name,
    NULL::text AS last_name,
    t.country AS nationality,
    NULL::text AS date_of_birth,
    NULL::text AS height,
    NULL::text AS weight,
    t.logo_url AS photo_url,
    NULL::integer AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    NULL::text AS team_name,
    NULL::text AS team_logo_url,
    NULL::text AS "position",
    jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), public.unaccent(lower(replace(t.name, ' '::text, ''::text))), public.unaccent(lower(t.city))) AS search_tokens,
    jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'conference', t.conference, 'division', t.division, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
   FROM public.teams t
  WHERE (t.sport = 'NBA'::text)
  WITH NO DATA;


--
-- Name: player; Type: VIEW; Schema: nba; Owner: -
--

CREATE VIEW nba.player AS
 SELECT player_id AS id,
    season,
    league_id,
    team_id,
    "position",
    stats,
    ((percentiles - '_position_group'::text) - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((percentiles IS NOT NULL) AND ((percentiles ->> '_position_group'::text) IS NOT NULL)) THEN jsonb_build_object('position_group', (percentiles ->> '_position_group'::text), 'sample_size', COALESCE(((percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    scoped_percentiles,
        CASE
            WHEN ((scoped_percentiles IS NOT NULL) AND (scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ps.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    updated_at AS stats_updated_at
   FROM public.player_stats ps
  WHERE (sport = 'NBA'::text);


--
-- Name: VIEW player; Type: COMMENT; Schema: nba; Owner: -
--

COMMENT ON VIEW nba.player IS 'NBA player profile — stats only. Name and other meta come from the frontend local cache.';


--
-- Name: standings; Type: VIEW; Schema: nba; Owner: -
--

CREATE VIEW nba.standings AS
 SELECT ts.team_id,
    ts.season,
    ts.league_id,
    t.name AS team_name,
    t.short_code AS team_abbr,
    t.logo_url,
    t.conference,
    t.division,
    ts.stats,
    round((((ts.stats ->> 'wins'::text))::numeric / (NULLIF((((ts.stats ->> 'wins'::text))::integer + ((ts.stats ->> 'losses'::text))::integer), 0))::numeric), 3) AS win_pct
   FROM (public.team_stats ts
     JOIN public.teams t ON (((t.id = ts.team_id) AND (t.sport = ts.sport))))
  WHERE (ts.sport = 'NBA'::text);


--
-- Name: VIEW standings; Type: COMMENT; Schema: nba; Owner: -
--

COMMENT ON VIEW nba.standings IS 'NBA standings. Order by win_pct DESC. Filter by season, conference.';


--
-- Name: stat_definitions; Type: VIEW; Schema: nba; Owner: -
--

CREATE VIEW nba.stat_definitions AS
 SELECT id,
    key_name,
    display_name,
    entity_type,
    category,
    is_inverse,
    is_derived,
    is_percentile_eligible,
    sort_order
   FROM public.stat_definitions
  WHERE (sport = 'NBA'::text);


--
-- Name: VIEW stat_definitions; Type: COMMENT; Schema: nba; Owner: -
--

COMMENT ON VIEW nba.stat_definitions IS 'NBA stat registry. Filter by entity_type.';


--
-- Name: team; Type: VIEW; Schema: nba; Owner: -
--

CREATE VIEW nba.team AS
 SELECT t.id,
    t.name,
    t.short_code,
    t.logo_url,
    t.country,
    t.city,
    t.founded,
    t.league_id,
    t.conference,
    t.division,
    t.venue_name,
    t.venue_capacity,
    ts.season,
    ts.stats,
    (ts.percentiles - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((ts.percentiles IS NOT NULL) AND ((ts.percentiles ->> '_sample_size'::text) IS NOT NULL)) THEN jsonb_build_object('sample_size', COALESCE(((ts.percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    ts.scoped_percentiles,
        CASE
            WHEN ((ts.scoped_percentiles IS NOT NULL) AND (ts.scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ts.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
   FROM (public.teams t
     LEFT JOIN public.team_stats ts ON (((ts.team_id = t.id) AND (ts.sport = t.sport))))
  WHERE (t.sport = 'NBA'::text);


--
-- Name: autofill_entities; Type: MATERIALIZED VIEW; Schema: nfl; Owner: -
--

CREATE MATERIALIZED VIEW nfl.autofill_entities AS
 SELECT p.id,
    'player'::text AS type,
    p.name,
    p.first_name,
    p.last_name,
    p.nationality,
    (p.date_of_birth)::text AS date_of_birth,
    p.height,
    p.weight,
    p.photo_url,
    COALESCE(pct.team_id, p.team_id) AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos."position", 'Unknown'::text) AS "position",
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), public.unaccent(lower(p.first_name)), public.unaccent(lower(p.last_name)), public.unaccent(lower(replace(p.name, ' '::text, ''::text))), public.unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    (COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name)) AS meta
   FROM (((public.players p
     LEFT JOIN public.player_current_team pct ON (((pct.player_id = p.id) AND (pct.sport = p.sport))))
     LEFT JOIN public.teams t ON (((t.id = COALESCE(pct.team_id, p.team_id)) AND (t.sport = p.sport))))
     LEFT JOIN LATERAL ( SELECT ps."position"
           FROM public.player_stats ps
          WHERE ((ps.player_id = p.id) AND (ps.sport = p.sport))
          ORDER BY ps.season DESC NULLS LAST
         LIMIT 1) pos ON (true))
  WHERE ((p.sport = 'NFL'::text) AND ((EXISTS ( SELECT 1
           FROM public.player_stats ps
          WHERE ((ps.player_id = p.id) AND (ps.sport = p.sport)))) OR ((p.meta ->> 'experience'::text) ~~* 'rookie%'::text)))
UNION ALL
 SELECT t.id,
    'team'::text AS type,
    t.name,
    NULL::text AS first_name,
    NULL::text AS last_name,
    t.country AS nationality,
    NULL::text AS date_of_birth,
    NULL::text AS height,
    NULL::text AS weight,
    t.logo_url AS photo_url,
    NULL::integer AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    NULL::text AS team_name,
    NULL::text AS team_logo_url,
    NULL::text AS "position",
    jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), public.unaccent(lower(replace(t.name, ' '::text, ''::text))), public.unaccent(lower(t.city))) AS search_tokens,
    jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'conference', t.conference, 'division', t.division, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
   FROM public.teams t
  WHERE (t.sport = 'NFL'::text)
  WITH NO DATA;


--
-- Name: player; Type: VIEW; Schema: nfl; Owner: -
--

CREATE VIEW nfl.player AS
 SELECT player_id AS id,
    season,
    league_id,
    team_id,
    "position",
    stats,
    ((percentiles - '_position_group'::text) - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((percentiles IS NOT NULL) AND ((percentiles ->> '_position_group'::text) IS NOT NULL)) THEN jsonb_build_object('position_group', (percentiles ->> '_position_group'::text), 'sample_size', COALESCE(((percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    scoped_percentiles,
        CASE
            WHEN ((scoped_percentiles IS NOT NULL) AND (scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ps.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    updated_at AS stats_updated_at
   FROM public.player_stats ps
  WHERE (sport = 'NFL'::text);


--
-- Name: VIEW player; Type: COMMENT; Schema: nfl; Owner: -
--

COMMENT ON VIEW nfl.player IS 'NFL player profile — stats only. Name and other meta come from the frontend local cache.';


--
-- Name: standings; Type: VIEW; Schema: nfl; Owner: -
--

CREATE VIEW nfl.standings AS
 SELECT ts.team_id,
    ts.season,
    ts.league_id,
    t.name AS team_name,
    t.short_code AS team_abbr,
    t.logo_url,
    t.conference,
    t.division,
    ts.stats,
    round((((ts.stats ->> 'wins'::text))::numeric / (NULLIF(((((ts.stats ->> 'wins'::text))::integer + ((ts.stats ->> 'losses'::text))::integer) + COALESCE(((ts.stats ->> 'ties'::text))::integer, 0)), 0))::numeric), 3) AS win_pct
   FROM (public.team_stats ts
     JOIN public.teams t ON (((t.id = ts.team_id) AND (t.sport = ts.sport))))
  WHERE (ts.sport = 'NFL'::text);


--
-- Name: VIEW standings; Type: COMMENT; Schema: nfl; Owner: -
--

COMMENT ON VIEW nfl.standings IS 'NFL standings. Order by win_pct DESC. Filter by season, conference, division.';


--
-- Name: stat_definitions; Type: VIEW; Schema: nfl; Owner: -
--

CREATE VIEW nfl.stat_definitions AS
 SELECT id,
    key_name,
    display_name,
    entity_type,
    category,
    is_inverse,
    is_derived,
    is_percentile_eligible,
    sort_order
   FROM public.stat_definitions
  WHERE (sport = 'NFL'::text);


--
-- Name: VIEW stat_definitions; Type: COMMENT; Schema: nfl; Owner: -
--

COMMENT ON VIEW nfl.stat_definitions IS 'NFL stat registry. Filter by entity_type.';


--
-- Name: team; Type: VIEW; Schema: nfl; Owner: -
--

CREATE VIEW nfl.team AS
 SELECT t.id,
    t.name,
    t.short_code,
    t.logo_url,
    t.country,
    t.city,
    t.founded,
    t.league_id,
    t.conference,
    t.division,
    t.venue_name,
    t.venue_capacity,
    ts.season,
    ts.stats,
    (ts.percentiles - '_sample_size'::text) AS percentiles,
        CASE
            WHEN ((ts.percentiles IS NOT NULL) AND ((ts.percentiles ->> '_sample_size'::text) IS NOT NULL)) THEN jsonb_build_object('sample_size', COALESCE(((ts.percentiles ->> '_sample_size'::text))::integer, 0))
            ELSE NULL::jsonb
        END AS percentile_metadata,
    ts.scoped_percentiles,
        CASE
            WHEN ((ts.scoped_percentiles IS NOT NULL) AND (ts.scoped_percentiles <> '{}'::jsonb)) THEN jsonb_build_object('scopes', ( SELECT COALESCE(jsonb_agg(k.k ORDER BY k.k), '[]'::jsonb) AS "coalesce"
               FROM jsonb_object_keys(ts.scoped_percentiles) k(k)))
            ELSE NULL::jsonb
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
   FROM (public.teams t
     LEFT JOIN public.team_stats ts ON (((ts.team_id = t.id) AND (ts.sport = t.sport))))
  WHERE (t.sport = 'NFL'::text);


--
-- Name: auth_refresh_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_refresh_tokens (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    token_hash text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone
);


--
-- Name: cognition_ledger; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cognition_ledger (
    id bigint NOT NULL,
    stage text NOT NULL,
    lens text NOT NULL,
    role text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    pair_entity_type text,
    pair_entity_id integer,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT 'null'::jsonb NOT NULL,
    product_table text NOT NULL,
    product_row_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    output_contract_version text NOT NULL,
    input_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    input_hash text,
    request_body jsonb,
    built_prompt text,
    included_evidence jsonb DEFAULT '[]'::jsonb NOT NULL,
    excluded_evidence jsonb DEFAULT '[]'::jsonb NOT NULL,
    context_budget jsonb DEFAULT '{}'::jsonb NOT NULL,
    parser_outcome text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT cognition_ledger_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT cognition_ledger_pair_entity_type_check CHECK (((pair_entity_type IS NULL) OR (pair_entity_type = ANY (ARRAY['player'::text, 'team'::text]))))
);


--
-- Name: TABLE cognition_ledger; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.cognition_ledger IS 'Diagnostic ledger for model-backed cognition lens calls. Product tables are still the served surface; this table records request/evidence/parser provenance for post-hoc failure analysis and fixture capture.';


--
-- Name: COLUMN cognition_ledger.output_contract_version; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.cognition_ledger.output_contract_version IS 'Parser/output schema version, distinct from prompt_version. Use this when a reply shape changes without changing the input prompt contract.';


--
-- Name: COLUMN cognition_ledger.included_evidence; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.cognition_ledger.included_evidence IS 'JSON summary of evidence sent to the model, including source ids and stage-specific context.';


--
-- Name: COLUMN cognition_ledger.excluded_evidence; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.cognition_ledger.excluded_evidence IS 'JSON summary of evidence considered but excluded, with reason when known.';


--
-- Name: COLUMN cognition_ledger.context_budget; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.cognition_ledger.context_budget IS 'JSON budget telemetry, e.g. available generation tokens and evaluated tokens when a model call happened.';


--
-- Name: cognition_ledger_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.cognition_ledger_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: cognition_ledger_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.cognition_ledger_id_seq OWNED BY public.cognition_ledger.id;


--
-- Name: event_box_scores; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.event_box_scores (
    id bigint NOT NULL,
    fixture_id integer NOT NULL,
    player_id integer NOT NULL,
    team_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer DEFAULT 0 NOT NULL,
    minutes_played numeric,
    stats jsonb DEFAULT '{}'::jsonb NOT NULL,
    raw_response jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    "position" text,
    percentiles jsonb DEFAULT '{}'::jsonb NOT NULL,
    composite_score numeric,
    rating_composite numeric,
    rating_specialist numeric,
    rating_specialty text,
    rating_composite_pct numeric,
    rating_specialist_pct numeric
);


--
-- Name: event_box_scores_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.event_box_scores_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: event_box_scores_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.event_box_scores_id_seq OWNED BY public.event_box_scores.id;


--
-- Name: event_team_stats; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.event_team_stats (
    id bigint NOT NULL,
    fixture_id integer NOT NULL,
    team_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer DEFAULT 0 NOT NULL,
    score integer,
    stats jsonb DEFAULT '{}'::jsonb NOT NULL,
    raw_response jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    percentiles jsonb DEFAULT '{}'::jsonb NOT NULL,
    composite_score numeric,
    rating_composite numeric,
    rating_specialist numeric,
    rating_specialty text,
    rating_composite_pct numeric,
    rating_specialist_pct numeric
);


--
-- Name: event_team_stats_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.event_team_stats_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: event_team_stats_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.event_team_stats_id_seq OWNED BY public.event_team_stats.id;


--
-- Name: fixtures; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fixtures (
    id integer NOT NULL,
    external_id integer,
    sport text NOT NULL,
    league_id integer,
    season integer NOT NULL,
    home_team_id integer NOT NULL,
    away_team_id integer NOT NULL,
    start_time timestamp with time zone NOT NULL,
    venue_name text,
    round text,
    status text DEFAULT 'scheduled'::text NOT NULL,
    seed_delay_hours integer DEFAULT 4 NOT NULL,
    seeded_at timestamp with time zone,
    seed_attempts integer DEFAULT 0,
    last_seed_error text,
    home_score integer,
    away_score integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    last_incomplete_reason text,
    CONSTRAINT different_teams CHECK ((home_team_id <> away_team_id)),
    CONSTRAINT fixtures_status_check CHECK ((status = ANY (ARRAY['scheduled'::text, 'in_progress'::text, 'completed'::text, 'seeded'::text, 'cancelled'::text, 'postponed'::text])))
);


--
-- Name: COLUMN fixtures.last_incomplete_reason; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.fixtures.last_incomplete_reason IS 'Why the most recent provider payload failed the completeness contract (seed/services/event/completeness.py). Distinct from last_seed_error, which records transport/processing failures. Cleared by mark_fixture_seeded().';


--
-- Name: fixtures_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fixtures_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fixtures_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fixtures_id_seq OWNED BY public.fixtures.id;


--
-- Name: momentum_scores; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.momentum_scores (
    id bigint NOT NULL,
    sport text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    season integer,
    league_id integer,
    team_id integer,
    "position" text,
    position_group text,
    conference text,
    division text,
    vibe_slope numeric,
    vibe_samples integer DEFAULT 0 NOT NULL,
    vibe_window_start timestamp with time zone,
    vibe_window_end timestamp with time zone,
    rating_slope numeric,
    rating_samples integer DEFAULT 0 NOT NULL,
    rating_window_start timestamp with time zone,
    rating_window_end timestamp with time zone,
    momentum_score numeric,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT momentum_scores_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: TABLE momentum_scores; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.momentum_scores IS 'Durable Momentum snapshots (append-only). vibe_slope: 21 calendar days. rating_slope: the entity''s last season_bridge_window(sport) rated games (~10% of season, the same schedule as the percentile cold-start guard), a game-count lookback that spans the season boundary and cannot be starved by bye weeks. momentum_score: SIGNED average of the present slopes (falls recorded, not clamped). Retention: full resolution 30 days, then thinned to the last snapshot per entity per day by the Go cleanup ticker.';


--
-- Name: latest_momentum_scores_per_entity; Type: MATERIALIZED VIEW; Schema: public; Owner: -
--

CREATE MATERIALIZED VIEW public.latest_momentum_scores_per_entity AS
 SELECT DISTINCT ON (sport, entity_type, entity_id) id,
    sport,
    entity_type,
    entity_id,
    season,
    league_id,
    team_id,
    "position",
    position_group,
    conference,
    division,
    vibe_slope,
    vibe_samples,
    vibe_window_start,
    vibe_window_end,
    rating_slope,
    rating_samples,
    rating_window_start,
    rating_window_end,
    momentum_score,
    generated_at
   FROM public.momentum_scores
  ORDER BY sport, entity_type, entity_id, generated_at DESC
  WITH NO DATA;


--
-- Name: MATERIALIZED VIEW latest_momentum_scores_per_entity; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON MATERIALIZED VIEW public.latest_momentum_scores_per_entity IS 'Current-row projection for momentum_scores. D1 latest-row read optimization: one row per (sport, entity_type, entity_id), refreshed by a statement trigger after momentum_scores changes so /leaderboard/momentum does not sort the full append-only history on every read.';


--
-- Name: meta; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meta (
    key text NOT NULL,
    value text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: metadata_refresh_queue; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.metadata_refresh_queue (
    id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL,
    season integer,
    reason text,
    priority integer DEFAULT 1,
    requested_at timestamp with time zone DEFAULT now(),
    processed_at timestamp with time zone,
    retry_count integer DEFAULT 0,
    error_message text
);


--
-- Name: TABLE metadata_refresh_queue; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.metadata_refresh_queue IS 'Queue for player metadata refresh requests. Populated by trigger on event_box_scores.';


--
-- Name: metadata_queue_status; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.metadata_queue_status AS
 SELECT sport,
    count(*) FILTER (WHERE (processed_at IS NULL)) AS pending,
    count(*) FILTER (WHERE ((processed_at IS NULL) AND (priority = 1))) AS high_priority,
    count(*) FILTER (WHERE ((processed_at IS NULL) AND (priority = 2))) AS normal_priority,
    count(*) FILTER (WHERE ((processed_at IS NOT NULL) AND (error_message IS NOT NULL))) AS failed,
    count(*) FILTER (WHERE ((processed_at IS NOT NULL) AND (error_message IS NULL))) AS successful,
    min(requested_at) FILTER (WHERE (processed_at IS NULL)) AS oldest_pending,
    max(processed_at) FILTER (WHERE (processed_at IS NOT NULL)) AS last_processed
   FROM public.metadata_refresh_queue
  GROUP BY sport;


--
-- Name: metadata_refresh_queue_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.metadata_refresh_queue_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: metadata_refresh_queue_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.metadata_refresh_queue_id_seq OWNED BY public.metadata_refresh_queue.id;


--
-- Name: metadata_sync_log; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.metadata_sync_log (
    id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL,
    last_sync_at timestamp with time zone,
    metadata_version integer DEFAULT 1,
    sync_source text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);


--
-- Name: TABLE metadata_sync_log; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.metadata_sync_log IS 'Records when player metadata was last synchronized from APIs.';


--
-- Name: metadata_sync_log_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.metadata_sync_log_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: metadata_sync_log_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.metadata_sync_log_id_seq OWNED BY public.metadata_sync_log.id;


--
-- Name: momentum_refresh_needed; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.momentum_refresh_needed (
    sport text NOT NULL,
    reason text,
    first_marked_at timestamp with time zone DEFAULT now() NOT NULL,
    last_marked_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE momentum_refresh_needed; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.momentum_refresh_needed IS 'Durable dirty-sport queue for Momentum snapshots. Upstream Vibe/event-rating changes mark a sport dirty; the API listener drains only pending rows into momentum_scores.';


--
-- Name: momentum_scores_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.momentum_scores_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: momentum_scores_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.momentum_scores_id_seq OWNED BY public.momentum_scores.id;


--
-- Name: momentum_summaries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.momentum_summaries (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    trigger_type text DEFAULT 'periodic'::text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    direction text,
    score smallint,
    blurb text,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT momentum_summaries_direction_check CHECK (((direction IS NULL) OR (direction = ANY (ARRAY['rising'::text, 'falling'::text, 'steady'::text])))),
    CONSTRAINT momentum_summaries_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT momentum_summaries_score_check CHECK (((score IS NULL) OR ((score >= '-5'::integer) AND (score <= 5))))
);


--
-- Name: TABLE momentum_summaries; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.momentum_summaries IS 'Generated Momentum trajectory cards. Numeric trajectory remains in momentum_scores; this table stores the first-class direction/score/blurb product consumed by Sigil.';


--
-- Name: momentum_summaries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.momentum_summaries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: momentum_summaries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.momentum_summaries_id_seq OWNED BY public.momentum_summaries.id;


--
-- Name: narrative_episodes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_episodes (
    id bigint NOT NULL,
    sport text NOT NULL,
    link_type text NOT NULL,
    subject_type text NOT NULL,
    subject_id integer NOT NULL,
    object_type text NOT NULL,
    object_id integer NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    outcome text,
    season integer,
    started_at timestamp with time zone NOT NULL,
    peaked_at timestamp with time zone NOT NULL,
    ended_at timestamp with time zone,
    peak_strength smallint NOT NULL,
    last_strength smallint,
    article_count integer DEFAULT 0 NOT NULL,
    distinct_sources integer DEFAULT 0 NOT NULL,
    event_count integer DEFAULT 0 NOT NULL,
    peak_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT narrative_episodes_last_strength_check CHECK (((last_strength IS NULL) OR ((last_strength >= 0) AND (last_strength <= 100)))),
    CONSTRAINT narrative_episodes_link_type_check CHECK ((link_type = ANY (ARRAY['co_mention'::text, 'trade_rumor'::text, 'trade_confirmed'::text, 'injury'::text, 'contract_dispute'::text, 'praise'::text, 'criticism'::text]))),
    CONSTRAINT narrative_episodes_no_self_loop_check CHECK ((NOT ((subject_type = object_type) AND (subject_id = object_id)))),
    CONSTRAINT narrative_episodes_object_type_check CHECK ((object_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text]))),
    CONSTRAINT narrative_episodes_open_ended_check CHECK (((status = 'open'::text) = (ended_at IS NULL))),
    CONSTRAINT narrative_episodes_open_outcome_check CHECK (((status = 'open'::text) = (outcome IS NULL))),
    CONSTRAINT narrative_episodes_outcome_check CHECK (((outcome IS NULL) OR (outcome = ANY (ARRAY['confirmed'::text, 'fizzled'::text])))),
    CONSTRAINT narrative_episodes_peak_strength_check CHECK (((peak_strength >= 0) AND (peak_strength <= 100))),
    CONSTRAINT narrative_episodes_status_check CHECK ((status = ANY (ARRAY['open'::text, 'sealed'::text]))),
    CONSTRAINT narrative_episodes_subject_type_check CHECK ((subject_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text])))
);


--
-- Name: TABLE narrative_episodes; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_episodes IS 'Bounded, outcome-tagged spans of narrative association — the graph''s long-term memory ("this is a place team A flirted with back in 2025"). Opened/peaked/sealed by roll_narrative_episodes() over narrative_links state; links forget by design, episodes are what remain. One open episode per edge (partial unique index); sealed rows accumulate per edge across seasons. outcome=confirmed is reserved for the event-driven path (e.g. a trade_confirmed narrative_event sealing a trade_rumor episode) — the mechanical quiet-seal only ever writes fizzled.';


--
-- Name: COLUMN narrative_episodes.season; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_episodes.season IS 'sports.current_season at open time — the era anchor for "back in 2025" retrieval. A span crossing seasons keeps its opening season.';


--
-- Name: narrative_episodes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.narrative_episodes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: narrative_episodes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.narrative_episodes_id_seq OWNED BY public.narrative_episodes.id;


--
-- Name: narrative_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_events (
    id bigint NOT NULL,
    sport text NOT NULL,
    subject_type text NOT NULL,
    subject_id integer NOT NULL,
    predicate text NOT NULL,
    object_type text,
    object_id integer,
    sentiment numeric(3,2),
    confidence text NOT NULL,
    article_id bigint NOT NULL,
    event_date timestamp with time zone NOT NULL,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    extracted_at timestamp with time zone DEFAULT now() NOT NULL,
    source text,
    CONSTRAINT narrative_events_confidence_check CHECK ((confidence = ANY (ARRAY['speculative'::text, 'reported'::text, 'confirmed'::text]))),
    CONSTRAINT narrative_events_object_pair_check CHECK (((object_type IS NULL) = (object_id IS NULL))),
    CONSTRAINT narrative_events_object_type_check CHECK (((object_type IS NULL) OR (object_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text])))),
    CONSTRAINT narrative_events_predicate_check CHECK ((predicate = ANY (ARRAY['trade_rumor'::text, 'trade_confirmed'::text, 'injury'::text, 'contract_dispute'::text, 'praise'::text, 'criticism'::text]))),
    CONSTRAINT narrative_events_sentiment_check CHECK (((sentiment IS NULL) OR ((sentiment >= '-1.0'::numeric) AND (sentiment <= 1.0)))),
    CONSTRAINT narrative_events_subject_type_check CHECK ((subject_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text])))
);


--
-- Name: TABLE narrative_events; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_events IS 'Append-only atomic narrative relations extracted from articles: the trajectory series is a time-ordered scan of this table per endpoint (or endpoint pair) — the graph''s event history. Deliberately small predicate vocabulary: precision over coverage; grow it by migration, with eval evidence, never by loosening the prompt alone.';


--
-- Name: COLUMN narrative_events.article_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_events.article_id IS 'Soft reference to news_articles (no FK since mig 155): the rail is prunable feedstock, the event is durable memory. Join tolerantly.';


--
-- Name: COLUMN narrative_events.event_date; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_events.event_date IS 'The article''s published_at, denormalized so trajectory scans never join the rail.';


--
-- Name: COLUMN narrative_events.source; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_events.source IS 'Denormalized news_articles.source at extraction time, so attribution survives rail pruning.';


--
-- Name: narrative_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.narrative_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: narrative_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.narrative_events_id_seq OWNED BY public.narrative_events.id;


--
-- Name: narrative_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_links (
    sport text NOT NULL,
    link_type text NOT NULL,
    subject_type text NOT NULL,
    subject_id integer NOT NULL,
    object_type text NOT NULL,
    object_id integer NOT NULL,
    strength smallint,
    components jsonb DEFAULT '{}'::jsonb NOT NULL,
    weighted_sentiment numeric(4,3),
    confidence text,
    event_count integer DEFAULT 0 NOT NULL,
    article_count integer DEFAULT 0 NOT NULL,
    distinct_sources integer DEFAULT 0 NOT NULL,
    first_seen_at timestamp with time zone,
    last_event_at timestamp with time zone,
    trajectory text DEFAULT 'developing_story'::text NOT NULL,
    trajectory_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT narrative_links_co_mention_canonical_check CHECK (((link_type <> 'co_mention'::text) OR (subject_type < object_type) OR ((subject_type = object_type) AND (subject_id < object_id)))),
    CONSTRAINT narrative_links_confidence_check CHECK (((confidence IS NULL) OR (confidence = ANY (ARRAY['speculative'::text, 'reported'::text, 'confirmed'::text])))),
    CONSTRAINT narrative_links_link_type_check CHECK ((link_type = ANY (ARRAY['co_mention'::text, 'trade_rumor'::text, 'trade_confirmed'::text, 'injury'::text, 'contract_dispute'::text, 'praise'::text, 'criticism'::text]))),
    CONSTRAINT narrative_links_no_self_loop_check CHECK ((NOT ((subject_type = object_type) AND (subject_id = object_id)))),
    CONSTRAINT narrative_links_object_type_check CHECK ((object_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text]))),
    CONSTRAINT narrative_links_sentiment_check CHECK (((weighted_sentiment IS NULL) OR ((weighted_sentiment >= '-1.0'::numeric) AND (weighted_sentiment <= 1.0)))),
    CONSTRAINT narrative_links_strength_check CHECK (((strength IS NULL) OR ((strength >= 0) AND (strength <= 100)))),
    CONSTRAINT narrative_links_subject_type_check CHECK ((subject_type = ANY (ARRAY['player'::text, 'team'::text, 'person'::text]))),
    CONSTRAINT narrative_links_trajectory_check CHECK ((trajectory = ANY (ARRAY['developing_story'::text, 'heating_up'::text, 'cooling_off'::text])))
);


--
-- Name: TABLE narrative_links; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_links IS 'The narrative graph''s edge state: one row per (subject, object, link_type) with strength 0-100 + components breakdown (compute_transfer_heat house pattern) and the shared ±10-bucket trajectory vocabulary. co_mention rows are mechanical derivations from the vetted news rail (refresh_co_mention_links); typed rows aggregate narrative_events. Directed as stored; co_mention is symmetric and canonically ordered. A player''s own-team edge is legitimately strong — display layers filter current-roster pairs via player_current_identity, the graph does not.';


--
-- Name: narrative_person_affiliations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_person_affiliations (
    id integer NOT NULL,
    person_id integer NOT NULL,
    sport text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    role text NOT NULL,
    season integer,
    valid_from timestamp with time zone DEFAULT now() NOT NULL,
    valid_until timestamp with time zone,
    is_current boolean DEFAULT true NOT NULL,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT narrative_person_affiliations_current_check CHECK ((is_current = (valid_until IS NULL))),
    CONSTRAINT narrative_person_affiliations_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT narrative_person_affiliations_role_check CHECK ((role = ANY (ARRAY['coach'::text, 'agent'::text, 'executive'::text, 'family'::text, 'other'::text])))
);


--
-- Name: TABLE narrative_person_affiliations; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_person_affiliations IS 'Interval-valued affiliation history for news-derived people, mirroring player_team_history ("he coached team B in 2019"). Entity-generic: a coach ties to a team, an agent ties to a player. The narrative_persons.team_id trigger maintains the automatic now-path; backdated intervals from extraction evidence insert directly with explicit valid_from/valid_until.';


--
-- Name: narrative_person_affiliations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.narrative_person_affiliations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: narrative_person_affiliations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.narrative_person_affiliations_id_seq OWNED BY public.narrative_person_affiliations.id;


--
-- Name: narrative_person_mentions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_person_mentions (
    article_id bigint NOT NULL,
    person_id integer NOT NULL,
    sport text NOT NULL,
    match_confidence numeric(4,3),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE narrative_person_mentions; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_person_mentions IS 'Article-to-person links, the person analogue of news_article_entities. Kept separate so the existing player/team scrub + derive-trigger machinery is untouched; a later migration may unify the rails once persons need those stages.';


--
-- Name: narrative_persons; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.narrative_persons (
    id integer NOT NULL,
    sport text NOT NULL,
    kind text NOT NULL,
    name text NOT NULL,
    aliases text[] DEFAULT '{}'::text[] NOT NULL,
    team_id integer,
    status text DEFAULT 'candidate'::text NOT NULL,
    merged_into integer,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    mention_count integer DEFAULT 0 NOT NULL,
    distinct_sources integer DEFAULT 0 NOT NULL,
    first_seen_at timestamp with time zone,
    last_seen_at timestamp with time zone,
    model_version text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT narrative_persons_kind_check CHECK ((kind = ANY (ARRAY['coach'::text, 'agent'::text, 'executive'::text, 'family'::text, 'other'::text]))),
    CONSTRAINT narrative_persons_merged_check CHECK (((status = 'merged'::text) = (merged_into IS NOT NULL))),
    CONSTRAINT narrative_persons_status_check CHECK ((status = ANY (ARRAY['candidate'::text, 'active'::text, 'merged'::text, 'retired'::text])))
);


--
-- Name: TABLE narrative_persons; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.narrative_persons IS 'News-derived people (coaches, agents, executives) absent from provider seeding. Extraction creates candidates; promotion to active requires recurring multi-source evidence (thresholds live in the promotion step, not here). Same-name collisions are legal (two Mike Browns) — disambiguation is the resolve gate''s job, merges via status=merged + merged_into.';


--
-- Name: COLUMN narrative_persons.team_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_persons.team_id IS 'Current primary affiliation derived from news evidence (nullable — agents float free).';


--
-- Name: COLUMN narrative_persons.evidence; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.narrative_persons.evidence IS 'Accumulated discovery evidence: mention/source tallies, team-affiliation votes, sample article ids — the promotion decision''s input, in one inspectable place.';


--
-- Name: narrative_persons_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.narrative_persons_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: narrative_persons_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.narrative_persons_id_seq OWNED BY public.narrative_persons.id;


--
-- Name: news_article_entities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news_article_entities (
    article_id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    match_confidence numeric(4,3),
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    title_pos smallint,
    vetted boolean,
    scrubbed_at timestamp with time zone,
    CONSTRAINT news_article_entities_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: COLUMN news_article_entities.title_pos; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_article_entities.title_pos IS 'Character offset of this entity in the article title (thirdparty.FirstMatchPos); NULL = unknown. Powers the co-mention proximity gate (migration 033).';


--
-- Name: COLUMN news_article_entities.vetted; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_article_entities.vetted IS 'Model scrub verdict: TRUE genuinely the subject, FALSE fuzzy false positive, NULL not yet scrubbed. See the cognition scrub stage.';


--
-- Name: COLUMN news_article_entities.scrubbed_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_article_entities.scrubbed_at IS 'When the model scrub last judged this link (NULL = unscrubbed). Drives the async scrub worker backlog query.';


--
-- Name: news_articles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news_articles (
    id bigint NOT NULL,
    url_hash text NOT NULL,
    url text NOT NULL,
    source text,
    title text NOT NULL,
    description text,
    published_at timestamp with time zone,
    fetched_at timestamp with time zone DEFAULT now() NOT NULL,
    raw jsonb DEFAULT '{}'::jsonb NOT NULL,
    bucket text,
    topic_heat integer,
    CONSTRAINT news_articles_bucket_check CHECK (((bucket IS NULL) OR (bucket = ANY (ARRAY['transfer'::text, 'non_transfer'::text])))),
    CONSTRAINT news_articles_topic_heat_check CHECK (((topic_heat IS NULL) OR (topic_heat >= 1)))
);


--
-- Name: COLUMN news_articles.bucket; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_articles.bucket IS 'Wave 5 scrub bucket: transfer or non_transfer. NULL means pre-backfill/unknown; downstream reads keep NULL lenient during transition.';


--
-- Name: COLUMN news_articles.topic_heat; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_articles.topic_heat IS 'Wave 5 topic heat-rank: size of the article''s same-day embedding topic cluster, recomputed idempotently by the Rust cognition worker.';


--
-- Name: news_articles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.news_articles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: news_articles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.news_articles_id_seq OWNED BY public.news_articles.id;


--
-- Name: news_summaries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news_summaries (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    body text,
    impact smallint,
    impact_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text,
    prompt_version text,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    narrative_title text,
    narrative_updated_at timestamp with time zone,
    source_count integer DEFAULT 0 NOT NULL,
    source_names text[] DEFAULT '{}'::text[] NOT NULL,
    source_latest_at timestamp with time zone,
    source_oldest_at timestamp with time zone,
    trajectory text DEFAULT 'developing_story'::text NOT NULL,
    trajectory_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    CONSTRAINT news_summaries_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT news_summaries_impact_check CHECK (((impact IS NULL) OR ((impact >= 0) AND (impact <= 100)))),
    CONSTRAINT news_summaries_trajectory_check CHECK ((trajectory = ANY (ARRAY['developing_story'::text, 'heating_up'::text, 'cooling_off'::text]))),
    CONSTRAINT news_summaries_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['news_spike'::text, 'periodic'::text, 'manual'::text])))
);


--
-- Name: TABLE news_summaries; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.news_summaries IS 'Append, ONE ROW PER NARRATIVE per generation. A generation = the rows sharing (entity_type, entity_id, sport, generated_at). narrative_title + body = the write-up; per-narrative impact ranks the leaderboard; NULL narrative_title/body = a no-narratives-this-cycle marker. Written by ml/news_narratives.go.';


--
-- Name: COLUMN news_summaries.body; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.body IS 'The narrative write-up (original prose); NULL = no-narratives marker row.';


--
-- Name: COLUMN news_summaries.impact; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.impact IS 'Per-narrative deterministic impact 0-100 (volume×sources×recency over this narrative''s articles); ranks the news leaderboard.';


--
-- Name: COLUMN news_summaries.narrative_title; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.narrative_title IS 'Storyline headline (e.g. "Cucurella to Real Madrid saga"); NULL = no-narratives marker row.';


--
-- Name: COLUMN news_summaries.narrative_updated_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.narrative_updated_at IS 'Narrative freshness timestamp for the News hub. Usually the newest cited source timestamp; falls back to generated_at.';


--
-- Name: COLUMN news_summaries.source_count; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.source_count IS 'Number of cited source articles behind this narrative row.';


--
-- Name: COLUMN news_summaries.source_names; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.source_names IS 'Distinct source names from the cited articles, preserved for provenance and client display.';


--
-- Name: COLUMN news_summaries.trajectory; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.trajectory IS 'Narrative trajectory marker: developing_story, heating_up, or cooling_off.';


--
-- Name: COLUMN news_summaries.trajectory_components; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.trajectory_components IS 'Transparent inputs behind the trajectory marker, usually impact delta versus the previous matching narrative.';


--
-- Name: COLUMN news_summaries.input_hash; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.news_summaries.input_hash IS 'SHA-256 (128-bit hex prefix) of the generation''s material inputs (vetted corpus article ids + transfer-heat facts); the Rust harness debounce key. NULL on pre-145 rows and markers written before the gate.';


--
-- Name: news_summaries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.news_summaries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: news_summaries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.news_summaries_id_seq OWNED BY public.news_summaries.id;


--
-- Name: news_summaries_shadow; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news_summaries_shadow (
    id bigint NOT NULL,
    source text DEFAULT 'rust'::text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT 'null'::jsonb NOT NULL,
    narrative_title text,
    body text,
    impact smallint,
    impact_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    original_corpus_size integer,
    deduped_corpus_size integer,
    model_version text,
    prompt_version text NOT NULL,
    temperature real NOT NULL,
    built_prompt text,
    ollama_request jsonb,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT news_summaries_shadow_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT news_summaries_shadow_impact_check CHECK (((impact IS NULL) OR ((impact >= 0) AND (impact <= 100)))),
    CONSTRAINT news_summaries_shadow_source_check CHECK ((source = ANY (ARRAY['rust'::text, 'go'::text])))
);


--
-- Name: TABLE news_summaries_shadow; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.news_summaries_shadow IS 'Rust Cognition Harness L13 narratives parity harness — offline shadow of news_summaries; holds source=rust and source=go rows. Diff built_prompt + ollama_request + model_version + prompt_version. Drop after narratives cutover.';


--
-- Name: news_summaries_shadow_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.news_summaries_shadow_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: news_summaries_shadow_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.news_summaries_shadow_id_seq OWNED BY public.news_summaries_shadow.id;


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id integer NOT NULL,
    user_id uuid NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    fixture_id integer,
    stat_key text NOT NULL,
    percentile numeric NOT NULL,
    message text NOT NULL,
    status text DEFAULT 'scheduled'::text NOT NULL,
    scheduled_for timestamp with time zone NOT NULL,
    sent_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT notifications_status_check CHECK ((status = ANY (ARRAY['scheduled'::text, 'sending'::text, 'sent'::text, 'failed'::text])))
);


--
-- Name: notifications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.notifications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notifications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.notifications_id_seq OWNED BY public.notifications.id;


--
-- Name: oracle_readings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oracle_readings (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    trigger_type text DEFAULT 'periodic'::text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    reading text,
    omen text,
    sigil_score smallint,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT oracle_readings_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT oracle_readings_omen_check CHECK (((omen IS NULL) OR (omen = ANY (ARRAY['ascendant'::text, 'steady'::text, 'waning'::text, 'crossroads'::text])))),
    CONSTRAINT oracle_readings_sigil_score_check CHECK (((sigil_score IS NULL) OR ((sigil_score >= 1) AND (sigil_score <= 100))))
);


--
-- Name: TABLE oracle_readings; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.oracle_readings IS 'FROZEN read-only history (Session C, 2026-07-16). The Oracle voice lives on sigil_synthesis (reading/omen/voiced_* — mig 152) since the sigil-voice merge; the latest real reading per serving scope was copied there by mig 153. No reader, no writer. Retained for provenance; candidate for DROP in a later season.';


--
-- Name: oracle_readings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oracle_readings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oracle_readings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oracle_readings_id_seq OWNED BY public.oracle_readings.id;


--
-- Name: peer_cohort_aggregate; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.peer_cohort_aggregate (
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer NOT NULL,
    entity_type text NOT NULL,
    "position" text DEFAULT ''::text NOT NULL,
    key_sums jsonb DEFAULT '{}'::jsonb NOT NULL,
    key_cnts jsonb DEFAULT '{}'::jsonb NOT NULL,
    score_sum numeric,
    score_cnt integer DEFAULT 0 NOT NULL,
    member_count integer DEFAULT 0 NOT NULL,
    computed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT peer_cohort_aggregate_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: TABLE peer_cohort_aggregate; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.peer_cohort_aggregate IS 'O1: precomputed per-cohort season aggregates for /momentum. Stores full-cohort SUMS+COUNTS so the read reconstructs exact leave-one-out peer averages. Refreshed by refresh_peer_cohort_aggregates().';


--
-- Name: pipeline_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pipeline_runs (
    id bigint NOT NULL,
    job text NOT NULL,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    finished_at timestamp with time zone,
    source_commit text,
    status text DEFAULT 'running'::text NOT NULL,
    attempted integer DEFAULT 0 NOT NULL,
    succeeded integer DEFAULT 0 NOT NULL,
    skipped integer DEFAULT 0 NOT NULL,
    failed integer DEFAULT 0 NOT NULL,
    error text,
    CONSTRAINT pipeline_runs_status_check CHECK ((status = ANY (ARRAY['running'::text, 'success'::text, 'partial'::text, 'failed'::text, 'skipped'::text])))
);


--
-- Name: TABLE pipeline_runs; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.pipeline_runs IS 'Append-only per-run record of the backend batch jobs (FIRST-GPT-AUDIT Session 13). One row per cmd/pipeline | cmd/statcommentary | cmd/vibesynth invocation: source commit, outcome (running/success/partial/failed/skipped), attempted/succeeded/skipped/failed counts, and a summarized error. The run-level companion to pipeline_work (the per-entity queue).';


--
-- Name: pipeline_runs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.pipeline_runs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: pipeline_runs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.pipeline_runs_id_seq OWNED BY public.pipeline_runs.id;


--
-- Name: pipeline_runs_latest; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.pipeline_runs_latest AS
 SELECT DISTINCT ON (job) job,
    id,
    status,
    started_at,
    finished_at,
    (EXTRACT(epoch FROM (COALESCE(finished_at, now()) - started_at)))::integer AS duration_s,
    source_commit,
    attempted,
    succeeded,
    skipped,
    failed,
    error
   FROM public.pipeline_runs
  ORDER BY job, started_at DESC;


--
-- Name: pipeline_stats; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pipeline_stats (
    id bigint NOT NULL,
    sport text NOT NULL,
    snapshot_date date NOT NULL,
    total_articles integer DEFAULT 0 NOT NULL,
    new_articles integer DEFAULT 0 NOT NULL,
    entities_with_summary integer DEFAULT 0 NOT NULL,
    entities_with_vibe integer DEFAULT 0 NOT NULL,
    transfer_rumors_active integer DEFAULT 0 NOT NULL,
    coverage_pct numeric(5,2),
    median_staleness_hours numeric(8,2),
    metrics jsonb DEFAULT '{}'::jsonb NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: pipeline_stats_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.pipeline_stats_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: pipeline_stats_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.pipeline_stats_id_seq OWNED BY public.pipeline_stats.id;


--
-- Name: pipeline_work; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pipeline_work (
    stage text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    available_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text,
    input_version text,
    CONSTRAINT pipeline_work_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text, 'article'::text]))),
    CONSTRAINT pipeline_work_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'failed'::text])))
);


--
-- Name: TABLE pipeline_work; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.pipeline_work IS 'Durable per-entity derivation work queue (FIRST-GPT-AUDIT Session 7). Holds only OUTSTANDING work — pending/running/failed; completed rows are deleted. Claimed via FOR UPDATE SKIP LOCKED (go/internal/work). input_version lets a changed input reopen completed/failed work instead of relying on elapsed time.';


--
-- Name: pipeline_work_status; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.pipeline_work_status AS
 SELECT stage,
    status,
    count(*) AS n,
    min(available_at) AS oldest_available_at,
    max(attempts) AS max_attempts
   FROM public.pipeline_work
  GROUP BY stage, status
  ORDER BY stage, status;


--
-- Name: player_current_identity_overrides_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.player_current_identity_overrides_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: player_current_identity_overrides_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.player_current_identity_overrides_id_seq OWNED BY public.player_current_identity_overrides.id;


--
-- Name: player_current_team_roster; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.player_current_team_roster AS
 SELECT sport,
    player_id,
    team_id,
    "position",
    position_group,
    jersey_number,
    source
   FROM public.player_current_identity;


--
-- Name: player_team_history; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.player_team_history (
    id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL,
    team_id integer NOT NULL,
    season integer,
    valid_from timestamp with time zone DEFAULT now(),
    valid_until timestamp with time zone,
    is_current boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT now()
);


--
-- Name: TABLE player_team_history; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.player_team_history IS 'Tracks player team movement over time. Used to detect transfers/trades.';


--
-- Name: player_team_history_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.player_team_history_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: player_team_history_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.player_team_history_id_seq OWNED BY public.player_team_history.id;


--
-- Name: provider_entity_map; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.provider_entity_map (
    provider text NOT NULL,
    sport text NOT NULL,
    entity_type text NOT NULL,
    provider_entity_id text NOT NULL,
    canonical_entity_id integer NOT NULL,
    meta jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT provider_entity_map_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: provider_fixture_map; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.provider_fixture_map (
    provider text NOT NULL,
    sport text NOT NULL,
    provider_fixture_id text NOT NULL,
    fixture_id integer NOT NULL,
    meta jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: provider_seasons; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.provider_seasons (
    id integer NOT NULL,
    league_id integer NOT NULL,
    season_year integer NOT NULL,
    provider text DEFAULT 'sportmonks'::text NOT NULL,
    provider_season_id integer NOT NULL
);


--
-- Name: provider_seasons_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.provider_seasons_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: provider_seasons_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.provider_seasons_id_seq OWNED BY public.provider_seasons.id;


--
-- Name: rate_modes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.rate_modes (
    sport text NOT NULL,
    mode text NOT NULL,
    suffix text NOT NULL,
    denom_key text NOT NULL,
    formula text NOT NULL,
    unit numeric,
    round_digits smallint NOT NULL,
    CONSTRAINT rate_modes_formula_check CHECK ((formula = ANY (ARRAY['div_mul'::text, 'mul_div'::text, 'div'::text, 'mul'::text])))
);


--
-- Name: COLUMN rate_modes.formula; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.rate_modes.formula IS 'Exact ROUND expression order (numeric division is inexact, so order matters at rounding ties): div_mul = ROUND(v / denom * unit, d) · mul_div = ROUND(v * unit / denom, d) · div = ROUND(v / denom, d) · mul = ROUND(v * denom, d)';


--
-- Name: rating_history; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.rating_history (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer NOT NULL,
    league_id integer DEFAULT 0 NOT NULL,
    rating_composite numeric,
    rating_composite_score numeric,
    rating_composite_rank numeric,
    rating_specialist numeric,
    rating_specialist_score numeric,
    rating_specialist_rank numeric,
    rating_specialty text,
    season_composite_rank_alltime numeric,
    rating_modes jsonb,
    trigger_type text DEFAULT 'recompute'::text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT rating_history_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT rating_history_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['seed'::text, 'in_season'::text, 'season_close'::text, 'recompute'::text, 'manual'::text])))
);


--
-- Name: TABLE rating_history; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.rating_history IS 'Append-only per-entity rating time-series. One frozen row per concluded season (trigger_type seed/season_close) plus in-season trajectory rows (daily / in_season). Written by snapshot_rating_history(), debounced insert-if-changed. The queryable rating-score series for future ML. season_composite_rank_alltime may be stale/NULL in seed-time rows until the post-backfill recalculate_alltime_ranks pass.';


--
-- Name: rating_history_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.rating_history_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: rating_history_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.rating_history_id_seq OWNED BY public.rating_history.id;


--
-- Name: rating_thresholds; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.rating_thresholds (
    sport text NOT NULL,
    stat_key text NOT NULL,
    min_value numeric NOT NULL
);


--
-- Name: TABLE rating_thresholds; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.rating_thresholds IS 'Rating eligibility gates: a player enters _compute_rating_bundle only when EVERY row for their sport passes (COALESCE(stats->>stat_key, 0) >= min_value). A sport with no rows rates nobody.';


--
-- Name: schema_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.schema_migrations (
    version text NOT NULL,
    applied_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE schema_migrations; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.schema_migrations IS 'Applied migration versions (file basename without .sql). Managed by sql/migrate.sh; backfilled for 001–051 in migration 051. Cloned to new envs by sql/build.sh.';


--
-- Name: season_recompute_needed; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.season_recompute_needed (
    sport text NOT NULL,
    season integer NOT NULL,
    requested_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text,
    attempts integer DEFAULT 0 NOT NULL
);


--
-- Name: TABLE season_recompute_needed; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.season_recompute_needed IS 'Durable dirty-season queue for deferred backfill (FIRST-GPT-AUDIT Session 6). A row means (sport, season) had >=1 fixture finalized with recompute=FALSE and still needs recompute_season + snapshot_rating_history. Upserted inside the finalize transaction; deleted only after a successful recompute + snapshot (end-of-run drain, `event recompute-drain`, or `event recompute --sport --season`).';


--
-- Name: sigil_synthesis; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sigil_synthesis (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    score smallint,
    previous_score smallint,
    blurb text,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    model_version text,
    prompt_version text,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    convergence smallint,
    disagreement text,
    why_now text,
    reading text,
    omen text,
    voiced_score smallint,
    voiced_at timestamp with time zone,
    voice_model_version text,
    voice_prompt_version text,
    CONSTRAINT sigil_synthesis_convergence_check CHECK (((convergence IS NULL) OR ((convergence >= 1) AND (convergence <= 100)))),
    CONSTRAINT sigil_synthesis_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT sigil_synthesis_omen_check CHECK (((omen IS NULL) OR (omen = ANY (ARRAY['ascendant'::text, 'steady'::text, 'waning'::text, 'crossroads'::text])))),
    CONSTRAINT sigil_synthesis_previous_score_check CHECK (((previous_score IS NULL) OR ((previous_score >= 1) AND (previous_score <= 100)))),
    CONSTRAINT sigil_synthesis_score_check CHECK (((score IS NULL) OR ((score >= 1) AND (score <= 100)))),
    CONSTRAINT sigil_synthesis_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['narrative_change'::text, 'sentiment_break'::text, 'composite_shift'::text, 'lazy_view'::text, 'periodic'::text, 'manual'::text]))),
    CONSTRAINT sigil_synthesis_voiced_score_check CHECK (((voiced_score IS NULL) OR ((voiced_score >= 1) AND (voiced_score <= 100))))
);


--
-- Name: TABLE sigil_synthesis; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.sigil_synthesis IS 'The Sigil: the crown synthesis (was vibe_synthesis). Fuses the three end products — Rating, Vibe, Momentum — into one holistic score + blurb.';


--
-- Name: COLUMN sigil_synthesis.convergence; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.convergence IS 'Phase 5.3 panel output: how strongly the lenses (PEAK/narrative/vibe/momentum/transfer) agree, 1-100. NULL = model did not emit it (pre-s11 row or omitted this run).';


--
-- Name: COLUMN sigil_synthesis.disagreement; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.disagreement IS 'Phase 5.3 panel output: short summary of where the rails diverge (e.g. strong stats vs negative narrative). NULL when the lenses agree or it was unstated.';


--
-- Name: COLUMN sigil_synthesis.why_now; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.why_now IS 'Phase 5.3 panel output: one-line breaking-news freshness note explaining why the read moved now. NULL when nothing is fresh or it was unstated.';


--
-- Name: COLUMN sigil_synthesis.reading; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.reading IS 'Oracle voice of this decided card (2-4 sentences). NULL = marker or pre-merge row. Carried forward verbatim from the prior row when the re-voice rule skips the model.';


--
-- Name: COLUMN sigil_synthesis.omen; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.omen IS 'Computed omen the current reading was drawn under (closed set; code-computed, never model output).';


--
-- Name: COLUMN sigil_synthesis.voiced_score; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.voiced_score IS 'Sigil score the current reading was drawn under — the re-voice hysteresis baseline (North Star #8).';


--
-- Name: COLUMN sigil_synthesis.voiced_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.sigil_synthesis.voiced_at IS 'When the current reading was actually drawn; older than generated_at on carried-forward rows.';


--
-- Name: sigil_synthesis_shadow; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sigil_synthesis_shadow (
    id bigint NOT NULL,
    source text DEFAULT 'rust'::text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    score smallint,
    blurb text,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    temperature real NOT NULL,
    built_prompt text,
    ollama_request jsonb,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT sigil_synthesis_shadow_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT sigil_synthesis_shadow_score_check CHECK (((score IS NULL) OR ((score >= 1) AND (score <= 100)))),
    CONSTRAINT sigil_synthesis_shadow_source_check CHECK ((source = ANY (ARRAY['rust'::text, 'go'::text])))
);


--
-- Name: TABLE sigil_synthesis_shadow; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.sigil_synthesis_shadow IS 'Rust Cognition Harness L3 sigil parity harness — offline shadow of sigil_synthesis; holds source=rust and source=go temp-0 rows for the parity diff. Drop after sigil cutover.';


--
-- Name: sigil_synthesis_shadow_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.sigil_synthesis_shadow_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sigil_synthesis_shadow_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.sigil_synthesis_shadow_id_seq OWNED BY public.sigil_synthesis_shadow.id;


--
-- Name: source_tiers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.source_tiers (
    source text NOT NULL,
    kind text NOT NULL,
    tier smallint NOT NULL,
    weight numeric(4,2) NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT source_tiers_kind_check CHECK ((kind = 'news'::text)),
    CONSTRAINT source_tiers_tier_check CHECK (((tier >= 1) AND (tier <= 3)))
);


--
-- Name: sport_autofill_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sport_autofill_versions (
    sport text NOT NULL,
    version bigint DEFAULT 1 NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    total_entities integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'ready'::text NOT NULL,
    reason text,
    CONSTRAINT sport_autofill_versions_status_check CHECK ((status = ANY (ARRAY['ready'::text, 'refreshing'::text, 'failed'::text])))
);


--
-- Name: stat_definitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.stat_definitions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: stat_definitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.stat_definitions_id_seq OWNED BY public.stat_definitions.id;


--
-- Name: stat_matchups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stat_matchups (
    sport text NOT NULL,
    scope text DEFAULT 'career'::text NOT NULL,
    stat_key text NOT NULL,
    subject_type text NOT NULL,
    subject_id integer NOT NULL,
    object_type text NOT NULL,
    object_id integer NOT NULL,
    n_games integer NOT NULL,
    baseline_avg numeric(8,3) NOT NULL,
    matchup_avg numeric(8,3) NOT NULL,
    expected_avg numeric(8,3) NOT NULL,
    raw_delta numeric(8,3) NOT NULL,
    adj_delta numeric(8,3) NOT NULL,
    shrunk_delta numeric(8,3) NOT NULL,
    reliability smallint NOT NULL,
    components jsonb DEFAULT '{}'::jsonb NOT NULL,
    first_season integer NOT NULL,
    last_season integer NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT stat_matchups_n_games_check CHECK ((n_games >= 1)),
    CONSTRAINT stat_matchups_no_self_loop_check CHECK ((NOT ((subject_type = object_type) AND (subject_id = object_id)))),
    CONSTRAINT stat_matchups_object_type_check CHECK ((object_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT stat_matchups_reliability_check CHECK (((reliability >= 0) AND (reliability <= 100))),
    CONSTRAINT stat_matchups_scope_check CHECK ((scope = ANY (ARRAY['career'::text, 'season'::text]))),
    CONSTRAINT stat_matchups_subject_type_check CHECK ((subject_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: TABLE stat_matchups; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.stat_matchups IS 'Pairwise performance edges (player-vs-team per stat), the stats rail''s computed counterpart to narrative_links. Presented, not gatekept: no minimum-sample row filter — reliability (shrinkage weight x recency x consistency, 0-100) travels with every row so the model frames weak samples ("only faced them once") instead of a threshold silently hiding them. Effect size (deltas) and trust (reliability) are orthogonal on purpose: huge-effect/low-reliability is the grain-of-salt case.';


--
-- Name: COLUMN stat_matchups.adj_delta; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_matchups.adj_delta IS 'matchup_avg minus (baseline_avg x opponent allow-ratio): the opponent-adjusted edge, so a stat the opponent concedes to everyone is not credited to the player.';


--
-- Name: COLUMN stat_matchups.reliability; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_matchups.reliability IS 'How much to believe adj_delta, 0-100: n/(n+k) shrinkage weight discounted by recency (meetings in the 2 newest seasons) and direction-consistency across meetings. Breakdown in components. Serve it WITH the fact; never filter by it.';


--
-- Name: stat_summaries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stat_summaries (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    body text,
    notability smallint,
    notability_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    model_version text,
    prompt_version text,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    season integer,
    input_hash text,
    divined_peak text,
    peak_trajectory text,
    peak_trajectory_label text,
    peak_trajectory_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT stat_summaries_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT stat_summaries_notability_check CHECK (((notability IS NULL) OR ((notability >= 0) AND (notability <= 100)))),
    CONSTRAINT stat_summaries_peak_trajectory_check CHECK ((peak_trajectory = ANY (ARRAY['rising'::text, 'falling'::text, 'steady'::text]))),
    CONSTRAINT stat_summaries_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['stat_change'::text, 'periodic'::text, 'manual'::text])))
);


--
-- Name: TABLE stat_summaries; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.stat_summaries IS 'Append, one row per generation per entity (latest-per-entity read). The STATS-rail narrative: the local model''s on-field IDENTITY analysis derived from the rating engine''s scrubbed datapoints (composite=how well, peak=how). notability (deterministic, distinctiveness) drives dynamic length + a board; NULL body = insufficient-stats marker. Twin of news_summaries.';


--
-- Name: COLUMN stat_summaries.season; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.season IS 'The season the commentary describes; reads are latest-per-(entity,sport,season). Previous seasons are immutable (backfilled once); the nightly cadence regenerates only the current season, and only when input_hash changes.';


--
-- Name: COLUMN stat_summaries.input_hash; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.input_hash IS 'Hash of input_components (the rating snapshot fed to the local model) — the nightly skip-unless-changed signal.';


--
-- Name: COLUMN stat_summaries.divined_peak; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.divined_peak IS 'Model-divined peak-strength label (e.g. "Rim Protection"), parsed from the rating prompt''s "PEAK: <label>" first line; the Rating card hero label. Was divined_sigil pre-convergence.';


--
-- Name: COLUMN stat_summaries.peak_trajectory; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.peak_trajectory IS 'Deterministic recent-form marker for the stats rail z-score values: rising, falling, or steady.';


--
-- Name: COLUMN stat_summaries.peak_trajectory_label; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.peak_trajectory_label IS 'Human label for the rating z-score trajectory, e.g. Composite and PEAK z-scores trending down over recent games.';


--
-- Name: COLUMN stat_summaries.peak_trajectory_components; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_summaries.peak_trajectory_components IS 'Transparent PEAK trajectory inputs: recent Composite/PEAK z-score samples, slopes, and sample sizes.';


--
-- Name: stat_summaries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.stat_summaries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: stat_summaries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.stat_summaries_id_seq OWNED BY public.stat_summaries.id;


--
-- Name: stat_summaries_shadow; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stat_summaries_shadow (
    id bigint NOT NULL,
    source text DEFAULT 'rust'::text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    season integer,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    body text,
    divined_peak text,
    notability smallint,
    notability_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    model_version text,
    prompt_version text NOT NULL,
    temperature real NOT NULL,
    built_prompt text,
    ollama_request jsonb,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT stat_summaries_shadow_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT stat_summaries_shadow_notability_check CHECK (((notability IS NULL) OR ((notability >= 0) AND (notability <= 100)))),
    CONSTRAINT stat_summaries_shadow_source_check CHECK ((source = ANY (ARRAY['rust'::text, 'go'::text])))
);


--
-- Name: TABLE stat_summaries_shadow; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.stat_summaries_shadow IS 'Rust Cognition Harness L12 rating parity harness — offline shadow of stat_summaries; holds source=rust and source=go rows. Diff built_prompt + ollama_request (whole jsonb) + model_version + prompt_version + input_hash. Drop after rating cutover.';


--
-- Name: stat_summaries_shadow_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.stat_summaries_shadow_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: stat_summaries_shadow_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.stat_summaries_shadow_id_seq OWNED BY public.stat_summaries_shadow.id;


--
-- Name: stat_templates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.stat_templates (
    sport text NOT NULL,
    position_group text NOT NULL,
    stat_key text NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    facet text
);


--
-- Name: TABLE stat_templates; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.stat_templates IS 'Per-(sport, position_group) counting-stat templates for the Composite pizza. Absent position_group => no template => frontend falls back to the z-score pizza.';


--
-- Name: COLUMN stat_templates.facet; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.stat_templates.facet IS 'Pizza grouping for the Composite card. NULL = single unfaceted pizza (the NFL/NBA fantasy templates); non-NULL = one pizza per facet, ordered by item sort_order (the football counting-stat pizzas).';


--
-- Name: topic_heat_embeddings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.topic_heat_embeddings (
    article_id bigint NOT NULL,
    fingerprint bigint NOT NULL,
    model text NOT NULL,
    embedding bytea NOT NULL,
    embedded_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE topic_heat_embeddings; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.topic_heat_embeddings IS 'Write-through cache of topic-heat article embeddings (mig 151). Reproducible from news_articles text; safe to truncate.';


--
-- Name: transfer_identity_applications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfer_identity_applications (
    id bigint NOT NULL,
    sport text NOT NULL,
    player_id integer NOT NULL,
    old_team_id integer,
    old_league_id integer,
    new_team_id integer NOT NULL,
    new_league_id integer,
    source_rumor_id bigint,
    source_synthesis_id bigint,
    deterministic_heat smallint NOT NULL,
    deterministic_confidence numeric(4,3) CONSTRAINT transfer_identity_application_deterministic_confidence_not_null NOT NULL,
    threshold_config jsonb DEFAULT '{}'::jsonb NOT NULL,
    adjudication jsonb DEFAULT '{}'::jsonb NOT NULL,
    adjudication_raw text,
    adjudication_model_version text,
    adjudication_prompt_version text,
    decision text NOT NULL,
    event_type text,
    adjudication_confidence numeric(4,3),
    status text NOT NULL,
    reason text,
    evidence jsonb DEFAULT '{}'::jsonb NOT NULL,
    override_id bigint,
    applied_at timestamp with time zone,
    reverted_at timestamp with time zone,
    reverted_by text,
    revert_reason text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT transfer_identity_applications_adjudication_confidence_check CHECK (((adjudication_confidence IS NULL) OR ((adjudication_confidence >= (0)::numeric) AND (adjudication_confidence <= (1)::numeric)))),
    CONSTRAINT transfer_identity_applications_decision_check CHECK ((decision = ANY (ARRAY['apply'::text, 'reject'::text, 'manual_review'::text, 'failed_closed'::text]))),
    CONSTRAINT transfer_identity_applications_deterministic_confidence_check CHECK (((deterministic_confidence >= (0)::numeric) AND (deterministic_confidence <= (1)::numeric))),
    CONSTRAINT transfer_identity_applications_deterministic_heat_check CHECK (((deterministic_heat >= 0) AND (deterministic_heat <= 100))),
    CONSTRAINT transfer_identity_applications_status_check CHECK ((status = ANY (ARRAY['applied'::text, 'rejected'::text, 'manual_review'::text, 'failed_closed'::text, 'reverted'::text])))
);


--
-- Name: transfer_identity_applications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.transfer_identity_applications_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: transfer_identity_applications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.transfer_identity_applications_id_seq OWNED BY public.transfer_identity_applications.id;


--
-- Name: transfer_identity_thresholds; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfer_identity_thresholds (
    sport text NOT NULL,
    min_heat smallint DEFAULT 80 NOT NULL,
    min_deterministic_confidence numeric(4,3) DEFAULT 0.800 CONSTRAINT transfer_identity_threshold_min_deterministic_confiden_not_null NOT NULL,
    min_adjudication_confidence numeric(4,3) DEFAULT 0.850 CONSTRAINT transfer_identity_threshold_min_adjudication_confidenc_not_null NOT NULL,
    allowed_event_types text[] DEFAULT ARRAY['transfer'::text, 'trade'::text, 'loan'::text, 'signing'::text] NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT transfer_identity_thresholds_min_adjudication_confidence_check CHECK (((min_adjudication_confidence >= (0)::numeric) AND (min_adjudication_confidence <= (1)::numeric))),
    CONSTRAINT transfer_identity_thresholds_min_deterministic_confidence_check CHECK (((min_deterministic_confidence >= (0)::numeric) AND (min_deterministic_confidence <= (1)::numeric))),
    CONSTRAINT transfer_identity_thresholds_min_heat_check CHECK (((min_heat >= 0) AND (min_heat <= 100)))
);


--
-- Name: transfer_rumors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfer_rumors (
    id bigint NOT NULL,
    team_id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    heat smallint,
    heat_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    is_rumor boolean,
    direction text,
    stage text,
    model_summary text,
    source_attribution text,
    confidence numeric(3,2),
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text,
    prompt_version text,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    rumor_updated_at timestamp with time zone,
    source_count integer DEFAULT 0 NOT NULL,
    source_names text[] DEFAULT '{}'::text[] NOT NULL,
    source_latest_at timestamp with time zone,
    source_oldest_at timestamp with time zone,
    trajectory text DEFAULT 'developing_story'::text NOT NULL,
    trajectory_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_hash text,
    CONSTRAINT transfer_rumors_direction_check CHECK (((direction IS NULL) OR (direction = ANY (ARRAY['incoming'::text, 'outgoing'::text, 'unclear'::text])))),
    CONSTRAINT transfer_rumors_heat_check CHECK (((heat IS NULL) OR ((heat >= 0) AND (heat <= 100)))),
    CONSTRAINT transfer_rumors_stage_check CHECK (((stage IS NULL) OR (stage = ANY (ARRAY['speculation'::text, 'concrete_interest'::text, 'advanced_talks'::text, 'here_we_go'::text])))),
    CONSTRAINT transfer_rumors_trajectory_check CHECK ((trajectory = ANY (ARRAY['developing_story'::text, 'heating_up'::text, 'cooling_off'::text]))),
    CONSTRAINT transfer_rumors_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['news_spike'::text, 'periodic'::text, 'manual'::text])))
);


--
-- Name: COLUMN transfer_rumors.rumor_updated_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.transfer_rumors.rumor_updated_at IS 'Transfer-rumor freshness timestamp for the News hub. Usually the newest cited source timestamp; falls back to generated_at.';


--
-- Name: COLUMN transfer_rumors.trajectory; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.transfer_rumors.trajectory IS 'Transfer-rumor trajectory marker shared with narratives: developing_story, heating_up, or cooling_off.';


--
-- Name: COLUMN transfer_rumors.input_hash; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.transfer_rumors.input_hash IS 'SHA-256 (128-bit hex prefix) of the pair''s material vetting inputs (sorted pair-corpus news ids + distinct_sources/tier_weight from the heat components + the deterministic team relationship); the per-pair transfer debounce key (F3, flow-friction plan 2026-07-12). Time-decay heat components are deliberately excluded. NULL on pre-F3 rows, which never match — each pair regenerates once post-deploy, then stamps.';


--
-- Name: transfer_rumors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.transfer_rumors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: transfer_rumors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.transfer_rumors_id_seq OWNED BY public.transfer_rumors.id;


--
-- Name: transfer_rumors_shadow; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfer_rumors_shadow (
    id bigint NOT NULL,
    source text DEFAULT 'rust'::text NOT NULL,
    team_id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    heat smallint,
    heat_components jsonb DEFAULT '{}'::jsonb NOT NULL,
    is_rumor boolean,
    direction text,
    stage text,
    model_summary text,
    source_attribution text,
    confidence real,
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text,
    prompt_version text NOT NULL,
    temperature real NOT NULL,
    built_prompt text,
    ollama_request jsonb,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT transfer_rumors_shadow_heat_check CHECK (((heat IS NULL) OR ((heat >= 0) AND (heat <= 100)))),
    CONSTRAINT transfer_rumors_shadow_source_check CHECK ((source = ANY (ARRAY['rust'::text, 'go'::text])))
);


--
-- Name: TABLE transfer_rumors_shadow; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.transfer_rumors_shadow IS 'Rust Cognition Harness L11 transfers parity harness — offline shadow of transfer_rumors; holds source=rust (t4) and source=go (t3) rows. Diff built_prompt + (ollama_request - system). Drop after transfers cutover.';


--
-- Name: transfer_rumors_shadow_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.transfer_rumors_shadow_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: transfer_rumors_shadow_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.transfer_rumors_shadow_id_seq OWNED BY public.transfer_rumors_shadow.id;


--
-- Name: user_devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_devices (
    id integer NOT NULL,
    user_id uuid NOT NULL,
    platform text NOT NULL,
    token text NOT NULL,
    is_active boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_devices_platform_check CHECK ((platform = ANY (ARRAY['ios'::text, 'android'::text, 'web'::text])))
);


--
-- Name: user_devices_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_devices_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_devices_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_devices_id_seq OWNED BY public.user_devices.id;


--
-- Name: user_follows; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_follows (
    id integer NOT NULL,
    user_id uuid NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_follows_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text])))
);


--
-- Name: user_follows_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_follows_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_follows_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_follows_id_seq OWNED BY public.user_follows.id;


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    timezone text DEFAULT 'UTC'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: vibe_scores; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.vibe_scores (
    id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    sentiment smallint,
    prompt text,
    input_hash text,
    CONSTRAINT vibe_scores_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT vibe_scores_sentiment_check CHECK (((sentiment IS NULL) OR ((sentiment >= 1) AND (sentiment <= 100)))),
    CONSTRAINT vibe_scores_trigger_type_check CHECK ((trigger_type = ANY (ARRAY['milestone'::text, 'manual'::text, 'periodic'::text, 'news_spike'::text])))
);


--
-- Name: TABLE vibe_scores; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.vibe_scores IS 'The Vibe: the emotional rail end product (was sentiment_scores). sentiment (1-100) + prompt; feeds the Sigil and the Momentum vibe-trajectory. Append-only.';


--
-- Name: COLUMN vibe_scores.input_hash; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.vibe_scores.input_hash IS 'SHA-256 (128-bit hex prefix) of the generation''s material inputs (latest narrative titles/impacts/trajectories + transfer-heat facts); the Rust harness debounce key. NULL on pre-147 rows.';


--
-- Name: vibe_scores_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.vibe_scores_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: vibe_scores_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.vibe_scores_id_seq OWNED BY public.vibe_scores.id;


--
-- Name: vibe_scores_shadow; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.vibe_scores_shadow (
    id bigint NOT NULL,
    source text DEFAULT 'rust'::text NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    trigger_type text NOT NULL,
    trigger_payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    sentiment smallint,
    prompt text,
    input_news_ids bigint[] DEFAULT '{}'::bigint[] NOT NULL,
    model_version text NOT NULL,
    prompt_version text NOT NULL,
    temperature real NOT NULL,
    built_prompt text,
    ollama_request jsonb,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    input_hash text,
    CONSTRAINT vibe_scores_shadow_entity_type_check CHECK ((entity_type = ANY (ARRAY['player'::text, 'team'::text]))),
    CONSTRAINT vibe_scores_shadow_sentiment_check CHECK (((sentiment IS NULL) OR ((sentiment >= 1) AND (sentiment <= 100)))),
    CONSTRAINT vibe_scores_shadow_source_check CHECK ((source = ANY (ARRAY['rust'::text, 'go'::text])))
);


--
-- Name: TABLE vibe_scores_shadow; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.vibe_scores_shadow IS 'Rust-scrubber Phase 1 vibe parity harness — offline shadow of vibe_scores; holds source=rust and source=go temp-0 rows for the parity diff. Drop after vibe cutover.';


--
-- Name: COLUMN vibe_scores_shadow.input_hash; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.vibe_scores_shadow.input_hash IS 'Parity capture of vibe_scores.input_hash (migration 147) — a deterministic diff axis.';


--
-- Name: vibe_scores_shadow_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.vibe_scores_shadow_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: vibe_scores_shadow_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.vibe_scores_shadow_id_seq OWNED BY public.vibe_scores_shadow.id;


--
-- Name: vibe_synthesis_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.vibe_synthesis_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: vibe_synthesis_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.vibe_synthesis_id_seq OWNED BY public.sigil_synthesis.id;


--
-- Name: cognition_ledger id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cognition_ledger ALTER COLUMN id SET DEFAULT nextval('public.cognition_ledger_id_seq'::regclass);


--
-- Name: event_box_scores id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_box_scores ALTER COLUMN id SET DEFAULT nextval('public.event_box_scores_id_seq'::regclass);


--
-- Name: event_team_stats id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_team_stats ALTER COLUMN id SET DEFAULT nextval('public.event_team_stats_id_seq'::regclass);


--
-- Name: fixtures id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fixtures ALTER COLUMN id SET DEFAULT nextval('public.fixtures_id_seq'::regclass);


--
-- Name: metadata_refresh_queue id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_refresh_queue ALTER COLUMN id SET DEFAULT nextval('public.metadata_refresh_queue_id_seq'::regclass);


--
-- Name: metadata_sync_log id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_sync_log ALTER COLUMN id SET DEFAULT nextval('public.metadata_sync_log_id_seq'::regclass);


--
-- Name: momentum_scores id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.momentum_scores ALTER COLUMN id SET DEFAULT nextval('public.momentum_scores_id_seq'::regclass);


--
-- Name: momentum_summaries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.momentum_summaries ALTER COLUMN id SET DEFAULT nextval('public.momentum_summaries_id_seq'::regclass);


--
-- Name: narrative_episodes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_episodes ALTER COLUMN id SET DEFAULT nextval('public.narrative_episodes_id_seq'::regclass);


--
-- Name: narrative_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_events ALTER COLUMN id SET DEFAULT nextval('public.narrative_events_id_seq'::regclass);


--
-- Name: narrative_person_affiliations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_affiliations ALTER COLUMN id SET DEFAULT nextval('public.narrative_person_affiliations_id_seq'::regclass);


--
-- Name: narrative_persons id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_persons ALTER COLUMN id SET DEFAULT nextval('public.narrative_persons_id_seq'::regclass);


--
-- Name: news_articles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_articles ALTER COLUMN id SET DEFAULT nextval('public.news_articles_id_seq'::regclass);


--
-- Name: news_summaries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_summaries ALTER COLUMN id SET DEFAULT nextval('public.news_summaries_id_seq'::regclass);


--
-- Name: news_summaries_shadow id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_summaries_shadow ALTER COLUMN id SET DEFAULT nextval('public.news_summaries_shadow_id_seq'::regclass);


--
-- Name: notifications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications ALTER COLUMN id SET DEFAULT nextval('public.notifications_id_seq'::regclass);


--
-- Name: oracle_readings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oracle_readings ALTER COLUMN id SET DEFAULT nextval('public.oracle_readings_id_seq'::regclass);


--
-- Name: pipeline_runs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_runs ALTER COLUMN id SET DEFAULT nextval('public.pipeline_runs_id_seq'::regclass);


--
-- Name: pipeline_stats id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_stats ALTER COLUMN id SET DEFAULT nextval('public.pipeline_stats_id_seq'::regclass);


--
-- Name: player_current_identity_overrides id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_current_identity_overrides ALTER COLUMN id SET DEFAULT nextval('public.player_current_identity_overrides_id_seq'::regclass);


--
-- Name: player_team_history id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_team_history ALTER COLUMN id SET DEFAULT nextval('public.player_team_history_id_seq'::regclass);


--
-- Name: provider_seasons id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_seasons ALTER COLUMN id SET DEFAULT nextval('public.provider_seasons_id_seq'::regclass);


--
-- Name: rating_history id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rating_history ALTER COLUMN id SET DEFAULT nextval('public.rating_history_id_seq'::regclass);


--
-- Name: sigil_synthesis id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sigil_synthesis ALTER COLUMN id SET DEFAULT nextval('public.vibe_synthesis_id_seq'::regclass);


--
-- Name: sigil_synthesis_shadow id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sigil_synthesis_shadow ALTER COLUMN id SET DEFAULT nextval('public.sigil_synthesis_shadow_id_seq'::regclass);


--
-- Name: stat_definitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_definitions ALTER COLUMN id SET DEFAULT nextval('public.stat_definitions_id_seq'::regclass);


--
-- Name: stat_summaries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_summaries ALTER COLUMN id SET DEFAULT nextval('public.stat_summaries_id_seq'::regclass);


--
-- Name: stat_summaries_shadow id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_summaries_shadow ALTER COLUMN id SET DEFAULT nextval('public.stat_summaries_shadow_id_seq'::regclass);


--
-- Name: transfer_identity_applications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications ALTER COLUMN id SET DEFAULT nextval('public.transfer_identity_applications_id_seq'::regclass);


--
-- Name: transfer_rumors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_rumors ALTER COLUMN id SET DEFAULT nextval('public.transfer_rumors_id_seq'::regclass);


--
-- Name: transfer_rumors_shadow id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_rumors_shadow ALTER COLUMN id SET DEFAULT nextval('public.transfer_rumors_shadow_id_seq'::regclass);


--
-- Name: user_devices id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_devices ALTER COLUMN id SET DEFAULT nextval('public.user_devices_id_seq'::regclass);


--
-- Name: user_follows id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows ALTER COLUMN id SET DEFAULT nextval('public.user_follows_id_seq'::regclass);


--
-- Name: vibe_scores id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vibe_scores ALTER COLUMN id SET DEFAULT nextval('public.vibe_scores_id_seq'::regclass);


--
-- Name: vibe_scores_shadow id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vibe_scores_shadow ALTER COLUMN id SET DEFAULT nextval('public.vibe_scores_shadow_id_seq'::regclass);


--
-- Name: auth_refresh_tokens auth_refresh_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_refresh_tokens
    ADD CONSTRAINT auth_refresh_tokens_pkey PRIMARY KEY (id);


--
-- Name: auth_refresh_tokens auth_refresh_tokens_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_refresh_tokens
    ADD CONSTRAINT auth_refresh_tokens_token_hash_key UNIQUE (token_hash);


--
-- Name: cognition_ledger cognition_ledger_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cognition_ledger
    ADD CONSTRAINT cognition_ledger_pkey PRIMARY KEY (id);


--
-- Name: event_box_scores event_box_scores_fixture_id_player_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_box_scores
    ADD CONSTRAINT event_box_scores_fixture_id_player_id_key UNIQUE (fixture_id, player_id);


--
-- Name: event_box_scores event_box_scores_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_box_scores
    ADD CONSTRAINT event_box_scores_pkey PRIMARY KEY (id);


--
-- Name: event_team_stats event_team_stats_fixture_id_team_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_team_stats
    ADD CONSTRAINT event_team_stats_fixture_id_team_id_key UNIQUE (fixture_id, team_id);


--
-- Name: event_team_stats event_team_stats_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_team_stats
    ADD CONSTRAINT event_team_stats_pkey PRIMARY KEY (id);


--
-- Name: fixtures fixtures_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fixtures
    ADD CONSTRAINT fixtures_pkey PRIMARY KEY (id);


--
-- Name: fixtures fixtures_sport_external_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fixtures
    ADD CONSTRAINT fixtures_sport_external_id_key UNIQUE (sport, external_id);


--
-- Name: leagues leagues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.leagues
    ADD CONSTRAINT leagues_pkey PRIMARY KEY (id);


--
-- Name: meta meta_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meta
    ADD CONSTRAINT meta_pkey PRIMARY KEY (key);


--
-- Name: metadata_refresh_queue metadata_refresh_queue_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_refresh_queue
    ADD CONSTRAINT metadata_refresh_queue_pkey PRIMARY KEY (id);


--
-- Name: metadata_sync_log metadata_sync_log_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_sync_log
    ADD CONSTRAINT metadata_sync_log_pkey PRIMARY KEY (id);


--
-- Name: momentum_refresh_needed momentum_refresh_needed_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.momentum_refresh_needed
    ADD CONSTRAINT momentum_refresh_needed_pkey PRIMARY KEY (sport);


--
-- Name: momentum_scores momentum_scores_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.momentum_scores
    ADD CONSTRAINT momentum_scores_pkey PRIMARY KEY (id);


--
-- Name: momentum_summaries momentum_summaries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.momentum_summaries
    ADD CONSTRAINT momentum_summaries_pkey PRIMARY KEY (id);


--
-- Name: narrative_episodes narrative_episodes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_episodes
    ADD CONSTRAINT narrative_episodes_pkey PRIMARY KEY (id);


--
-- Name: narrative_events narrative_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_events
    ADD CONSTRAINT narrative_events_pkey PRIMARY KEY (id);


--
-- Name: narrative_links narrative_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_links
    ADD CONSTRAINT narrative_links_pkey PRIMARY KEY (sport, link_type, subject_type, subject_id, object_type, object_id);


--
-- Name: narrative_person_affiliations narrative_person_affiliations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_affiliations
    ADD CONSTRAINT narrative_person_affiliations_pkey PRIMARY KEY (id);


--
-- Name: narrative_person_mentions narrative_person_mentions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_mentions
    ADD CONSTRAINT narrative_person_mentions_pkey PRIMARY KEY (article_id, person_id);


--
-- Name: narrative_persons narrative_persons_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_persons
    ADD CONSTRAINT narrative_persons_pkey PRIMARY KEY (id);


--
-- Name: news_article_entities news_article_entities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_article_entities
    ADD CONSTRAINT news_article_entities_pkey PRIMARY KEY (article_id, entity_type, entity_id, sport);


--
-- Name: news_articles news_articles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_articles
    ADD CONSTRAINT news_articles_pkey PRIMARY KEY (id);


--
-- Name: news_articles news_articles_url_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_articles
    ADD CONSTRAINT news_articles_url_hash_key UNIQUE (url_hash);


--
-- Name: news_summaries news_summaries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_summaries
    ADD CONSTRAINT news_summaries_pkey PRIMARY KEY (id);


--
-- Name: news_summaries_shadow news_summaries_shadow_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_summaries_shadow
    ADD CONSTRAINT news_summaries_shadow_pkey PRIMARY KEY (id);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: oracle_readings oracle_readings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oracle_readings
    ADD CONSTRAINT oracle_readings_pkey PRIMARY KEY (id);


--
-- Name: peer_cohort_aggregate peer_cohort_aggregate_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.peer_cohort_aggregate
    ADD CONSTRAINT peer_cohort_aggregate_pkey PRIMARY KEY (sport, season, league_id, entity_type, "position");


--
-- Name: pipeline_runs pipeline_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_runs
    ADD CONSTRAINT pipeline_runs_pkey PRIMARY KEY (id);


--
-- Name: pipeline_stats pipeline_stats_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_stats
    ADD CONSTRAINT pipeline_stats_pkey PRIMARY KEY (id);


--
-- Name: pipeline_stats pipeline_stats_sport_snapshot_date_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_stats
    ADD CONSTRAINT pipeline_stats_sport_snapshot_date_key UNIQUE (sport, snapshot_date);


--
-- Name: pipeline_work pipeline_work_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_work
    ADD CONSTRAINT pipeline_work_pkey PRIMARY KEY (stage, entity_type, entity_id, sport);


--
-- Name: player_current_identity_overrides player_current_identity_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_current_identity_overrides
    ADD CONSTRAINT player_current_identity_overrides_pkey PRIMARY KEY (id);


--
-- Name: player_stats player_stats_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_stats
    ADD CONSTRAINT player_stats_pkey PRIMARY KEY (player_id, sport, season, league_id);


--
-- Name: player_team_history player_team_history_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_team_history
    ADD CONSTRAINT player_team_history_pkey PRIMARY KEY (id);


--
-- Name: players players_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.players
    ADD CONSTRAINT players_pkey PRIMARY KEY (id, sport);


--
-- Name: provider_entity_map provider_entity_map_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_entity_map
    ADD CONSTRAINT provider_entity_map_pkey PRIMARY KEY (provider, sport, entity_type, provider_entity_id);


--
-- Name: provider_fixture_map provider_fixture_map_fixture_id_provider_sport_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_fixture_map
    ADD CONSTRAINT provider_fixture_map_fixture_id_provider_sport_key UNIQUE (fixture_id, provider, sport);


--
-- Name: provider_fixture_map provider_fixture_map_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_fixture_map
    ADD CONSTRAINT provider_fixture_map_pkey PRIMARY KEY (provider, sport, provider_fixture_id);


--
-- Name: provider_seasons provider_seasons_league_id_season_year_provider_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_seasons
    ADD CONSTRAINT provider_seasons_league_id_season_year_provider_key UNIQUE (league_id, season_year, provider);


--
-- Name: provider_seasons provider_seasons_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_seasons
    ADD CONSTRAINT provider_seasons_pkey PRIMARY KEY (id);


--
-- Name: rate_modes rate_modes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rate_modes
    ADD CONSTRAINT rate_modes_pkey PRIMARY KEY (sport, mode);


--
-- Name: rating_history rating_history_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rating_history
    ADD CONSTRAINT rating_history_pkey PRIMARY KEY (id);


--
-- Name: rating_thresholds rating_thresholds_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rating_thresholds
    ADD CONSTRAINT rating_thresholds_pkey PRIMARY KEY (sport, stat_key);


--
-- Name: schema_migrations schema_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.schema_migrations
    ADD CONSTRAINT schema_migrations_pkey PRIMARY KEY (version);


--
-- Name: season_recompute_needed season_recompute_needed_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_recompute_needed
    ADD CONSTRAINT season_recompute_needed_pkey PRIMARY KEY (sport, season);


--
-- Name: sigil_synthesis sigil_synthesis_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sigil_synthesis
    ADD CONSTRAINT sigil_synthesis_pkey PRIMARY KEY (id);


--
-- Name: sigil_synthesis_shadow sigil_synthesis_shadow_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sigil_synthesis_shadow
    ADD CONSTRAINT sigil_synthesis_shadow_pkey PRIMARY KEY (id);


--
-- Name: source_tiers source_tiers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.source_tiers
    ADD CONSTRAINT source_tiers_pkey PRIMARY KEY (kind, source);


--
-- Name: sport_autofill_versions sport_autofill_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sport_autofill_versions
    ADD CONSTRAINT sport_autofill_versions_pkey PRIMARY KEY (sport);


--
-- Name: sports sports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sports
    ADD CONSTRAINT sports_pkey PRIMARY KEY (id);


--
-- Name: stat_definitions stat_definitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_definitions
    ADD CONSTRAINT stat_definitions_pkey PRIMARY KEY (id);


--
-- Name: stat_definitions stat_definitions_sport_key_name_entity_type_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_definitions
    ADD CONSTRAINT stat_definitions_sport_key_name_entity_type_key UNIQUE (sport, key_name, entity_type);


--
-- Name: stat_matchups stat_matchups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_matchups
    ADD CONSTRAINT stat_matchups_pkey PRIMARY KEY (sport, scope, stat_key, subject_type, subject_id, object_type, object_id);


--
-- Name: stat_summaries stat_summaries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_summaries
    ADD CONSTRAINT stat_summaries_pkey PRIMARY KEY (id);


--
-- Name: stat_summaries_shadow stat_summaries_shadow_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_summaries_shadow
    ADD CONSTRAINT stat_summaries_shadow_pkey PRIMARY KEY (id);


--
-- Name: stat_templates stat_templates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_templates
    ADD CONSTRAINT stat_templates_pkey PRIMARY KEY (sport, position_group, stat_key);


--
-- Name: team_rosters team_rosters_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_rosters
    ADD CONSTRAINT team_rosters_pkey PRIMARY KEY (sport, season, team_id, player_id);


--
-- Name: team_stats team_stats_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_stats
    ADD CONSTRAINT team_stats_pkey PRIMARY KEY (team_id, sport, season, league_id);


--
-- Name: teams teams_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_pkey PRIMARY KEY (id, sport);


--
-- Name: topic_heat_embeddings topic_heat_embeddings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.topic_heat_embeddings
    ADD CONSTRAINT topic_heat_embeddings_pkey PRIMARY KEY (article_id);


--
-- Name: transfer_identity_applications transfer_identity_applications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_pkey PRIMARY KEY (id);


--
-- Name: transfer_identity_thresholds transfer_identity_thresholds_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_thresholds
    ADD CONSTRAINT transfer_identity_thresholds_pkey PRIMARY KEY (sport);


--
-- Name: transfer_rumors transfer_rumors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_rumors
    ADD CONSTRAINT transfer_rumors_pkey PRIMARY KEY (id);


--
-- Name: transfer_rumors_shadow transfer_rumors_shadow_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_rumors_shadow
    ADD CONSTRAINT transfer_rumors_shadow_pkey PRIMARY KEY (id);


--
-- Name: metadata_refresh_queue unique_pending_request; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_refresh_queue
    ADD CONSTRAINT unique_pending_request UNIQUE (player_id, sport, processed_at);


--
-- Name: metadata_sync_log unique_player_sync; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.metadata_sync_log
    ADD CONSTRAINT unique_player_sync UNIQUE (player_id, sport);


--
-- Name: user_devices user_devices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_devices
    ADD CONSTRAINT user_devices_pkey PRIMARY KEY (id);


--
-- Name: user_devices user_devices_user_id_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_devices
    ADD CONSTRAINT user_devices_user_id_token_key UNIQUE (user_id, token);


--
-- Name: user_follows user_follows_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_pkey PRIMARY KEY (id);


--
-- Name: user_follows user_follows_user_id_entity_type_entity_id_sport_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_user_id_entity_type_entity_id_sport_key UNIQUE (user_id, entity_type, entity_id, sport);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: vibe_scores vibe_scores_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vibe_scores
    ADD CONSTRAINT vibe_scores_pkey PRIMARY KEY (id);


--
-- Name: vibe_scores_shadow vibe_scores_shadow_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vibe_scores_shadow
    ADD CONSTRAINT vibe_scores_shadow_pkey PRIMARY KEY (id);


--
-- Name: idx_football_autofill_pk; Type: INDEX; Schema: football; Owner: -
--

CREATE UNIQUE INDEX idx_football_autofill_pk ON football.autofill_entities USING btree (id, type);


--
-- Name: idx_nba_autofill_pk; Type: INDEX; Schema: nba; Owner: -
--

CREATE UNIQUE INDEX idx_nba_autofill_pk ON nba.autofill_entities USING btree (id, type);


--
-- Name: idx_nfl_autofill_pk; Type: INDEX; Schema: nfl; Owner: -
--

CREATE UNIQUE INDEX idx_nfl_autofill_pk ON nfl.autofill_entities USING btree (id, type);


--
-- Name: idx_auth_refresh_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_auth_refresh_user ON public.auth_refresh_tokens USING btree (user_id);


--
-- Name: idx_cognition_ledger_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_cognition_ledger_entity ON public.cognition_ledger USING btree (stage, entity_type, entity_id, sport, generated_at DESC);


--
-- Name: idx_cognition_ledger_pair; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_cognition_ledger_pair ON public.cognition_ledger USING btree (stage, entity_type, entity_id, pair_entity_type, pair_entity_id, sport, generated_at DESC) WHERE (pair_entity_id IS NOT NULL);


--
-- Name: idx_cognition_ledger_product_rows; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_cognition_ledger_product_rows ON public.cognition_ledger USING gin (product_row_ids);


--
-- Name: idx_event_box_scores_fixture; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_box_scores_fixture ON public.event_box_scores USING btree (fixture_id);


--
-- Name: idx_event_box_scores_player_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_box_scores_player_composite ON public.event_box_scores USING btree (player_id, sport, season) INCLUDE (composite_score, minutes_played);


--
-- Name: idx_event_box_scores_player_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_box_scores_player_season ON public.event_box_scores USING btree (player_id, sport, season, league_id);


--
-- Name: idx_event_box_scores_team_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_box_scores_team_season ON public.event_box_scores USING btree (team_id, sport, season, league_id);


--
-- Name: idx_event_team_stats_fixture; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_team_stats_fixture ON public.event_team_stats USING btree (fixture_id);


--
-- Name: idx_event_team_stats_team_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_team_stats_team_composite ON public.event_team_stats USING btree (team_id, sport, season) INCLUDE (composite_score);


--
-- Name: idx_event_team_stats_team_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_event_team_stats_team_season ON public.event_team_stats USING btree (team_id, sport, season, league_id);


--
-- Name: idx_fixtures_away_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_away_team ON public.fixtures USING btree (away_team_id);


--
-- Name: idx_fixtures_home_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_home_team ON public.fixtures USING btree (home_team_id);


--
-- Name: idx_fixtures_league_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_league_date ON public.fixtures USING btree (league_id, start_time) WHERE (league_id IS NOT NULL);


--
-- Name: idx_fixtures_pending_seed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_pending_seed ON public.fixtures USING btree (sport, status, start_time) WHERE ((status = 'scheduled'::text) OR (status = 'completed'::text));


--
-- Name: idx_fixtures_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_season ON public.fixtures USING btree (season);


--
-- Name: idx_fixtures_sport_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_fixtures_sport_date ON public.fixtures USING btree (sport, start_time);


--
-- Name: idx_latest_momentum_scores_per_entity_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_latest_momentum_scores_per_entity_key ON public.latest_momentum_scores_per_entity USING btree (sport, entity_type, entity_id);


--
-- Name: idx_latest_momentum_scores_per_entity_rating; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_latest_momentum_scores_per_entity_rating ON public.latest_momentum_scores_per_entity USING btree (sport, rating_slope DESC, generated_at DESC) WHERE (rating_slope IS NOT NULL);


--
-- Name: idx_latest_momentum_scores_per_entity_vibe; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_latest_momentum_scores_per_entity_vibe ON public.latest_momentum_scores_per_entity USING btree (sport, vibe_slope DESC, generated_at DESC) WHERE (vibe_slope IS NOT NULL);


--
-- Name: idx_leagues_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_leagues_sport ON public.leagues USING btree (sport);


--
-- Name: idx_metadata_queue_pending; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_metadata_queue_pending ON public.metadata_refresh_queue USING btree (processed_at, priority, requested_at) WHERE (processed_at IS NULL);


--
-- Name: idx_metadata_queue_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_metadata_queue_player ON public.metadata_refresh_queue USING btree (player_id, sport);


--
-- Name: idx_momentum_scores_read; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_momentum_scores_read ON public.momentum_scores USING btree (sport, entity_type, entity_id, generated_at DESC);


--
-- Name: idx_momentum_summaries_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_momentum_summaries_entity_recent ON public.momentum_summaries USING btree (entity_type, entity_id, sport, season, generated_at DESC);


--
-- Name: idx_momentum_summaries_input_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_momentum_summaries_input_hash ON public.momentum_summaries USING btree (entity_type, entity_id, sport, season, input_hash) WHERE (input_hash IS NOT NULL);


--
-- Name: idx_nae_vetted_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_nae_vetted_lookup ON public.news_article_entities USING btree (entity_type, entity_id, sport) WHERE (vetted IS TRUE);


--
-- Name: idx_narrative_episodes_object; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_episodes_object ON public.narrative_episodes USING btree (sport, object_type, object_id, started_at DESC);


--
-- Name: idx_narrative_episodes_one_open; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_narrative_episodes_one_open ON public.narrative_episodes USING btree (sport, link_type, subject_type, subject_id, object_type, object_id) WHERE (status = 'open'::text);


--
-- Name: idx_narrative_episodes_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_episodes_season ON public.narrative_episodes USING btree (sport, season);


--
-- Name: idx_narrative_episodes_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_episodes_subject ON public.narrative_episodes USING btree (sport, subject_type, subject_id, started_at DESC);


--
-- Name: idx_narrative_episodes_type_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_episodes_type_status ON public.narrative_episodes USING btree (sport, link_type, status);


--
-- Name: idx_narrative_events_article; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_events_article ON public.narrative_events USING btree (article_id);


--
-- Name: idx_narrative_events_dedupe; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_narrative_events_dedupe ON public.narrative_events USING btree (article_id, sport, subject_type, subject_id, predicate, COALESCE(object_type, ''::text), COALESCE(object_id, 0));


--
-- Name: idx_narrative_events_object_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_events_object_date ON public.narrative_events USING btree (sport, object_type, object_id, event_date DESC) WHERE (object_id IS NOT NULL);


--
-- Name: idx_narrative_events_subject_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_events_subject_date ON public.narrative_events USING btree (sport, subject_type, subject_id, event_date DESC);


--
-- Name: idx_narrative_links_last_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_links_last_event ON public.narrative_links USING btree (sport, last_event_at DESC) WHERE (last_event_at IS NOT NULL);


--
-- Name: idx_narrative_links_object; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_links_object ON public.narrative_links USING btree (sport, object_type, object_id);


--
-- Name: idx_narrative_links_strength; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_links_strength ON public.narrative_links USING btree (sport, strength DESC) WHERE ((strength IS NOT NULL) AND (strength > 0));


--
-- Name: idx_narrative_person_affiliations_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_person_affiliations_entity ON public.narrative_person_affiliations USING btree (sport, entity_type, entity_id);


--
-- Name: idx_narrative_person_affiliations_one_current; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_narrative_person_affiliations_one_current ON public.narrative_person_affiliations USING btree (person_id, entity_type, entity_id, role) WHERE is_current;


--
-- Name: idx_narrative_person_affiliations_person; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_person_affiliations_person ON public.narrative_person_affiliations USING btree (person_id, valid_from DESC);


--
-- Name: idx_narrative_person_mentions_person; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_person_mentions_person ON public.narrative_person_mentions USING btree (person_id, created_at DESC);


--
-- Name: idx_narrative_persons_aliases; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_persons_aliases ON public.narrative_persons USING gin (aliases);


--
-- Name: idx_narrative_persons_lower_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_persons_lower_name ON public.narrative_persons USING btree (sport, lower(name));


--
-- Name: idx_narrative_persons_sport_kind_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_persons_sport_kind_status ON public.narrative_persons USING btree (sport, kind, status);


--
-- Name: idx_narrative_persons_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_narrative_persons_team ON public.narrative_persons USING btree (team_id, sport) WHERE (team_id IS NOT NULL);


--
-- Name: idx_news_articles_bucket; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_articles_bucket ON public.news_articles USING btree (bucket) WHERE (bucket IS NOT NULL);


--
-- Name: idx_news_articles_fetched_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_articles_fetched_at ON public.news_articles USING btree (fetched_at DESC);


--
-- Name: idx_news_articles_published_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_articles_published_at ON public.news_articles USING btree (published_at DESC);


--
-- Name: idx_news_articles_topic_heat; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_articles_topic_heat ON public.news_articles USING btree (topic_heat DESC, published_at DESC) WHERE (topic_heat IS NOT NULL);


--
-- Name: idx_news_entities_by_article; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_entities_by_article ON public.news_article_entities USING btree (article_id);


--
-- Name: idx_news_entities_lookup_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_entities_lookup_created ON public.news_article_entities USING btree (entity_type, entity_id, sport, created_at);


--
-- Name: idx_news_entities_unscrubbed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_entities_unscrubbed ON public.news_article_entities USING btree (article_id) WHERE (scrubbed_at IS NULL);


--
-- Name: idx_news_summaries_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_summaries_entity_recent ON public.news_summaries USING btree (entity_type, entity_id, sport, generated_at DESC);


--
-- Name: idx_news_summaries_scope; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_summaries_scope ON public.news_summaries USING btree (sport, entity_type, entity_id, generated_at DESC);


--
-- Name: idx_news_summaries_shadow_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_summaries_shadow_entity ON public.news_summaries_shadow USING btree (entity_type, entity_id, sport, source, generated_at DESC);


--
-- Name: idx_news_summaries_sport_impact; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_summaries_sport_impact ON public.news_summaries USING btree (sport, impact DESC, generated_at DESC) WHERE ((body IS NOT NULL) AND (impact IS NOT NULL));


--
-- Name: idx_news_summaries_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_news_summaries_updated ON public.news_summaries USING btree (sport, narrative_updated_at DESC) WHERE (body IS NOT NULL);


--
-- Name: idx_notifications_dispatch; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_notifications_dispatch ON public.notifications USING btree (status, scheduled_for) WHERE (status = 'scheduled'::text);


--
-- Name: idx_notifications_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_notifications_user ON public.notifications USING btree (user_id, created_at DESC);


--
-- Name: idx_oracle_readings_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_oracle_readings_entity_recent ON public.oracle_readings USING btree (entity_type, entity_id, sport, season, generated_at DESC);


--
-- Name: idx_oracle_readings_input_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_oracle_readings_input_hash ON public.oracle_readings USING btree (entity_type, entity_id, sport, season, input_hash) WHERE (input_hash IS NOT NULL);


--
-- Name: idx_pipeline_runs_job_started; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pipeline_runs_job_started ON public.pipeline_runs USING btree (job, started_at DESC);


--
-- Name: idx_pipeline_stats_sport_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pipeline_stats_sport_date ON public.pipeline_stats USING btree (sport, snapshot_date DESC);


--
-- Name: idx_pipeline_work_claim; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pipeline_work_claim ON public.pipeline_work USING btree (stage, available_at) WHERE (status = ANY (ARRAY['pending'::text, 'failed'::text]));


--
-- Name: idx_pipeline_work_running; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pipeline_work_running ON public.pipeline_work USING btree (updated_at) WHERE (status = 'running'::text);


--
-- Name: idx_player_current_identity_overrides_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_current_identity_overrides_active ON public.player_current_identity_overrides USING btree (sport, player_id, applied_at DESC, id DESC) WHERE (reverted_at IS NULL);


--
-- Name: idx_player_current_identity_overrides_source_rumor; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_current_identity_overrides_source_rumor ON public.player_current_identity_overrides USING btree (source_rumor_id) WHERE (source_rumor_id IS NOT NULL);


--
-- Name: idx_player_current_identity_overrides_transfer_idempotent; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_player_current_identity_overrides_transfer_idempotent ON public.player_current_identity_overrides USING btree (sport, player_id, COALESCE(team_id, 0), COALESCE(source_rumor_id, (0)::bigint), COALESCE(source_synthesis_id, (0)::bigint)) WHERE (source = 'applied_transfer'::text);


--
-- Name: idx_player_stats_fantasy_points; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_fantasy_points ON public.player_stats USING btree (sport, season, (((stats ->> 'fantasy_points'::text))::numeric) DESC) WHERE ((stats ->> 'fantasy_points'::text) IS NOT NULL);


--
-- Name: idx_player_stats_league; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_league ON public.player_stats USING btree (league_id) WHERE (league_id > 0);


--
-- Name: idx_player_stats_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_position ON public.player_stats USING btree (sport, "position");


--
-- Name: idx_player_stats_rated_enum; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_rated_enum ON public.player_stats USING btree (sport, season, player_id) WHERE (rating_composite_score IS NOT NULL);


--
-- Name: idx_player_stats_rating_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_rating_composite ON public.player_stats USING btree (sport, season, rating_composite DESC) WHERE (rating_composite IS NOT NULL);


--
-- Name: idx_player_stats_rating_specialist; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_rating_specialist ON public.player_stats USING btree (sport, season, rating_specialist DESC) WHERE (rating_specialist IS NOT NULL);


--
-- Name: idx_player_stats_rating_specialty; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_rating_specialty ON public.player_stats USING btree (sport, season, rating_specialty) WHERE (rating_specialty IS NOT NULL);


--
-- Name: idx_player_stats_sport_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_sport_season ON public.player_stats USING btree (sport, season);


--
-- Name: idx_player_stats_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_player_stats_team ON public.player_stats USING btree (team_id);


--
-- Name: idx_players_league; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_players_league ON public.players USING btree (league_id) WHERE (league_id IS NOT NULL);


--
-- Name: idx_players_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_players_name ON public.players USING btree (name);


--
-- Name: idx_players_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_players_sport ON public.players USING btree (sport);


--
-- Name: idx_players_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_players_team ON public.players USING btree (team_id);


--
-- Name: idx_players_tier_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_players_tier_sport ON public.players USING btree (tier, sport);


--
-- Name: idx_provider_entity_map_canonical; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_provider_entity_map_canonical ON public.provider_entity_map USING btree (sport, entity_type, canonical_entity_id);


--
-- Name: idx_provider_fixture_map_fixture; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_provider_fixture_map_fixture ON public.provider_fixture_map USING btree (fixture_id);


--
-- Name: idx_provider_seasons_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_provider_seasons_lookup ON public.provider_seasons USING btree (league_id, season_year);


--
-- Name: idx_rating_history_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_rating_history_entity_recent ON public.rating_history USING btree (entity_type, entity_id, sport, season, generated_at DESC);


--
-- Name: idx_sigil_synthesis_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_sigil_synthesis_entity_recent ON public.sigil_synthesis USING btree (entity_type, entity_id, sport, generated_at DESC);


--
-- Name: idx_sigil_synthesis_shadow_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_sigil_synthesis_shadow_entity ON public.sigil_synthesis_shadow USING btree (entity_type, entity_id, sport, season, source, generated_at DESC);


--
-- Name: idx_sigil_synthesis_sport_score; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_sigil_synthesis_sport_score ON public.sigil_synthesis USING btree (sport, score DESC, generated_at DESC) WHERE ((score IS NOT NULL) AND (blurb IS NOT NULL));


--
-- Name: idx_stat_definitions_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_definitions_sport ON public.stat_definitions USING btree (sport, entity_type);


--
-- Name: idx_stat_matchups_effect; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_matchups_effect ON public.stat_matchups USING btree (sport, stat_key, abs(shrunk_delta) DESC);


--
-- Name: idx_stat_matchups_object; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_matchups_object ON public.stat_matchups USING btree (sport, object_type, object_id, stat_key);


--
-- Name: idx_stat_summaries_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_summaries_entity_recent ON public.stat_summaries USING btree (entity_type, entity_id, sport, season, generated_at DESC);


--
-- Name: idx_stat_summaries_peak_trajectory; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_summaries_peak_trajectory ON public.stat_summaries USING btree (sport, peak_trajectory, generated_at DESC) WHERE ((body IS NOT NULL) AND (peak_trajectory IS NOT NULL));


--
-- Name: idx_stat_summaries_shadow_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_summaries_shadow_entity ON public.stat_summaries_shadow USING btree (entity_type, entity_id, sport, season, source, generated_at DESC);


--
-- Name: idx_stat_summaries_sport_notability; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_stat_summaries_sport_notability ON public.stat_summaries USING btree (sport, notability DESC, generated_at DESC) WHERE ((body IS NOT NULL) AND (notability IS NOT NULL));


--
-- Name: idx_sync_log_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_sync_log_player ON public.metadata_sync_log USING btree (player_id, sport);


--
-- Name: idx_team_history_current; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_history_current ON public.player_team_history USING btree (sport, is_current, team_id);


--
-- Name: idx_team_history_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_history_player ON public.player_team_history USING btree (player_id, sport, is_current);


--
-- Name: idx_team_rosters_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_rosters_active ON public.team_rosters USING btree (sport, season, is_active) WHERE is_active;


--
-- Name: idx_team_rosters_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_rosters_player ON public.team_rosters USING btree (sport, player_id);


--
-- Name: idx_team_stats_league; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_league ON public.team_stats USING btree (league_id) WHERE (league_id > 0);


--
-- Name: idx_team_stats_points; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_points ON public.team_stats USING btree ((((stats ->> 'points'::text))::integer)) WHERE ((stats ->> 'points'::text) IS NOT NULL);


--
-- Name: idx_team_stats_rated_enum; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_rated_enum ON public.team_stats USING btree (sport, season, team_id) WHERE (rating_composite_score IS NOT NULL);


--
-- Name: idx_team_stats_rating_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_rating_composite ON public.team_stats USING btree (sport, season, rating_composite DESC) WHERE (rating_composite IS NOT NULL);


--
-- Name: idx_team_stats_rating_specialist; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_rating_specialist ON public.team_stats USING btree (sport, season, rating_specialist DESC) WHERE (rating_specialist IS NOT NULL);


--
-- Name: idx_team_stats_sport_season; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_sport_season ON public.team_stats USING btree (sport, season);


--
-- Name: idx_team_stats_wins; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_team_stats_wins ON public.team_stats USING btree ((((stats ->> 'wins'::text))::integer)) WHERE ((stats ->> 'wins'::text) IS NOT NULL);


--
-- Name: idx_teams_conference; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_conference ON public.teams USING btree (sport, conference) WHERE (conference IS NOT NULL);


--
-- Name: idx_teams_league; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_league ON public.teams USING btree (league_id) WHERE (league_id IS NOT NULL);


--
-- Name: idx_teams_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_name ON public.teams USING btree (name);


--
-- Name: idx_teams_search_aliases; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_search_aliases ON public.teams USING gin (search_aliases);


--
-- Name: idx_teams_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_sport ON public.teams USING btree (sport);


--
-- Name: idx_teams_tier_sport; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_tier_sport ON public.teams USING btree (tier, sport);


--
-- Name: idx_transfer_identity_applications_idempotent_applied; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_transfer_identity_applications_idempotent_applied ON public.transfer_identity_applications USING btree (sport, player_id, COALESCE(old_team_id, 0), new_team_id, COALESCE(source_rumor_id, (0)::bigint), COALESCE(source_synthesis_id, (0)::bigint)) WHERE (status = ANY (ARRAY['applied'::text, 'reverted'::text]));


--
-- Name: idx_transfer_identity_applications_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_identity_applications_player ON public.transfer_identity_applications USING btree (sport, player_id, created_at DESC);


--
-- Name: idx_transfer_identity_applications_source_rumor; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_identity_applications_source_rumor ON public.transfer_identity_applications USING btree (source_rumor_id) WHERE (source_rumor_id IS NOT NULL);


--
-- Name: idx_transfer_rumors_pair_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_pair_recent ON public.transfer_rumors USING btree (team_id, player_id, sport, generated_at DESC);


--
-- Name: idx_transfer_rumors_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_player ON public.transfer_rumors USING btree (player_id, sport, generated_at DESC);


--
-- Name: idx_transfer_rumors_scope; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_scope ON public.transfer_rumors USING btree (sport, team_id, player_id, generated_at DESC);


--
-- Name: idx_transfer_rumors_shadow_pair; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_shadow_pair ON public.transfer_rumors_shadow USING btree (team_id, player_id, sport, source, generated_at DESC);


--
-- Name: idx_transfer_rumors_team_heat; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_team_heat ON public.transfer_rumors USING btree (team_id, sport, heat DESC, generated_at DESC) WHERE (is_rumor IS TRUE);


--
-- Name: idx_transfer_rumors_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfer_rumors_updated ON public.transfer_rumors USING btree (sport, rumor_updated_at DESC) WHERE (is_rumor IS TRUE);


--
-- Name: idx_user_follows_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_follows_entity ON public.user_follows USING btree (entity_type, entity_id, sport);


--
-- Name: idx_user_follows_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_follows_user ON public.user_follows USING btree (user_id);


--
-- Name: idx_vibe_scores_entity_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vibe_scores_entity_recent ON public.vibe_scores USING btree (entity_type, entity_id, sport, generated_at DESC);


--
-- Name: idx_vibe_scores_input_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vibe_scores_input_hash ON public.vibe_scores USING btree (entity_type, entity_id, sport, input_hash) WHERE (input_hash IS NOT NULL);


--
-- Name: idx_vibe_scores_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vibe_scores_recent ON public.vibe_scores USING btree (generated_at DESC);


--
-- Name: idx_vibe_scores_shadow_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vibe_scores_shadow_entity ON public.vibe_scores_shadow USING btree (entity_type, entity_id, sport, source, generated_at DESC);


--
-- Name: idx_vibe_scores_sport_sentiment; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vibe_scores_sport_sentiment ON public.vibe_scores USING btree (sport, sentiment DESC, generated_at DESC) WHERE (sentiment IS NOT NULL);


--
-- Name: news_article_entities enqueue_derive_on_vetted; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER enqueue_derive_on_vetted AFTER UPDATE OF vetted ON public.news_article_entities FOR EACH ROW WHEN (((new.vetted IS TRUE) AND (old.vetted IS DISTINCT FROM new.vetted))) EXECUTE FUNCTION public.enqueue_derive_on_vetted();


--
-- Name: event_box_scores mark_momentum_refresh_event_box_scores; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mark_momentum_refresh_event_box_scores AFTER INSERT OR UPDATE OF rating_composite_pct ON public.event_box_scores FOR EACH ROW EXECUTE FUNCTION public.mark_momentum_refresh_from_event_rating();


--
-- Name: event_team_stats mark_momentum_refresh_event_team_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mark_momentum_refresh_event_team_stats AFTER INSERT OR UPDATE OF rating_composite_pct ON public.event_team_stats FOR EACH ROW EXECUTE FUNCTION public.mark_momentum_refresh_from_event_rating();


--
-- Name: vibe_scores mark_momentum_refresh_vibe_scores; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mark_momentum_refresh_vibe_scores AFTER INSERT OR UPDATE OF sentiment ON public.vibe_scores FOR EACH ROW EXECUTE FUNCTION public.mark_momentum_refresh_from_vibe();


--
-- Name: pipeline_work pipeline_work_notify_insert; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER pipeline_work_notify_insert AFTER INSERT ON public.pipeline_work REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.notify_pipeline_work_ready();


--
-- Name: pipeline_work pipeline_work_notify_update; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER pipeline_work_notify_update AFTER UPDATE ON public.pipeline_work REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.notify_pipeline_work_ready();


--
-- Name: momentum_scores refresh_latest_momentum_scores_per_entity; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER refresh_latest_momentum_scores_per_entity AFTER INSERT OR DELETE OR UPDATE OR TRUNCATE ON public.momentum_scores FOR EACH STATEMENT EXECUTE FUNCTION public.refresh_latest_momentum_scores_per_entity();


--
-- Name: event_box_scores trg_detect_team_change; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_detect_team_change AFTER INSERT ON public.event_box_scores FOR EACH ROW EXECUTE FUNCTION public.detect_team_change();


--
-- Name: player_stats trg_football_derived_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_football_derived_stats BEFORE INSERT OR UPDATE ON public.player_stats FOR EACH ROW WHEN ((new.sport = 'FOOTBALL'::text)) EXECUTE FUNCTION football.compute_derived_player_stats();


--
-- Name: narrative_persons trg_narrative_persons_affiliation; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_narrative_persons_affiliation AFTER INSERT OR UPDATE OF team_id ON public.narrative_persons FOR EACH ROW EXECUTE FUNCTION public.narrative_persons_track_affiliation();


--
-- Name: player_stats trg_nba_player_derived_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_nba_player_derived_stats BEFORE INSERT OR UPDATE ON public.player_stats FOR EACH ROW WHEN ((new.sport = 'NBA'::text)) EXECUTE FUNCTION nba.compute_derived_player_stats();


--
-- Name: team_stats trg_nba_team_derived_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_nba_team_derived_stats BEFORE INSERT OR UPDATE ON public.team_stats FOR EACH ROW WHEN ((new.sport = 'NBA'::text)) EXECUTE FUNCTION nba.compute_derived_team_stats();


--
-- Name: player_stats trg_nfl_player_derived_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_nfl_player_derived_stats BEFORE INSERT OR UPDATE ON public.player_stats FOR EACH ROW WHEN ((new.sport = 'NFL'::text)) EXECUTE FUNCTION nfl.compute_derived_player_stats();


--
-- Name: team_stats trg_nfl_team_derived_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_nfl_team_derived_stats BEFORE INSERT OR UPDATE ON public.team_stats FOR EACH ROW WHEN ((new.sport = 'NFL'::text)) EXECUTE FUNCTION nfl.compute_derived_team_stats();


--
-- Name: player_stats trg_percentile_changed_player_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_percentile_changed_player_stats AFTER UPDATE OF percentiles ON public.player_stats FOR EACH ROW WHEN (((new.percentiles IS NOT NULL) AND (new.percentiles <> '{}'::jsonb))) EXECUTE FUNCTION public.notify_percentile_changed();


--
-- Name: team_stats trg_percentile_changed_team_stats; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_percentile_changed_team_stats AFTER UPDATE OF percentiles ON public.team_stats FOR EACH ROW WHEN (((new.percentiles IS NOT NULL) AND (new.percentiles <> '{}'::jsonb))) EXECUTE FUNCTION public.notify_percentile_changed();


--
-- Name: metadata_sync_log trg_sync_log_timestamp; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_sync_log_timestamp BEFORE UPDATE ON public.metadata_sync_log FOR EACH ROW EXECUTE FUNCTION public.update_sync_log_timestamp();


--
-- Name: auth_refresh_tokens auth_refresh_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_refresh_tokens
    ADD CONSTRAINT auth_refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: event_box_scores event_box_scores_fixture_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_box_scores
    ADD CONSTRAINT event_box_scores_fixture_id_fkey FOREIGN KEY (fixture_id) REFERENCES public.fixtures(id);


--
-- Name: event_box_scores event_box_scores_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_box_scores
    ADD CONSTRAINT event_box_scores_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: event_team_stats event_team_stats_fixture_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_team_stats
    ADD CONSTRAINT event_team_stats_fixture_id_fkey FOREIGN KEY (fixture_id) REFERENCES public.fixtures(id);


--
-- Name: event_team_stats event_team_stats_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_team_stats
    ADD CONSTRAINT event_team_stats_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: fixtures fixtures_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fixtures
    ADD CONSTRAINT fixtures_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: leagues leagues_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.leagues
    ADD CONSTRAINT leagues_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_episodes narrative_episodes_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_episodes
    ADD CONSTRAINT narrative_episodes_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_events narrative_events_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_events
    ADD CONSTRAINT narrative_events_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_links narrative_links_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_links
    ADD CONSTRAINT narrative_links_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_person_affiliations narrative_person_affiliations_person_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_affiliations
    ADD CONSTRAINT narrative_person_affiliations_person_id_fkey FOREIGN KEY (person_id) REFERENCES public.narrative_persons(id) ON DELETE CASCADE;


--
-- Name: narrative_person_affiliations narrative_person_affiliations_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_affiliations
    ADD CONSTRAINT narrative_person_affiliations_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_person_mentions narrative_person_mentions_article_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_mentions
    ADD CONSTRAINT narrative_person_mentions_article_id_fkey FOREIGN KEY (article_id) REFERENCES public.news_articles(id) ON DELETE CASCADE;


--
-- Name: narrative_person_mentions narrative_person_mentions_person_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_mentions
    ADD CONSTRAINT narrative_person_mentions_person_id_fkey FOREIGN KEY (person_id) REFERENCES public.narrative_persons(id) ON DELETE CASCADE;


--
-- Name: narrative_person_mentions narrative_person_mentions_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_person_mentions
    ADD CONSTRAINT narrative_person_mentions_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_persons narrative_persons_merged_into_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_persons
    ADD CONSTRAINT narrative_persons_merged_into_fkey FOREIGN KEY (merged_into) REFERENCES public.narrative_persons(id);


--
-- Name: narrative_persons narrative_persons_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_persons
    ADD CONSTRAINT narrative_persons_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: narrative_persons narrative_persons_team_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.narrative_persons
    ADD CONSTRAINT narrative_persons_team_fkey FOREIGN KEY (team_id, sport) REFERENCES public.teams(id, sport);


--
-- Name: news_article_entities news_article_entities_article_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_article_entities
    ADD CONSTRAINT news_article_entities_article_id_fkey FOREIGN KEY (article_id) REFERENCES public.news_articles(id) ON DELETE CASCADE;


--
-- Name: news_article_entities news_article_entities_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_article_entities
    ADD CONSTRAINT news_article_entities_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: news_summaries news_summaries_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_summaries
    ADD CONSTRAINT news_summaries_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: notifications notifications_fixture_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_fixture_id_fkey FOREIGN KEY (fixture_id) REFERENCES public.fixtures(id);


--
-- Name: notifications notifications_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: pipeline_stats pipeline_stats_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipeline_stats
    ADD CONSTRAINT pipeline_stats_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: player_current_identity_overrides player_current_identity_overrides_player_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_current_identity_overrides
    ADD CONSTRAINT player_current_identity_overrides_player_id_sport_fkey FOREIGN KEY (player_id, sport) REFERENCES public.players(id, sport) ON DELETE CASCADE;


--
-- Name: player_current_identity_overrides player_current_identity_overrides_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_current_identity_overrides
    ADD CONSTRAINT player_current_identity_overrides_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: player_current_identity_overrides player_current_identity_overrides_team_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_current_identity_overrides
    ADD CONSTRAINT player_current_identity_overrides_team_id_sport_fkey FOREIGN KEY (team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT;


--
-- Name: player_stats player_stats_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_stats
    ADD CONSTRAINT player_stats_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: players players_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.players
    ADD CONSTRAINT players_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: provider_entity_map provider_entity_map_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_entity_map
    ADD CONSTRAINT provider_entity_map_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: provider_fixture_map provider_fixture_map_fixture_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_fixture_map
    ADD CONSTRAINT provider_fixture_map_fixture_id_fkey FOREIGN KEY (fixture_id) REFERENCES public.fixtures(id);


--
-- Name: provider_fixture_map provider_fixture_map_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_fixture_map
    ADD CONSTRAINT provider_fixture_map_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: provider_seasons provider_seasons_league_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_seasons
    ADD CONSTRAINT provider_seasons_league_id_fkey FOREIGN KEY (league_id) REFERENCES public.leagues(id);


--
-- Name: rate_modes rate_modes_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rate_modes
    ADD CONSTRAINT rate_modes_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: rating_thresholds rating_thresholds_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rating_thresholds
    ADD CONSTRAINT rating_thresholds_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: sigil_synthesis sigil_synthesis_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sigil_synthesis
    ADD CONSTRAINT sigil_synthesis_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: sport_autofill_versions sport_autofill_versions_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sport_autofill_versions
    ADD CONSTRAINT sport_autofill_versions_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: stat_definitions stat_definitions_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_definitions
    ADD CONSTRAINT stat_definitions_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: stat_matchups stat_matchups_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_matchups
    ADD CONSTRAINT stat_matchups_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: stat_summaries stat_summaries_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.stat_summaries
    ADD CONSTRAINT stat_summaries_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: team_rosters team_rosters_player_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_rosters
    ADD CONSTRAINT team_rosters_player_id_sport_fkey FOREIGN KEY (player_id, sport) REFERENCES public.players(id, sport) ON DELETE CASCADE;


--
-- Name: team_rosters team_rosters_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_rosters
    ADD CONSTRAINT team_rosters_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: team_rosters team_rosters_team_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_rosters
    ADD CONSTRAINT team_rosters_team_id_sport_fkey FOREIGN KEY (team_id, sport) REFERENCES public.teams(id, sport) ON DELETE CASCADE;


--
-- Name: team_stats team_stats_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_stats
    ADD CONSTRAINT team_stats_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: teams teams_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: topic_heat_embeddings topic_heat_embeddings_article_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.topic_heat_embeddings
    ADD CONSTRAINT topic_heat_embeddings_article_id_fkey FOREIGN KEY (article_id) REFERENCES public.news_articles(id) ON DELETE CASCADE;


--
-- Name: transfer_identity_applications transfer_identity_applications_new_team_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_new_team_id_sport_fkey FOREIGN KEY (new_team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT;


--
-- Name: transfer_identity_applications transfer_identity_applications_old_team_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_old_team_id_sport_fkey FOREIGN KEY (old_team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT;


--
-- Name: transfer_identity_applications transfer_identity_applications_override_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_override_id_fkey FOREIGN KEY (override_id) REFERENCES public.player_current_identity_overrides(id);


--
-- Name: transfer_identity_applications transfer_identity_applications_player_id_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_player_id_sport_fkey FOREIGN KEY (player_id, sport) REFERENCES public.players(id, sport) ON DELETE CASCADE;


--
-- Name: transfer_identity_applications transfer_identity_applications_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_applications
    ADD CONSTRAINT transfer_identity_applications_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: transfer_identity_thresholds transfer_identity_thresholds_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_identity_thresholds
    ADD CONSTRAINT transfer_identity_thresholds_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: transfer_rumors transfer_rumors_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_rumors
    ADD CONSTRAINT transfer_rumors_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: user_devices user_devices_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_devices
    ADD CONSTRAINT user_devices_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: user_follows user_follows_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: user_follows user_follows_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: vibe_scores vibe_scores_sport_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vibe_scores
    ADD CONSTRAINT vibe_scores_sport_fkey FOREIGN KEY (sport) REFERENCES public.sports(id);


--
-- Name: notifications; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.notifications ENABLE ROW LEVEL SECURITY;

--
-- Name: notifications notifications_own; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY notifications_own ON public.notifications FOR SELECT TO web_user USING (((user_id)::text = ((current_setting('request.jwt.claims'::text, true))::json ->> 'sub'::text)));


--
-- Name: user_devices; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.user_devices ENABLE ROW LEVEL SECURITY;

--
-- Name: user_devices user_devices_own; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY user_devices_own ON public.user_devices TO web_user USING (((user_id)::text = ((current_setting('request.jwt.claims'::text, true))::json ->> 'sub'::text))) WITH CHECK (((user_id)::text = ((current_setting('request.jwt.claims'::text, true))::json ->> 'sub'::text)));


--
-- Name: user_follows; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.user_follows ENABLE ROW LEVEL SECURITY;

--
-- Name: user_follows user_follows_own; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY user_follows_own ON public.user_follows TO web_user USING (((user_id)::text = ((current_setting('request.jwt.claims'::text, true))::json ->> 'sub'::text))) WITH CHECK (((user_id)::text = ((current_setting('request.jwt.claims'::text, true))::json ->> 'sub'::text)));


--
-- PostgreSQL database dump complete
--

\unrestrict D9NGRPSJLCYRat2c2f8pZ2ncPpPFHlNWZiIY2jFRWyEqaFY8cHXPs50qwEFsBJ9

