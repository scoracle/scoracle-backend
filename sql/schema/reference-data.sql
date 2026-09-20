--
-- PostgreSQL database dump
--

\restrict 3Ty5sfvkCzBmHDvDKAcd3j9MVKZeJ7IALORSiclduVux9dyNKgTqnFYkSq5bdcP

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: sports; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.sports (id, display_name, api_base_url, current_season, is_active, created_at, updated_at, clock_league_id) VALUES
	('NBA', 'NBA Basketball', NULL, 2025, true, '2026-04-18 14:26:56.095212-04', '2026-04-18 14:26:56.095212-04', NULL),
	('FOOTBALL', 'Football (Soccer)', NULL, 2026, true, '2026-04-18 14:26:56.095212-04', '2026-04-18 14:26:56.095212-04', 8),
	('NFL', 'NFL Football', NULL, 2026, true, '2026-04-18 14:26:56.095212-04', '2026-04-18 14:26:56.095212-04', NULL);


--
-- PostgreSQL database dump complete
--

\unrestrict 3Ty5sfvkCzBmHDvDKAcd3j9MVKZeJ7IALORSiclduVux9dyNKgTqnFYkSq5bdcP

--
-- PostgreSQL database dump
--

\restrict H2tkO6cRSZ6X0zHedpKofWh8JofMgPplIcD6gEvr5DjWkz9LowYjF3leb6qj8Ze

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: leagues; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.leagues (id, sport, name, country, logo_url, sportmonks_id, is_benchmark, is_active, handicap, meta, created_at, updated_at) VALUES
	(8, 'FOOTBALL', 'Premier League', 'England', NULL, 8, true, true, NULL, '{}', '2026-04-18 14:26:56.09834-04', '2026-04-18 14:26:56.09834-04'),
	(82, 'FOOTBALL', 'Bundesliga', 'Germany', NULL, 82, true, true, NULL, '{}', '2026-04-18 14:26:56.09834-04', '2026-04-18 14:26:56.09834-04'),
	(301, 'FOOTBALL', 'Ligue 1', 'France', NULL, 301, true, true, NULL, '{}', '2026-04-18 14:26:56.09834-04', '2026-04-18 14:26:56.09834-04'),
	(384, 'FOOTBALL', 'Serie A', 'Italy', NULL, 384, true, true, NULL, '{}', '2026-04-18 14:26:56.09834-04', '2026-04-18 14:26:56.09834-04'),
	(564, 'FOOTBALL', 'La Liga', 'Spain', NULL, 564, true, true, NULL, '{}', '2026-04-18 14:26:56.09834-04', '2026-04-18 14:26:56.09834-04');


--
-- PostgreSQL database dump complete
--

\unrestrict H2tkO6cRSZ6X0zHedpKofWh8JofMgPplIcD6gEvr5DjWkz9LowYjF3leb6qj8Ze

--
-- PostgreSQL database dump
--

\restrict Lh9ZMrQBbBxyllG5jEwuLVLbZkbrV7quhzwqzi78ZgbIXexYtmkh1MsRedf2W9H

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: provider_seasons; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.provider_seasons (id, league_id, season_year, provider, provider_season_id) VALUES
	(1, 8, 2025, 'sportmonks', 25583),
	(2, 82, 2025, 'sportmonks', 25646),
	(3, 301, 2025, 'sportmonks', 25651),
	(4, 384, 2025, 'sportmonks', 25533),
	(5, 564, 2025, 'sportmonks', 25659),
	(6, 8, 2024, 'sportmonks', 23614),
	(7, 82, 2024, 'sportmonks', 23744),
	(8, 301, 2024, 'sportmonks', 23643),
	(9, 384, 2024, 'sportmonks', 23746),
	(10, 564, 2024, 'sportmonks', 23621),
	(11, 8, 2023, 'sportmonks', 21646),
	(12, 82, 2023, 'sportmonks', 21795),
	(13, 301, 2023, 'sportmonks', 21779),
	(14, 384, 2023, 'sportmonks', 21818),
	(15, 564, 2023, 'sportmonks', 21694),
	(16, 8, 2022, 'sportmonks', 19734),
	(17, 82, 2022, 'sportmonks', 19744),
	(18, 301, 2022, 'sportmonks', 19745),
	(19, 384, 2022, 'sportmonks', 19806),
	(20, 564, 2022, 'sportmonks', 19799),
	(21, 8, 2021, 'sportmonks', 18378),
	(22, 82, 2021, 'sportmonks', 18444),
	(23, 301, 2021, 'sportmonks', 18441),
	(24, 384, 2021, 'sportmonks', 18576),
	(25, 564, 2021, 'sportmonks', 18462),
	(26, 8, 2020, 'sportmonks', 17420),
	(27, 82, 2020, 'sportmonks', 17361),
	(28, 301, 2020, 'sportmonks', 17160),
	(29, 384, 2020, 'sportmonks', 17488),
	(30, 564, 2020, 'sportmonks', 17480),
	(31, 8, 2019, 'sportmonks', 16036),
	(32, 82, 2019, 'sportmonks', 16264),
	(33, 301, 2019, 'sportmonks', 16043),
	(34, 384, 2019, 'sportmonks', 16415),
	(35, 564, 2019, 'sportmonks', 16326),
	(36, 8, 2018, 'sportmonks', 12962),
	(37, 82, 2018, 'sportmonks', 13005),
	(38, 301, 2018, 'sportmonks', 12935),
	(39, 384, 2018, 'sportmonks', 13158),
	(40, 564, 2018, 'sportmonks', 13133);


--
-- Name: provider_seasons_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.provider_seasons_id_seq', 40, true);


--
-- PostgreSQL database dump complete
--

\unrestrict Lh9ZMrQBbBxyllG5jEwuLVLbZkbrV7quhzwqzi78ZgbIXexYtmkh1MsRedf2W9H

--
-- PostgreSQL database dump
--

\restrict 6Qb7g7EY0dCYqzlg3exnCAgZnFSXbEDxo7jnVcAUQMEFnJb257zKotbFwX7O017

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: stat_definitions; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(1332, 'FOOTBALL', 'clean_sheets', 'Clean Sheets', 'player', 'goalkeeper', false, false, true, 40, 'cumulative_total', false, false, NULL),
	(1333, 'FOOTBALL', 'expected_assists', 'Expected Assists (xA)', 'player', 'passing', false, false, true, 41, 'cumulative_total', false, false, NULL),
	(45, 'NFL', 'passing_yards_per_game', 'Pass Yards/Game', 'player', 'passing', false, false, true, 15, 'per_game_avg', true, false, NULL),
	(46, 'NFL', 'passing_completion_pct', 'Completion %', 'player', 'passing', false, false, true, 16, 'rate_pct', true, false, NULL),
	(48, 'NFL', 'td_int_ratio', 'TD/INT Ratio', 'player', 'passing', false, true, true, 18, 'per_game_avg', true, false, NULL),
	(1334, 'FOOTBALL', 'expected_goal_involvements', 'Expected Involvements (xGI)', 'player', 'scoring', false, false, true, 42, 'cumulative_total', false, false, NULL),
	(1335, 'FOOTBALL', 'expected_goals_conceded', 'Expected Goals Conceded (xGC)', 'player', 'goalkeeper', true, false, true, 43, 'cumulative_total', false, false, NULL),
	(52, 'NFL', 'rushing_yards_per_game', 'Rush Yards/Game', 'player', 'rushing', false, false, true, 23, 'per_game_avg', true, false, NULL),
	(53, 'NFL', 'yards_per_rush_attempt', 'Yards/Carry', 'player', 'rushing', false, false, true, 24, 'cumulative_total', false, false, NULL),
	(1330, 'FOOTBALL', 'fantasy_points_per_90', 'Fantasy Points Per 90', 'player', 'fantasy', false, true, true, 91, NULL, false, false, NULL),
	(1331, 'FOOTBALL', 'fantasy_points_per_game', 'Fantasy Points Per Game', 'player', 'fantasy', false, true, true, 92, NULL, false, false, NULL),
	(1329, 'FOOTBALL', 'fantasy_points', 'Fantasy Points', 'player', 'fantasy', false, true, true, 90, NULL, false, true, NULL),
	(1336, 'FOOTBALL', 'defensive_contribution', 'Defensive Contribution', 'player', 'defensive', false, false, true, 44, 'cumulative_total', false, false, NULL),
	(1337, 'FOOTBALL', 'cbi', 'Clearances/Blocks/Intercepts', 'player', 'defensive', false, false, true, 45, 'cumulative_total', false, false, NULL),
	(59, 'NFL', 'receiving_yards_per_game', 'Receiving Yards/Game', 'player', 'receiving', false, false, true, 34, 'per_game_avg', true, false, NULL),
	(60, 'NFL', 'yards_per_reception', 'Yards/Reception', 'player', 'receiving', false, false, true, 35, 'per_game_avg', true, false, NULL),
	(1338, 'FOOTBALL', 'bonus_points', 'Bonus Points', 'player', 'fantasy', false, false, true, 46, 'cumulative_total', false, false, NULL),
	(62, 'NFL', 'catch_pct', 'Catch %', 'player', 'receiving', false, true, true, 37, 'rate_pct', true, false, NULL),
	(1339, 'FOOTBALL', 'ict_index', 'ICT Index', 'player', 'fantasy', false, false, true, 47, 'cumulative_total', false, false, NULL),
	(1340, 'FOOTBALL', 'bps', 'Bonus Point System', 'player', 'fantasy', false, false, false, 48, 'cumulative_total', false, false, NULL),
	(1341, 'FOOTBALL', 'influence', 'Influence', 'player', 'fantasy', false, false, false, 49, 'cumulative_total', false, false, NULL),
	(1342, 'FOOTBALL', 'creativity', 'Creativity', 'player', 'fantasy', false, false, false, 50, 'cumulative_total', false, false, NULL),
	(1343, 'FOOTBALL', 'threat', 'Threat', 'player', 'fantasy', false, false, false, 51, 'cumulative_total', false, false, NULL),
	(1344, 'FOOTBALL', 'clean_sheets', 'Clean Sheets', 'team', 'defensive', false, false, true, 40, 'cumulative_total', false, false, NULL),
	(1345, 'FOOTBALL', 'expected_goals_for', 'Expected Goals For (xG)', 'team', 'attacking', false, false, true, 41, 'cumulative_total', false, false, NULL),
	(1346, 'FOOTBALL', 'expected_goals_against', 'Expected Goals Against (xGA)', 'team', 'defensive', true, false, true, 42, 'cumulative_total', false, false, NULL),
	(1347, 'FOOTBALL', 'expected_assists_per_90', 'xA Per 90', 'player', 'passing', false, true, false, 210, 'per_game_avg', true, false, NULL),
	(1348, 'FOOTBALL', 'expected_assists_per_game', 'xA Per Game', 'player', 'passing', false, true, true, 310, '', false, false, NULL),
	(1349, 'FOOTBALL', 'defensive_contribution_per_90', 'Defensive Contrib. Per 90', 'player', 'defensive', false, true, false, 211, 'per_game_avg', true, false, NULL),
	(74, 'NFL', 'field_goal_pct', 'FG Percentage', 'player', 'kicking', false, false, true, 52, 'rate_pct', true, false, NULL),
	(1350, 'FOOTBALL', 'defensive_contribution_per_game', 'Defensive Contrib. Per Game', 'player', 'defensive', false, true, true, 311, '', false, false, NULL),
	(1351, 'FOOTBALL', 'cbi_per_90', 'CBI Per 90', 'player', 'defensive', false, true, false, 212, 'per_game_avg', true, false, NULL),
	(1352, 'FOOTBALL', 'cbi_per_game', 'CBI Per Game', 'player', 'defensive', false, true, true, 312, '', false, false, NULL),
	(1263, 'NBA', 'pts_per_season', 'Total Points', 'player', 'advanced', false, true, true, 50, NULL, false, false, NULL),
	(1264, 'NBA', 'reb_per_season', 'Total Rebounds', 'player', 'advanced', false, true, true, 51, NULL, false, false, NULL),
	(95, 'NFL', 'ties', 'Ties', 'team', 'standings', false, false, false, 3, 'special', false, false, NULL),
	(1265, 'NBA', 'ast_per_season', 'Total Assists', 'player', 'advanced', false, true, true, 52, NULL, false, false, NULL),
	(1266, 'NBA', 'stl_per_season', 'Total Steals', 'player', 'advanced', false, true, true, 53, NULL, false, false, NULL),
	(1267, 'NBA', 'blk_per_season', 'Total Blocks', 'player', 'advanced', false, true, true, 54, NULL, false, false, NULL),
	(75, 'NFL', 'punts', 'Punts', 'player', 'special', false, false, true, 60, 'cumulative_total', false, true, NULL),
	(100, 'NFL', 'passing_yards', 'Passing Yards', 'team', 'offense', false, false, true, 10, 'cumulative_total', true, false, NULL),
	(101, 'NFL', 'passing_touchdowns', 'Passing TDs', 'team', 'offense', false, false, true, 11, 'cumulative_total', true, false, NULL),
	(102, 'NFL', 'passing_attempts', 'Pass Attempts', 'team', 'offense', false, false, true, 12, 'cumulative_total', true, false, NULL),
	(103, 'NFL', 'passing_completions', 'Completions', 'team', 'offense', false, false, true, 13, 'cumulative_total', true, false, NULL),
	(104, 'NFL', 'passing_interceptions', 'Interceptions Thrown', 'team', 'offense', true, false, true, 14, 'cumulative_total', true, false, NULL),
	(105, 'NFL', 'rushing_yards', 'Rushing Yards', 'team', 'offense', false, false, true, 15, 'cumulative_total', true, false, NULL),
	(106, 'NFL', 'rushing_touchdowns', 'Rushing TDs', 'team', 'offense', false, false, true, 16, 'cumulative_total', true, false, NULL),
	(107, 'NFL', 'rushing_attempts', 'Rush Attempts', 'team', 'offense', false, false, true, 17, 'cumulative_total', true, false, NULL),
	(108, 'NFL', 'total_yards', 'Total Yards', 'team', 'offense', false, true, true, 18, 'per_game_avg', true, false, NULL),
	(109, 'NFL', 'defensive_sacks', 'Sacks', 'team', 'defense', false, false, true, 20, 'cumulative_total', true, false, NULL),
	(110, 'NFL', 'defensive_interceptions', 'Interceptions', 'team', 'defense', false, false, true, 21, 'cumulative_total', true, false, NULL),
	(111, 'NFL', 'total_tackles', 'Total Tackles', 'team', 'defense', false, false, true, 22, 'cumulative_total', true, false, NULL),
	(112, 'NFL', 'passes_defended', 'Passes Defended', 'team', 'defense', false, false, true, 23, 'cumulative_total', true, false, NULL),
	(113, 'NFL', 'fumbles_lost', 'Fumbles Lost', 'team', 'turnovers', true, false, true, 30, 'cumulative_total', true, false, NULL),
	(114, 'NFL', 'turnovers', 'Total Turnovers', 'team', 'turnovers', true, true, true, 31, 'per_game_avg', true, false, NULL),
	(115, 'NFL', 'field_goals_made', 'FG Made', 'team', 'kicking', false, false, true, 40, 'cumulative_total', true, false, NULL),
	(116, 'NFL', 'field_goal_attempts', 'FG Attempts', 'team', 'kicking', false, false, true, 41, 'cumulative_total', true, false, NULL),
	(117, 'NFL', 'field_goal_pct', 'FG Percentage', 'team', 'kicking', false, true, true, 42, 'rate_pct', true, false, NULL),
	(118, 'FOOTBALL', 'appearances', 'Appearances', 'player', 'general', false, false, true, 1, 'cumulative_total', false, false, NULL),
	(119, 'FOOTBALL', 'lineups', 'Starting Lineups', 'player', 'general', false, false, false, 2, 'cumulative_total', false, false, NULL),
	(120, 'FOOTBALL', 'minutes_played', 'Minutes Played', 'player', 'general', false, false, false, 3, 'cumulative_total', false, false, NULL),
	(1268, 'NBA', 'tov_per_season', 'Total Turnovers', 'player', 'advanced', true, true, true, 55, NULL, false, false, NULL),
	(1269, 'NBA', 'pf_per_season', 'Total Fouls', 'player', 'advanced', true, true, false, 56, NULL, false, false, NULL),
	(1270, 'NBA', 'oreb_per_season', 'Total Off Rebounds', 'player', 'advanced', false, true, true, 57, NULL, false, false, NULL),
	(129, 'FOOTBALL', 'shot_accuracy', 'Shot Accuracy %', 'player', 'shooting', false, true, true, 23, 'rate_pct', true, false, NULL),
	(1271, 'NBA', 'dreb_per_season', 'Total Def Rebounds', 'player', 'advanced', false, true, true, 58, NULL, false, false, NULL),
	(136, 'FOOTBALL', 'pass_accuracy', 'Pass Accuracy %', 'player', 'passing', false, true, true, 36, 'rate_pct', true, false, NULL),
	(1272, 'NBA', 'fgm_per_season', 'Total FG Made', 'player', 'advanced', false, true, true, 59, NULL, false, false, NULL),
	(1273, 'NBA', 'fga_per_season', 'Total FG Attempted', 'player', 'advanced', false, true, true, 60, NULL, false, false, NULL),
	(145, 'FOOTBALL', 'duel_success_rate', 'Duel Success Rate %', 'player', 'duels', false, true, true, 52, 'rate_pct', true, false, NULL),
	(148, 'FOOTBALL', 'dribble_success_rate', 'Dribble Success %', 'player', 'dribbling', false, true, true, 57, 'rate_pct', true, false, NULL),
	(149, 'FOOTBALL', 'yellow_cards', 'Yellow Cards', 'player', 'discipline', true, false, true, 60, 'cumulative_total', false, false, NULL),
	(150, 'FOOTBALL', 'red_cards', 'Red Cards', 'player', 'discipline', true, false, true, 61, 'cumulative_total', false, false, NULL),
	(1274, 'NBA', 'fg3m_per_season', 'Total 3PT Made', 'player', 'advanced', false, true, true, 61, NULL, false, false, NULL),
	(156, 'FOOTBALL', 'save_pct', 'Save Percentage %', 'player', 'goalkeeper', false, true, true, 73, 'rate_pct', true, false, NULL),
	(157, 'FOOTBALL', 'matches_played', 'Matches Played', 'team', 'standings', false, false, false, 1, 'special', false, false, NULL),
	(1275, 'NBA', 'fg3a_per_season', 'Total 3PT Attempted', 'player', 'advanced', false, true, true, 62, NULL, false, false, NULL),
	(1276, 'NBA', 'ftm_per_season', 'Total FT Made', 'player', 'advanced', false, true, true, 63, NULL, false, false, NULL),
	(1277, 'NBA', 'fta_per_season', 'Total FT Attempted', 'player', 'advanced', false, true, true, 64, NULL, false, false, NULL),
	(161, 'FOOTBALL', 'goals_for', 'Goals For', 'team', 'scoring', false, false, true, 5, 'cumulative_total', true, false, NULL),
	(1278, 'FOOTBALL', 'goals_per_game', 'Goals Per Game', 'player', 'scoring', false, true, true, 300, NULL, false, false, NULL),
	(1279, 'FOOTBALL', 'assists_per_game', 'Assists Per Game', 'player', 'scoring', false, true, true, 301, NULL, false, false, NULL),
	(1280, 'FOOTBALL', 'expected_goals_per_game', 'xG Per Game', 'player', 'scoring', false, true, true, 302, NULL, false, false, NULL),
	(1281, 'FOOTBALL', 'shots_per_game', 'Shots Per Game', 'player', 'shooting', false, true, true, 303, NULL, false, false, NULL),
	(166, 'FOOTBALL', 'position', 'League Position', 'team', 'standings', false, false, false, 10, 'special', false, false, NULL),
	(167, 'FOOTBALL', 'home_played', 'Home Matches', 'team', 'home', false, false, false, 20, 'special', false, false, NULL),
	(168, 'FOOTBALL', 'home_won', 'Home Wins', 'team', 'home', false, false, false, 21, 'special', false, false, NULL),
	(169, 'FOOTBALL', 'home_draw', 'Home Draws', 'team', 'home', false, false, false, 22, 'special', false, false, NULL),
	(170, 'FOOTBALL', 'home_lost', 'Home Losses', 'team', 'home', false, false, false, 23, 'special', false, false, NULL),
	(171, 'FOOTBALL', 'home_scored', 'Home Goals Scored', 'team', 'home', false, false, false, 24, 'special', false, false, NULL),
	(172, 'FOOTBALL', 'home_conceded', 'Home Goals Conceded', 'team', 'home', false, false, false, 25, 'special', false, false, NULL),
	(173, 'FOOTBALL', 'home_points', 'Home Points', 'team', 'home', false, false, false, 26, 'special', false, false, NULL),
	(174, 'FOOTBALL', 'away_played', 'Away Matches', 'team', 'away', false, false, false, 30, 'special', false, false, NULL),
	(175, 'FOOTBALL', 'away_won', 'Away Wins', 'team', 'away', false, false, false, 31, 'special', false, false, NULL),
	(176, 'FOOTBALL', 'away_draw', 'Away Draws', 'team', 'away', false, false, false, 32, 'special', false, false, NULL),
	(177, 'FOOTBALL', 'away_lost', 'Away Losses', 'team', 'away', false, false, false, 33, 'special', false, false, NULL),
	(178, 'FOOTBALL', 'away_scored', 'Away Goals Scored', 'team', 'away', false, false, false, 34, 'special', false, false, NULL),
	(179, 'FOOTBALL', 'away_conceded', 'Away Goals Conceded', 'team', 'away', false, false, false, 35, 'special', false, false, NULL),
	(180, 'FOOTBALL', 'away_points', 'Away Points', 'team', 'away', false, false, false, 36, 'special', false, false, NULL),
	(244, 'FOOTBALL', 'fouls_committed', 'Fouls Committed', 'team', 'discipline', true, false, true, 40, 'cumulative_total', true, false, NULL);
INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(245, 'FOOTBALL', 'yellow_cards_total', 'Yellow Cards', 'team', 'discipline', true, false, true, 41, 'cumulative_total', true, false, NULL),
	(246, 'FOOTBALL', 'red_cards_total', 'Red Cards', 'team', 'discipline', true, false, true, 42, 'cumulative_total', true, false, NULL),
	(287, 'FOOTBALL', 'punches', 'Punches', 'player', 'goalkeeper', false, false, true, 75, 'cumulative_total', false, false, NULL),
	(288, 'FOOTBALL', 'penalties_saved', 'Penalties Saved', 'player', 'goalkeeper', false, false, true, 76, 'cumulative_total', false, false, NULL),
	(289, 'FOOTBALL', 'good_high_claim', 'High Claims', 'player', 'goalkeeper', false, false, true, 77, 'cumulative_total', false, false, NULL),
	(290, 'FOOTBALL', 'penalty_goals', 'Penalty Goals', 'player', 'scoring', false, false, true, 15, 'cumulative_total', false, false, NULL),
	(291, 'FOOTBALL', 'penalties_missed', 'Penalties Missed', 'player', 'scoring', true, false, true, 16, 'cumulative_total', false, false, NULL),
	(292, 'FOOTBALL', 'penalties_won', 'Penalties Won', 'player', 'scoring', false, false, true, 17, 'cumulative_total', false, false, NULL),
	(293, 'FOOTBALL', 'penalties_committed', 'Penalties Committed', 'player', 'discipline', true, false, true, 64, 'cumulative_total', false, false, NULL),
	(294, 'FOOTBALL', 'penalties_scored', 'Penalties Scored', 'player', 'scoring', false, false, true, 18, 'cumulative_total', false, false, NULL),
	(295, 'FOOTBALL', 'own_goals', 'Own Goals', 'player', 'scoring', true, false, true, 19, 'cumulative_total', false, false, NULL),
	(296, 'FOOTBALL', 'yellowred_cards', 'Second Yellows', 'player', 'discipline', true, false, true, 65, 'cumulative_total', false, false, NULL),
	(297, 'FOOTBALL', 'hit_woodwork', 'Hit Woodwork', 'player', 'shooting', false, false, true, 24, 'cumulative_total', false, false, NULL),
	(298, 'FOOTBALL', 'shots_off_target', 'Shots off Target', 'player', 'shooting', false, false, true, 25, 'cumulative_total', false, false, NULL),
	(299, 'FOOTBALL', 'shots_blocked', 'Shots Blocked', 'player', 'shooting', true, false, true, 26, 'cumulative_total', false, false, NULL),
	(302, 'FOOTBALL', 'big_chances_missed', 'Big Chances Missed', 'player', 'shooting', true, false, true, 27, 'cumulative_total', false, false, NULL),
	(305, 'FOOTBALL', 'long_ball_accuracy', 'Long Ball Accuracy %', 'player', 'passing', false, true, true, 47, 'rate_pct', true, false, NULL),
	(308, 'FOOTBALL', 'backward_passes', 'Backward Passes', 'player', 'passing', false, false, true, 66, 'cumulative_total', false, false, NULL),
	(310, 'FOOTBALL', 'cross_accuracy', 'Cross Accuracy %', 'player', 'passing', false, true, true, 68, 'rate_pct', true, false, NULL),
	(312, 'FOOTBALL', 'tackles_won_percentage', 'Tackle Success %', 'player', 'defensive', false, true, true, 79, 'rate_pct', true, false, NULL),
	(313, 'FOOTBALL', 'last_man_tackle', 'Last Man Tackles', 'player', 'defensive', false, false, true, 80, 'cumulative_total', false, false, NULL),
	(314, 'FOOTBALL', 'clearance_offline', 'Goal-line Clearances', 'player', 'defensive', false, false, true, 81, 'cumulative_total', false, false, NULL),
	(315, 'FOOTBALL', 'error_lead_to_shot', 'Errors Leading to Shot', 'player', 'defensive', true, false, true, 82, 'cumulative_total', false, false, NULL),
	(316, 'FOOTBALL', 'error_lead_to_goal', 'Errors Leading to Goal', 'player', 'defensive', true, false, true, 83, 'cumulative_total', false, false, NULL),
	(317, 'FOOTBALL', 'duels_lost', 'Duels Lost', 'player', 'duels', true, false, true, 53, 'cumulative_total', false, false, NULL),
	(320, 'FOOTBALL', 'aeriels_lost', 'Aerials Lost', 'player', 'duels', true, false, true, 59, 'cumulative_total', false, false, NULL),
	(321, 'FOOTBALL', 'aerials_won_percentage', 'Aerial Success %', 'player', 'duels', false, true, true, 69, 'rate_pct', true, false, NULL),
	(326, 'FOOTBALL', 'touches', 'Touches', 'player', 'possession', false, false, true, 88, 'cumulative_total', false, false, NULL),
	(328, 'FOOTBALL', 'offsides', 'Offsides', 'player', 'discipline', true, false, true, 90, 'cumulative_total', false, false, NULL),
	(329, 'FOOTBALL', 'offsides_provoked', 'Offsides Won', 'player', 'defensive', false, false, true, 91, 'cumulative_total', false, false, NULL),
	(330, 'FOOTBALL', 'motm_awards', 'Man of the Match', 'player', 'general', false, false, true, 4, 'cumulative_total', false, false, NULL),
	(331, 'FOOTBALL', 'rating_avg', 'Average Match Rating', 'player', 'general', false, true, true, 5, 'per_game_avg', true, false, NULL),
	(359, 'FOOTBALL', 'fouls_drawn', 'Fouls Drawn', 'team', 'discipline', false, false, true, 43, 'cumulative_total', true, false, NULL),
	(360, 'FOOTBALL', 'tackles', 'Tackles', 'team', 'defensive', false, false, true, 50, 'cumulative_total', true, false, NULL),
	(361, 'FOOTBALL', 'tackles_won', 'Tackles Won', 'team', 'defensive', false, false, true, 51, 'cumulative_total', true, false, NULL),
	(362, 'FOOTBALL', 'tackles_won_percentage', 'Tackle Success %', 'team', 'defensive', false, true, true, 52, 'rate_pct', true, false, NULL),
	(363, 'FOOTBALL', 'interceptions', 'Interceptions', 'team', 'defensive', false, false, true, 53, 'cumulative_total', true, false, NULL),
	(364, 'FOOTBALL', 'clearances', 'Clearances', 'team', 'defensive', false, false, true, 54, 'cumulative_total', true, false, NULL),
	(365, 'FOOTBALL', 'blocked_shots', 'Blocked Shots (Defensive)', 'team', 'defensive', false, false, true, 55, 'cumulative_total', true, false, NULL),
	(366, 'FOOTBALL', 'ball_recovery', 'Ball Recoveries', 'team', 'defensive', false, false, true, 56, 'cumulative_total', true, false, NULL),
	(367, 'FOOTBALL', 'dispossessed', 'Dispossessed', 'team', 'possession', true, false, true, 57, 'cumulative_total', true, false, NULL),
	(368, 'FOOTBALL', 'possession_lost', 'Possession Lost', 'team', 'possession', true, false, true, 58, 'cumulative_total', true, false, NULL),
	(369, 'FOOTBALL', 'dribbled_past', 'Dribbled Past', 'team', 'defensive', true, false, true, 59, 'cumulative_total', true, false, NULL),
	(370, 'FOOTBALL', 'passes', 'Total Passes', 'team', 'passing', false, false, true, 60, 'cumulative_total', true, false, NULL),
	(371, 'FOOTBALL', 'accurate_passes', 'Accurate Passes', 'team', 'passing', false, false, true, 61, 'cumulative_total', true, false, NULL),
	(372, 'FOOTBALL', 'pass_accuracy', 'Pass Accuracy %', 'team', 'passing', false, true, true, 62, 'rate_pct', true, false, NULL),
	(373, 'FOOTBALL', 'key_passes', 'Key Passes', 'team', 'passing', false, false, true, 63, 'cumulative_total', true, false, NULL),
	(374, 'FOOTBALL', 'backward_passes', 'Backward Passes', 'team', 'passing', false, false, true, 64, 'cumulative_total', true, false, NULL),
	(375, 'FOOTBALL', 'passes_final_third', 'Passes in Final Third', 'team', 'passing', false, false, true, 65, 'cumulative_total', true, false, NULL),
	(376, 'FOOTBALL', 'long_balls', 'Long Balls', 'team', 'passing', false, false, true, 66, 'cumulative_total', true, false, NULL),
	(377, 'FOOTBALL', 'long_balls_won', 'Long Balls Won', 'team', 'passing', false, false, true, 67, 'cumulative_total', true, false, NULL),
	(378, 'FOOTBALL', 'long_ball_accuracy', 'Long Ball Accuracy %', 'team', 'passing', false, true, true, 68, 'rate_pct', true, false, NULL),
	(379, 'FOOTBALL', 'through_balls', 'Through Balls', 'team', 'passing', false, false, true, 69, 'cumulative_total', true, false, NULL),
	(380, 'FOOTBALL', 'total_crosses', 'Total Crosses', 'team', 'passing', false, false, true, 70, 'cumulative_total', true, false, NULL),
	(381, 'FOOTBALL', 'accurate_crosses', 'Accurate Crosses', 'team', 'passing', false, false, true, 71, 'cumulative_total', true, false, NULL),
	(382, 'FOOTBALL', 'cross_accuracy', 'Cross Accuracy %', 'team', 'passing', false, true, true, 72, 'rate_pct', true, false, NULL),
	(383, 'FOOTBALL', 'shots_total', 'Total Shots', 'team', 'shooting', false, false, true, 80, 'cumulative_total', true, false, NULL),
	(384, 'FOOTBALL', 'shots_on_target', 'Shots on Target', 'team', 'shooting', false, false, true, 81, 'cumulative_total', true, false, NULL),
	(385, 'FOOTBALL', 'shots_off_target', 'Shots off Target', 'team', 'shooting', false, false, true, 82, 'cumulative_total', true, false, NULL),
	(386, 'FOOTBALL', 'shot_accuracy', 'Shot Accuracy %', 'team', 'shooting', false, true, true, 83, 'rate_pct', true, false, NULL),
	(387, 'FOOTBALL', 'shots_blocked_by_opp', 'Shots Blocked by Opponent', 'team', 'shooting', true, false, true, 84, 'cumulative_total', true, false, NULL),
	(388, 'FOOTBALL', 'chances_created', 'Chances Created', 'team', 'attacking', false, false, true, 85, 'cumulative_total', true, false, NULL),
	(389, 'FOOTBALL', 'big_chances_created', 'Big Chances Created', 'team', 'attacking', false, false, true, 86, 'cumulative_total', true, false, NULL),
	(390, 'FOOTBALL', 'big_chances_missed', 'Big Chances Missed', 'team', 'attacking', true, false, true, 87, 'cumulative_total', true, false, NULL),
	(391, 'FOOTBALL', 'dribble_attempts', 'Dribble Attempts', 'team', 'attacking', false, false, true, 88, 'cumulative_total', true, false, NULL),
	(392, 'FOOTBALL', 'successful_dribbles', 'Successful Dribbles', 'team', 'attacking', false, false, true, 89, 'cumulative_total', true, false, NULL),
	(393, 'FOOTBALL', 'dribble_success_rate', 'Dribble Success %', 'team', 'attacking', false, true, true, 90, 'rate_pct', true, false, NULL),
	(394, 'FOOTBALL', 'total_duels', 'Total Duels', 'team', 'duels', false, false, true, 100, 'cumulative_total', true, false, NULL),
	(395, 'FOOTBALL', 'duels_won', 'Duels Won', 'team', 'duels', false, false, true, 101, 'cumulative_total', true, false, NULL),
	(396, 'FOOTBALL', 'duels_lost', 'Duels Lost', 'team', 'duels', true, false, true, 102, 'cumulative_total', true, false, NULL),
	(397, 'FOOTBALL', 'duels_won_percentage', 'Duel Success %', 'team', 'duels', false, true, true, 103, 'rate_pct', true, false, NULL),
	(398, 'FOOTBALL', 'aerials_total', 'Aerials', 'team', 'duels', false, false, true, 104, 'cumulative_total', true, false, NULL),
	(399, 'FOOTBALL', 'aerials_won', 'Aerials Won', 'team', 'duels', false, false, true, 105, 'cumulative_total', true, false, NULL),
	(400, 'FOOTBALL', 'aerials_lost', 'Aerials Lost', 'team', 'duels', true, false, true, 106, 'cumulative_total', true, false, NULL),
	(401, 'FOOTBALL', 'aerials_won_percentage', 'Aerial Success %', 'team', 'duels', false, true, true, 107, 'rate_pct', true, false, NULL),
	(402, 'FOOTBALL', 'touches', 'Touches', 'team', 'possession', false, false, true, 110, 'cumulative_total', true, false, NULL),
	(403, 'FOOTBALL', 'turnovers', 'Turnovers', 'team', 'possession', true, false, true, 111, 'cumulative_total', true, false, NULL),
	(404, 'FOOTBALL', 'offsides', 'Offsides', 'team', 'attacking', true, false, true, 112, 'cumulative_total', true, false, NULL),
	(405, 'FOOTBALL', 'offsides_provoked', 'Offsides Won', 'team', 'defensive', false, false, true, 113, 'cumulative_total', true, false, NULL),
	(406, 'FOOTBALL', 'saves', 'Saves', 'team', 'goalkeeper', false, false, true, 120, 'cumulative_total', true, false, NULL),
	(407, 'FOOTBALL', 'saves_insidebox', 'Saves Inside Box', 'team', 'goalkeeper', false, false, true, 121, 'cumulative_total', true, false, NULL),
	(408, 'FOOTBALL', 'good_high_claim', 'High Claims', 'team', 'goalkeeper', false, false, true, 122, 'cumulative_total', true, false, NULL),
	(599, 'NBA', 'efg_pct', 'Effective FG %', 'player', 'advanced', false, true, true, 37, 'rate_pct', true, false, NULL),
	(600, 'NBA', 'ast_to_tov', 'Assist/Turnover Ratio', 'player', 'advanced', false, true, true, 38, 'per_game_avg', true, false, NULL),
	(1282, 'FOOTBALL', 'shots_on_target_per_game', 'Shots on Target/Game', 'player', 'shooting', false, true, true, 304, NULL, false, false, NULL),
	(1283, 'FOOTBALL', 'key_passes_per_game', 'Key Passes Per Game', 'player', 'passing', false, true, true, 305, NULL, false, false, NULL),
	(613, 'NBA', 'pts_allowed', 'Points Allowed/Game', 'team', 'defensive', true, false, true, 11, 'per_game_avg', true, false, NULL),
	(1284, 'FOOTBALL', 'passes_total_per_game', 'Passes Per Game', 'player', 'passing', false, true, true, 306, NULL, false, false, NULL),
	(615, 'NBA', 'stl', 'Steals Per Game', 'team', 'defensive', false, false, true, 13, 'per_game_avg', true, false, NULL),
	(616, 'NBA', 'blk', 'Blocks Per Game', 'team', 'defensive', false, false, true, 14, 'per_game_avg', true, false, NULL),
	(617, 'NBA', 'oreb', 'Off Rebounds/Game', 'team', 'rebounding', false, false, true, 15, 'per_game_avg', true, false, NULL),
	(618, 'NBA', 'dreb', 'Def Rebounds/Game', 'team', 'rebounding', false, false, true, 16, 'per_game_avg', true, false, NULL),
	(619, 'NBA', 'turnover', 'Turnovers Per Game', 'team', 'general', true, false, true, 17, 'per_game_avg', true, false, NULL),
	(620, 'NBA', 'pf', 'Fouls Per Game', 'team', 'general', true, false, true, 18, 'per_game_avg', true, false, NULL),
	(621, 'NBA', 'fgm', 'Field Goals Made', 'team', 'shooting', false, false, true, 19, 'per_game_avg', true, false, NULL),
	(622, 'NBA', 'fga', 'Field Goals Attempted', 'team', 'shooting', false, false, true, 20, 'per_game_avg', true, false, NULL),
	(623, 'NBA', 'fg3m', 'Three-Pointers Made', 'team', 'shooting', false, false, true, 21, 'per_game_avg', true, false, NULL),
	(624, 'NBA', 'fg3a', 'Three-Pointers Att', 'team', 'shooting', false, false, true, 22, 'per_game_avg', true, false, NULL),
	(625, 'NBA', 'ftm', 'Free Throws Made', 'team', 'shooting', false, false, true, 23, 'per_game_avg', true, false, NULL),
	(626, 'NBA', 'fta', 'Free Throws Attempted', 'team', 'shooting', false, false, true, 24, 'per_game_avg', true, false, NULL);
INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(627, 'NBA', 'true_shooting_pct', 'True Shooting %', 'team', 'advanced', false, true, true, 25, 'rate_pct', true, false, NULL),
	(628, 'NBA', 'efg_pct', 'Effective FG %', 'team', 'advanced', false, true, true, 26, 'rate_pct', true, false, NULL),
	(629, 'NBA', 'ast_to_tov', 'Assist/Turnover Ratio', 'team', 'advanced', false, true, true, 27, 'per_game_avg', true, false, NULL),
	(630, 'NBA', 'efficiency', 'Efficiency Rating', 'team', 'advanced', false, true, true, 28, 'rate_pct', true, false, NULL),
	(685, 'NFL', 'qb_rating', 'Passer Rating (NFL)', 'player', 'passing', false, false, true, 19, 'rate_pct', true, false, NULL),
	(686, 'NFL', 'yards_per_pass_attempt', 'Yards/Attempt', 'player', 'passing', false, true, true, 26, 'per_game_avg', true, false, NULL),
	(689, 'NFL', 'long_pass', 'Longest Pass', 'player', 'passing', false, false, true, 29, 'cumulative_total', false, false, NULL),
	(690, 'NFL', 'long_rushing', 'Longest Rush', 'player', 'rushing', false, false, true, 70, 'cumulative_total', false, false, NULL),
	(691, 'NFL', 'long_reception', 'Longest Reception', 'player', 'receiving', false, false, true, 71, 'cumulative_total', false, false, NULL),
	(692, 'NFL', 'long_field_goal_made', 'Longest FG Made', 'player', 'kicking', false, false, true, 72, 'cumulative_total', false, false, NULL),
	(693, 'NFL', 'long_punt', 'Longest Punt', 'player', 'special', false, false, true, 73, 'cumulative_total', false, false, NULL),
	(694, 'NFL', 'long_kick_return', 'Longest Kick Return', 'player', 'special', false, false, true, 74, 'cumulative_total', false, false, NULL),
	(695, 'NFL', 'long_punt_return', 'Longest Punt Return', 'player', 'special', false, false, true, 75, 'cumulative_total', false, false, NULL),
	(696, 'NFL', 'avg_punt_yards', 'Yards/Punt', 'player', 'special', false, true, true, 76, 'per_game_avg', true, false, NULL),
	(697, 'NFL', 'yards_per_kick_return', 'Yards/Kick Return', 'player', 'special', false, true, true, 77, 'per_game_avg', true, false, NULL),
	(698, 'NFL', 'yards_per_punt_return', 'Yards/Punt Return', 'player', 'special', false, true, true, 78, 'per_game_avg', true, false, NULL),
	(727, 'NFL', 'extra_points_made', 'Extra Points Made', 'team', 'kicking', false, false, true, 43, 'cumulative_total', true, false, NULL),
	(728, 'NFL', 'games_played', 'Games Played', 'team', 'general', false, false, false, 0, 'special', false, false, NULL),
	(1285, 'FOOTBALL', 'passes_accurate_per_game', 'Accurate Passes/Game', 'player', 'passing', false, true, true, 307, NULL, false, false, NULL),
	(1286, 'FOOTBALL', 'crosses_total_per_game', 'Crosses Per Game', 'player', 'passing', false, true, true, 308, NULL, false, false, NULL),
	(1287, 'FOOTBALL', 'crosses_accurate_per_game', 'Accurate Crosses/Game', 'player', 'passing', false, true, true, 309, NULL, false, false, NULL),
	(732, 'NFL', 'yards_per_rush_attempt', 'Yards/Carry', 'team', 'offense', false, true, true, 24, 'per_game_avg', true, false, NULL),
	(733, 'NFL', 'yards_per_pass_attempt', 'Yards/Attempt', 'team', 'offense', false, true, true, 25, 'per_game_avg', true, false, NULL),
	(734, 'NFL', 'passing_completion_pct', 'Completion %', 'team', 'offense', false, true, true, 26, 'rate_pct', true, false, NULL),
	(735, 'NFL', 'solo_tackles', 'Solo Tackles', 'team', 'defense', false, false, true, 27, 'cumulative_total', true, false, NULL),
	(736, 'NFL', 'tackles_for_loss', 'Tackles for Loss', 'team', 'defense', false, false, true, 28, 'cumulative_total', true, false, NULL),
	(738, 'NFL', 'interception_touchdowns', 'INT Return TDs', 'team', 'defense', false, false, true, 32, 'cumulative_total', true, false, NULL),
	(739, 'NFL', 'fumbles_recovered', 'Fumbles Recovered', 'team', 'defense', false, false, true, 33, 'cumulative_total', true, false, NULL),
	(740, 'NFL', 'fumbles_touchdowns', 'Fumble Return TDs', 'team', 'defense', false, false, true, 34, 'cumulative_total', true, false, NULL),
	(741, 'NFL', 'fumbles', 'Fumbles', 'team', 'turnovers', true, false, true, 35, 'cumulative_total', true, false, NULL),
	(742, 'NFL', 'takeaways', 'Takeaways', 'team', 'defense', false, true, true, 36, 'per_game_avg', true, false, NULL),
	(743, 'NFL', 'turnover_differential', 'Turnover Differential', 'team', 'turnovers', false, true, true, 37, 'per_game_avg', true, false, NULL),
	(744, 'NFL', 'punts', 'Punts', 'team', 'special', false, false, true, 50, 'cumulative_total', true, false, NULL),
	(745, 'NFL', 'punt_yards', 'Punt Yards', 'team', 'special', false, false, true, 51, 'cumulative_total', true, false, NULL),
	(746, 'NFL', 'punts_inside_20', 'Punts Inside 20', 'team', 'special', false, false, true, 52, 'cumulative_total', true, false, NULL),
	(747, 'NFL', 'gross_avg_punt_yards', 'Yards/Punt', 'team', 'special', false, true, true, 53, 'per_game_avg', true, false, NULL),
	(750, 'NFL', 'kick_return_yards', 'Kick Return Yards', 'team', 'special', false, false, true, 56, 'cumulative_total', true, false, NULL),
	(751, 'NFL', 'kick_return_touchdowns', 'Kick Return TDs', 'team', 'special', false, false, true, 57, 'cumulative_total', true, false, NULL),
	(752, 'NFL', 'yards_per_kick_return', 'Yards/Kick Return', 'team', 'special', false, true, true, 58, 'per_game_avg', true, false, NULL),
	(753, 'NFL', 'punt_returns', 'Punt Returns', 'team', 'special', false, false, true, 59, 'cumulative_total', true, false, NULL),
	(754, 'NFL', 'punt_return_yards', 'Punt Return Yards', 'team', 'special', false, false, true, 60, 'cumulative_total', true, false, NULL),
	(755, 'NFL', 'punt_return_touchdowns', 'Punt Return TDs', 'team', 'special', false, false, true, 61, 'cumulative_total', true, false, NULL),
	(756, 'NFL', 'yards_per_punt_return', 'Yards/Punt Return', 'team', 'special', false, true, true, 62, 'per_game_avg', true, false, NULL),
	(757, 'NFL', 'qbr', 'Passer Rating (ESPN)', 'team', 'offense', false, false, true, 63, 'rate_pct', true, false, NULL),
	(758, 'NFL', 'qb_rating', 'Passer Rating (NFL)', 'team', 'offense', false, false, true, 64, 'rate_pct', true, false, NULL),
	(921, 'FOOTBALL', 'possession_pct', 'Possession %', 'team', 'possession', false, false, true, 108, 'rate_pct', true, false, NULL),
	(922, 'FOOTBALL', 'assists', 'Assists', 'team', 'attacking', false, false, true, 91, 'cumulative_total', true, false, NULL),
	(923, 'FOOTBALL', 'goal_attempts', 'Goal Attempts', 'team', 'shooting', false, false, true, 92, 'cumulative_total', true, false, NULL),
	(924, 'FOOTBALL', 'hit_woodwork', 'Hit Woodwork', 'team', 'shooting', false, false, true, 93, 'cumulative_total', true, false, NULL),
	(925, 'FOOTBALL', 'shots_insidebox', 'Shots Inside Box', 'team', 'shooting', false, false, true, 94, 'cumulative_total', true, false, NULL),
	(926, 'FOOTBALL', 'shots_outsidebox', 'Shots Outside Box', 'team', 'shooting', false, false, true, 95, 'cumulative_total', true, false, NULL),
	(927, 'FOOTBALL', 'successful_headers', 'Successful Headers', 'team', 'duels', false, false, true, 109, 'cumulative_total', true, false, NULL),
	(928, 'FOOTBALL', 'corners', 'Corners', 'team', 'attacking', false, false, true, 130, 'cumulative_total', true, false, NULL),
	(929, 'FOOTBALL', 'attacks', 'Attacks', 'team', 'attacking', false, false, true, 131, 'cumulative_total', true, false, NULL),
	(930, 'FOOTBALL', 'dangerous_attacks', 'Dangerous Attacks', 'team', 'attacking', false, false, true, 132, 'cumulative_total', true, false, NULL),
	(931, 'FOOTBALL', 'ball_safe', 'Ball Safe Events', 'team', 'possession', false, false, true, 133, 'cumulative_total', true, false, NULL),
	(932, 'FOOTBALL', 'goal_kicks', 'Goal Kicks', 'team', 'set_pieces', false, false, true, 140, 'cumulative_total', true, false, NULL),
	(933, 'FOOTBALL', 'free_kicks', 'Free Kicks', 'team', 'set_pieces', false, false, true, 141, 'cumulative_total', true, false, NULL),
	(934, 'FOOTBALL', 'throw_ins', 'Throw-ins', 'team', 'set_pieces', false, false, true, 142, 'cumulative_total', true, false, NULL),
	(935, 'FOOTBALL', 'penalties', 'Penalties', 'team', 'set_pieces', false, false, true, 143, 'cumulative_total', true, false, NULL),
	(936, 'FOOTBALL', 'injuries', 'Injuries', 'team', 'discipline', true, false, true, 150, 'cumulative_total', true, false, NULL),
	(937, 'FOOTBALL', 'substitutions', 'Substitutions', 'team', 'general', false, false, false, 151, 'cumulative_total', true, false, NULL),
	(1066, 'NFL', 'first_downs', 'First Downs', 'team', 'offense', false, false, true, 65, 'cumulative_total', true, false, NULL),
	(1067, 'NFL', 'first_downs_passing', 'First Downs (Passing)', 'team', 'offense', false, false, true, 66, 'cumulative_total', true, false, NULL),
	(1068, 'NFL', 'first_downs_rushing', 'First Downs (Rushing)', 'team', 'offense', false, false, true, 67, 'cumulative_total', true, false, NULL),
	(1069, 'NFL', 'first_downs_penalty', 'First Downs (Penalty)', 'team', 'offense', false, false, true, 68, 'cumulative_total', true, false, NULL),
	(1070, 'NFL', 'third_down_attempts', 'Third Down Attempts', 'team', 'offense', false, false, true, 70, 'cumulative_total', true, false, NULL),
	(1071, 'NFL', 'third_down_conversions', 'Third Down Conversions', 'team', 'offense', false, false, true, 71, 'cumulative_total', true, false, NULL),
	(1072, 'NFL', 'third_down_pct', 'Third Down %', 'team', 'offense', false, true, true, 72, 'rate_pct', true, false, NULL),
	(1073, 'NFL', 'fourth_down_attempts', 'Fourth Down Attempts', 'team', 'offense', false, false, true, 73, 'cumulative_total', true, false, NULL),
	(1074, 'NFL', 'fourth_down_conversions', 'Fourth Down Conversions', 'team', 'offense', false, false, true, 74, 'cumulative_total', true, false, NULL),
	(1075, 'NFL', 'fourth_down_pct', 'Fourth Down %', 'team', 'offense', false, true, true, 75, 'rate_pct', true, false, NULL),
	(1076, 'NFL', 'red_zone_attempts', 'Red Zone Attempts', 'team', 'offense', false, false, true, 76, 'cumulative_total', true, false, NULL),
	(1077, 'NFL', 'red_zone_scores', 'Red Zone Scores', 'team', 'offense', false, false, true, 77, 'cumulative_total', true, false, NULL),
	(1078, 'NFL', 'red_zone_pct', 'Red Zone %', 'team', 'offense', false, true, true, 78, 'rate_pct', true, false, NULL),
	(1079, 'NFL', 'total_drives', 'Total Drives', 'team', 'offense', false, false, true, 79, 'cumulative_total', true, false, NULL),
	(1080, 'NFL', 'total_offensive_plays', 'Total Offensive Plays', 'team', 'offense', false, false, true, 80, 'cumulative_total', true, false, NULL),
	(1081, 'NFL', 'yards_per_play', 'Yards/Play', 'team', 'offense', false, true, true, 81, 'per_game_avg', true, false, NULL),
	(1082, 'NFL', 'net_passing_yards', 'Net Passing Yards', 'team', 'offense', false, false, true, 82, 'cumulative_total', true, false, NULL),
	(1083, 'NFL', 'sack_yards_lost', 'Sack Yards Lost', 'team', 'offense', true, false, true, 83, 'cumulative_total', true, false, NULL),
	(1084, 'NFL', 'possession_time_seconds', 'Possession Time (s)', 'team', 'general', false, false, true, 84, 'cumulative_total', true, false, NULL),
	(1085, 'NFL', 'avg_possession_seconds', 'Avg Possession/Game (s)', 'team', 'general', false, true, true, 85, 'per_game_avg', true, false, NULL),
	(1086, 'NFL', 'penalties', 'Penalties', 'team', 'discipline', true, false, true, 86, 'cumulative_total', true, false, NULL),
	(1087, 'NFL', 'penalty_yards', 'Penalty Yards', 'team', 'discipline', true, false, true, 87, 'cumulative_total', true, false, NULL),
	(1088, 'NFL', 'defensive_touchdowns', 'Defensive TDs', 'team', 'defense', false, false, true, 88, 'cumulative_total', true, false, NULL),
	(1288, 'FOOTBALL', 'clearances_per_game', 'Clearances Per Game', 'player', 'defensive', false, true, true, 310, NULL, false, false, NULL),
	(1289, 'FOOTBALL', 'blocks_per_game', 'Blocks Per Game', 'player', 'defensive', false, true, true, 311, NULL, false, false, NULL),
	(1290, 'FOOTBALL', 'duels_total_per_game', 'Duels Per Game', 'player', 'duels', false, true, true, 312, NULL, false, false, NULL),
	(1291, 'FOOTBALL', 'duels_won_per_game', 'Duels Won Per Game', 'player', 'duels', false, true, true, 313, NULL, false, false, NULL),
	(1292, 'FOOTBALL', 'dribbles_attempts_per_game', 'Dribble Attempts/Game', 'player', 'dribbling', false, true, true, 314, NULL, false, false, NULL),
	(1293, 'FOOTBALL', 'dribbles_success_per_game', 'Successful Dribbles/Game', 'player', 'dribbling', false, true, true, 315, NULL, false, false, NULL),
	(1294, 'FOOTBALL', 'saves_per_game', 'Saves Per Game', 'player', 'goalkeeper', false, true, true, 316, NULL, false, false, NULL),
	(1295, 'FOOTBALL', 'goals_conceded_per_game', 'Goals Conceded/Game', 'player', 'goalkeeper', true, true, true, 317, NULL, false, false, NULL),
	(1296, 'FOOTBALL', 'saves_insidebox_per_game', 'Saves Inside Box/Game', 'player', 'goalkeeper', false, true, true, 318, NULL, false, false, NULL),
	(1297, 'FOOTBALL', 'chances_created_per_game', 'Chances Created/Game', 'player', 'passing', false, true, true, 319, NULL, false, false, NULL),
	(1298, 'FOOTBALL', 'big_chances_created_per_game', 'Big Chances Created/Game', 'player', 'passing', false, true, true, 320, NULL, false, false, NULL),
	(1299, 'FOOTBALL', 'long_balls_per_game', 'Long Balls Per Game', 'player', 'passing', false, true, true, 321, NULL, false, false, NULL),
	(1300, 'FOOTBALL', 'long_balls_won_per_game', 'Long Balls Won/Game', 'player', 'passing', false, true, true, 322, NULL, false, false, NULL),
	(1301, 'FOOTBALL', 'through_balls_per_game', 'Through Balls Per Game', 'player', 'passing', false, true, true, 323, NULL, false, false, NULL),
	(1302, 'FOOTBALL', 'through_balls_won_per_game', 'Through Balls Won/Game', 'player', 'passing', false, true, true, 324, NULL, false, false, NULL);
INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(1303, 'FOOTBALL', 'passes_in_final_third_per_game', 'Final-Third Passes/Game', 'player', 'passing', false, true, true, 325, NULL, false, false, NULL),
	(1304, 'FOOTBALL', 'tackles_per_game', 'Tackles Per Game', 'player', 'defensive', false, true, true, 326, NULL, false, false, NULL),
	(1305, 'FOOTBALL', 'tackles_won_per_game', 'Tackles Won Per Game', 'player', 'defensive', false, true, true, 327, NULL, false, false, NULL),
	(1306, 'FOOTBALL', 'interceptions_per_game', 'Interceptions/Game', 'player', 'defensive', false, true, true, 328, NULL, false, false, NULL),
	(1307, 'FOOTBALL', 'dribbled_past_per_game', 'Dribbled Past Per Game', 'player', 'defensive', true, true, true, 329, NULL, false, false, NULL),
	(1308, 'FOOTBALL', 'dispossessed_per_game', 'Dispossessed Per Game', 'player', 'possession', true, true, true, 330, NULL, false, false, NULL),
	(36, 'NBA', 'fg3_pct', 'Three-Point %', 'team', 'shooting', false, false, true, 8, 'rate_pct', true, false, NULL),
	(37, 'NBA', 'ft_pct', 'Free Throw %', 'team', 'shooting', false, false, true, 9, 'rate_pct', true, false, NULL),
	(1309, 'FOOTBALL', 'possession_lost_per_game', 'Possession Lost/Game', 'player', 'possession', true, true, true, 331, NULL, false, false, NULL),
	(39, 'NFL', 'games_played', 'Games Played', 'player', 'general', false, false, false, 1, 'special', false, false, NULL),
	(1310, 'FOOTBALL', 'turnovers_per_game', 'Turnovers Per Game', 'player', 'possession', true, true, true, 332, NULL, false, false, NULL),
	(1311, 'FOOTBALL', 'ball_recovery_per_game', 'Ball Recoveries/Game', 'player', 'defensive', false, true, true, 333, NULL, false, false, NULL),
	(1312, 'FOOTBALL', 'aerials_per_game', 'Aerials Per Game', 'player', 'duels', false, true, true, 334, NULL, false, false, NULL),
	(1313, 'FOOTBALL', 'aeriels_won_per_game', 'Aerials Won Per Game', 'player', 'duels', false, true, true, 335, NULL, false, false, NULL),
	(1314, 'FOOTBALL', 'fouls_committed_per_game', 'Fouls Committed/Game', 'player', 'discipline', true, true, true, 336, NULL, false, false, NULL),
	(1315, 'FOOTBALL', 'fouls_drawn_per_game', 'Fouls Drawn Per Game', 'player', 'discipline', false, true, true, 337, NULL, false, false, NULL),
	(40, 'NFL', 'passing_completions', 'Completions', 'player', 'passing', false, false, true, 10, 'cumulative_total', false, true, NULL),
	(41, 'NFL', 'passing_attempts', 'Pass Attempts', 'player', 'passing', false, false, true, 11, 'cumulative_total', false, true, NULL),
	(42, 'NFL', 'passing_yards', 'Passing Yards', 'player', 'passing', false, false, true, 12, 'cumulative_total', false, true, NULL),
	(93, 'NFL', 'wins', 'Wins', 'team', 'standings', false, false, true, 1, 'special', false, false, NULL),
	(737, 'NFL', 'qb_hits', 'QB Hits', 'team', 'defense', false, false, true, 29, 'cumulative_total', true, false, NULL),
	(96, 'NFL', 'points_for', 'Points For', 'team', 'scoring', false, false, true, 4, 'special', false, false, NULL),
	(98, 'NFL', 'point_differential', 'Point Differential', 'team', 'scoring', false, false, true, 6, 'special', false, false, NULL),
	(99, 'NFL', 'win_pct', 'Win Percentage', 'team', 'standings', false, true, true, 7, 'special', false, false, NULL),
	(158, 'FOOTBALL', 'wins', 'Wins', 'team', 'standings', false, false, true, 2, 'special', false, false, NULL),
	(159, 'FOOTBALL', 'draws', 'Draws', 'team', 'standings', false, false, true, 3, 'special', false, false, NULL),
	(163, 'FOOTBALL', 'goal_difference', 'Goal Difference', 'team', 'scoring', false, false, true, 7, 'special', false, false, NULL),
	(164, 'FOOTBALL', 'points', 'Points', 'team', 'standings', false, false, true, 8, 'special', false, false, NULL),
	(165, 'FOOTBALL', 'overall_points', 'Overall Points', 'team', 'standings', false, false, true, 9, 'special', false, false, NULL),
	(614, 'NBA', 'point_differential', 'Point Differential', 'team', 'advanced', false, true, true, 12, 'special', false, false, NULL),
	(38, 'NBA', 'win_pct', 'Win Percentage', 'team', 'standings', false, true, true, 10, 'special', false, false, NULL),
	(162, 'FOOTBALL', 'goals_against', 'Goals Against', 'team', 'scoring', true, false, true, 6, 'cumulative_total', true, false, NULL),
	(94, 'NFL', 'losses', 'Losses', 'team', 'standings', true, false, true, 2, 'special', false, false, NULL),
	(97, 'NFL', 'points_against', 'Points Against', 'team', 'scoring', true, false, true, 5, 'special', false, false, NULL),
	(160, 'FOOTBALL', 'losses', 'Losses', 'team', 'standings', true, false, true, 4, 'special', false, false, NULL),
	(124, 'FOOTBALL', 'goals_per_90', 'Goals Per 90', 'player', 'scoring', false, true, false, 13, 'per_game_avg', true, false, NULL),
	(125, 'FOOTBALL', 'assists_per_90', 'Assists Per 90', 'player', 'scoring', false, true, false, 14, 'per_game_avg', true, false, NULL),
	(128, 'FOOTBALL', 'shots_per_90', 'Shots Per 90', 'player', 'shooting', false, true, false, 22, 'per_game_avg', true, false, NULL),
	(135, 'FOOTBALL', 'key_passes_per_90', 'Key Passes Per 90', 'player', 'passing', false, true, false, 35, 'per_game_avg', true, false, NULL),
	(141, 'FOOTBALL', 'tackles_per_90', 'Tackles Per 90', 'player', 'defensive', false, true, false, 44, 'per_game_avg', true, false, NULL),
	(142, 'FOOTBALL', 'interceptions_per_90', 'Interceptions/90', 'player', 'defensive', false, true, false, 45, 'per_game_avg', true, false, NULL),
	(155, 'FOOTBALL', 'goals_conceded_per_90', 'Goals Conceded/90', 'player', 'goalkeeper', true, true, false, 72, 'per_game_avg', true, false, NULL),
	(601, 'NBA', 'tov_per_36', 'Turnovers Per 36 Min', 'player', 'advanced', true, true, false, 39, 'per_game_avg', true, false, NULL),
	(602, 'NBA', 'pf_per_36', 'Fouls Per 36 Min', 'player', 'advanced', true, true, false, 40, 'per_game_avg', true, false, NULL),
	(729, 'NFL', 'points_per_game', 'Points/Game', 'team', 'scoring', false, true, false, 8, 'per_game_avg', true, false, NULL),
	(730, 'NFL', 'points_allowed_per_game', 'Points Allowed/Game', 'team', 'scoring', true, true, false, 9, 'per_game_avg', true, false, NULL),
	(731, 'NFL', 'yards_per_game', 'Total Yards/Game', 'team', 'offense', false, true, false, 19, 'per_game_avg', true, false, NULL),
	(1128, 'NBA', 'oreb_per_36', 'Off Rebounds Per 36 Min', 'player', 'advanced', false, true, false, 41, 'per_game_avg', true, false, NULL),
	(1129, 'NBA', 'dreb_per_36', 'Def Rebounds Per 36 Min', 'player', 'advanced', false, true, false, 42, 'per_game_avg', true, false, NULL),
	(1130, 'NBA', 'fgm_per_36', 'FG Made Per 36 Min', 'player', 'advanced', false, true, false, 43, 'per_game_avg', true, false, NULL),
	(1131, 'NBA', 'fga_per_36', 'FG Att Per 36 Min', 'player', 'advanced', false, true, false, 44, 'per_game_avg', true, false, NULL),
	(1132, 'NBA', 'fg3m_per_36', '3PT Made Per 36 Min', 'player', 'advanced', false, true, false, 45, 'per_game_avg', true, false, NULL),
	(1133, 'NBA', 'fg3a_per_36', '3PT Att Per 36 Min', 'player', 'advanced', false, true, false, 46, 'per_game_avg', true, false, NULL),
	(1134, 'NBA', 'ftm_per_36', 'FT Made Per 36 Min', 'player', 'advanced', false, true, false, 47, 'per_game_avg', true, false, NULL),
	(1135, 'NBA', 'fta_per_36', 'FT Att Per 36 Min', 'player', 'advanced', false, true, false, 48, 'per_game_avg', true, false, NULL),
	(1136, 'FOOTBALL', 'expected_goals_per_90', 'xG Per 90', 'player', 'scoring', false, true, false, 200, 'per_game_avg', true, false, NULL),
	(1137, 'FOOTBALL', 'shots_on_target_per_90', 'Shots on Target/90', 'player', 'shooting', false, true, false, 201, 'per_game_avg', true, false, NULL),
	(1138, 'FOOTBALL', 'passes_total_per_90', 'Passes Per 90', 'player', 'passing', false, true, false, 202, 'per_game_avg', true, false, NULL),
	(1139, 'FOOTBALL', 'passes_accurate_per_90', 'Accurate Passes/90', 'player', 'passing', false, true, false, 203, 'per_game_avg', true, false, NULL),
	(1140, 'FOOTBALL', 'crosses_total_per_90', 'Crosses Per 90', 'player', 'passing', false, true, false, 204, 'per_game_avg', true, false, NULL),
	(1141, 'FOOTBALL', 'crosses_accurate_per_90', 'Accurate Crosses/90', 'player', 'passing', false, true, false, 205, 'per_game_avg', true, false, NULL),
	(1142, 'FOOTBALL', 'clearances_per_90', 'Clearances Per 90', 'player', 'defensive', false, true, false, 206, 'per_game_avg', true, false, NULL),
	(1143, 'FOOTBALL', 'blocks_per_90', 'Blocks Per 90', 'player', 'defensive', false, true, false, 207, 'per_game_avg', true, false, NULL),
	(1144, 'FOOTBALL', 'duels_total_per_90', 'Duels Per 90', 'player', 'duels', false, true, false, 208, 'per_game_avg', true, false, NULL),
	(1145, 'FOOTBALL', 'duels_won_per_90', 'Duels Won Per 90', 'player', 'duels', false, true, false, 209, 'per_game_avg', true, false, NULL),
	(1146, 'FOOTBALL', 'dribbles_attempts_per_90', 'Dribble Attempts/90', 'player', 'dribbling', false, true, false, 210, 'per_game_avg', true, false, NULL),
	(1147, 'FOOTBALL', 'dribbles_success_per_90', 'Successful Dribbles/90', 'player', 'dribbling', false, true, false, 211, 'per_game_avg', true, false, NULL),
	(1149, 'FOOTBALL', 'saves_insidebox_per_90', 'Saves Inside Box/90', 'player', 'goalkeeper', false, true, false, 213, 'per_game_avg', true, false, NULL),
	(1150, 'FOOTBALL', 'chances_created_per_90', 'Chances Created Per 90', 'player', 'passing', false, true, false, 214, 'per_game_avg', true, false, NULL),
	(1148, 'FOOTBALL', 'saves_per_90', 'Saves Per 90', 'player', 'goalkeeper', false, true, false, 212, 'per_game_avg', true, false, NULL),
	(1158, 'FOOTBALL', 'dribbled_past_per_90', 'Dribbled Past Per 90', 'player', 'defensive', true, true, false, 222, 'per_game_avg', true, false, NULL),
	(1159, 'FOOTBALL', 'dispossessed_per_90', 'Dispossessed Per 90', 'player', 'possession', true, true, false, 223, 'per_game_avg', true, false, NULL),
	(1160, 'FOOTBALL', 'possession_lost_per_90', 'Possession Lost Per 90', 'player', 'possession', true, true, false, 224, 'per_game_avg', true, false, NULL),
	(1161, 'FOOTBALL', 'turnovers_per_90', 'Turnovers Per 90', 'player', 'possession', true, true, false, 225, 'per_game_avg', true, false, NULL),
	(1162, 'FOOTBALL', 'ball_recovery_per_90', 'Ball Recoveries Per 90', 'player', 'defensive', false, true, false, 226, 'per_game_avg', true, false, NULL),
	(1163, 'FOOTBALL', 'aerials_per_90', 'Aerials Per 90', 'player', 'duels', false, true, false, 227, 'per_game_avg', true, false, NULL),
	(1164, 'FOOTBALL', 'aeriels_won_per_90', 'Aerials Won Per 90', 'player', 'duels', false, true, false, 228, 'per_game_avg', true, false, NULL),
	(1165, 'FOOTBALL', 'fouls_committed_per_90', 'Fouls Committed Per 90', 'player', 'discipline', true, true, false, 229, 'per_game_avg', true, false, NULL),
	(1166, 'FOOTBALL', 'fouls_drawn_per_90', 'Fouls Drawn Per 90', 'player', 'discipline', false, true, false, 230, 'per_game_avg', true, false, NULL),
	(1167, 'NFL', 'passing_completions_per_game', 'Completions/Game', 'player', 'passing', false, true, false, 100, 'per_game_avg', true, false, NULL),
	(1168, 'NFL', 'passing_attempts_per_game', 'Pass Attempts/Game', 'player', 'passing', false, true, false, 101, 'per_game_avg', true, false, NULL),
	(1169, 'NFL', 'passing_touchdowns_per_game', 'Passing TDs/Game', 'player', 'passing', false, true, false, 102, 'per_game_avg', true, false, NULL),
	(1170, 'NFL', 'passing_interceptions_per_game', 'INTs Thrown/Game', 'player', 'passing', true, true, false, 103, 'per_game_avg', true, false, NULL),
	(1171, 'NFL', 'sacks_taken_per_game', 'Sacks Taken/Game', 'player', 'passing', true, true, false, 104, 'per_game_avg', true, false, NULL),
	(1172, 'NFL', 'sack_yards_lost_per_game', 'Sack Yards Lost/Game', 'player', 'passing', true, true, false, 105, 'per_game_avg', true, false, NULL),
	(1173, 'NFL', 'rushing_attempts_per_game', 'Rush Attempts/Game', 'player', 'rushing', false, true, false, 110, 'per_game_avg', true, false, NULL),
	(1174, 'NFL', 'rushing_touchdowns_per_game', 'Rush TDs/Game', 'player', 'rushing', false, true, false, 111, 'per_game_avg', true, false, NULL),
	(1175, 'NFL', 'rushing_first_downs_per_game', 'Rush First Downs/Game', 'player', 'rushing', false, true, false, 112, 'per_game_avg', true, false, NULL),
	(1176, 'NFL', 'receptions_per_game', 'Receptions/Game', 'player', 'receiving', false, true, false, 120, 'per_game_avg', true, false, NULL),
	(1177, 'NFL', 'receiving_targets_per_game', 'Targets/Game', 'player', 'receiving', false, true, false, 121, 'per_game_avg', true, false, NULL),
	(1178, 'NFL', 'receiving_touchdowns_per_game', 'Receiving TDs/Game', 'player', 'receiving', false, true, false, 122, 'per_game_avg', true, false, NULL),
	(1179, 'NFL', 'receiving_first_downs_per_game', 'Rec First Downs/Game', 'player', 'receiving', false, true, false, 123, 'per_game_avg', true, false, NULL),
	(1180, 'NFL', 'total_tackles_per_game', 'Total Tackles/Game', 'player', 'defensive', false, true, false, 130, 'per_game_avg', true, false, NULL),
	(1181, 'NFL', 'solo_tackles_per_game', 'Solo Tackles/Game', 'player', 'defensive', false, true, false, 131, 'per_game_avg', true, false, NULL),
	(1182, 'NFL', 'assist_tackles_per_game', 'Assist Tackles/Game', 'player', 'defensive', false, true, false, 132, 'per_game_avg', true, false, NULL),
	(1183, 'NFL', 'defensive_sacks_per_game', 'Sacks/Game', 'player', 'defensive', false, true, false, 133, 'per_game_avg', true, false, NULL),
	(1184, 'NFL', 'defensive_sack_yards_per_game', 'Sack Yards/Game', 'player', 'defensive', false, true, false, 134, 'per_game_avg', true, false, NULL),
	(1185, 'NFL', 'defensive_interceptions_per_game', 'INTs/Game', 'player', 'defensive', false, true, false, 135, 'per_game_avg', true, false, NULL),
	(1186, 'NFL', 'interception_touchdowns_per_game', 'INT Return TDs/Game', 'player', 'defensive', false, true, false, 136, 'per_game_avg', true, false, NULL),
	(1187, 'NFL', 'interception_yards_per_game', 'INT Return Yards/Game', 'player', 'defensive', false, true, false, 137, 'per_game_avg', true, false, NULL);
INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(1188, 'NFL', 'fumbles_forced_per_game', 'Forced Fumbles/Game', 'player', 'defensive', false, true, false, 138, 'per_game_avg', true, false, NULL),
	(1189, 'NFL', 'fumbles_recovered_per_game', 'Fumbles Recovered/Game', 'player', 'defensive', false, true, false, 139, 'per_game_avg', true, false, NULL),
	(1190, 'NFL', 'fumbles_touchdowns_per_game', 'Fumble Return TDs/Game', 'player', 'defensive', false, true, false, 140, 'per_game_avg', true, false, NULL),
	(1191, 'NFL', 'tackles_for_loss_per_game', 'TFL/Game', 'player', 'defensive', false, true, false, 141, 'per_game_avg', true, false, NULL),
	(1192, 'NFL', 'passes_defended_per_game', 'Passes Defended/Game', 'player', 'defensive', false, true, false, 142, 'per_game_avg', true, false, NULL),
	(1193, 'NFL', 'qb_hits_per_game', 'QB Hits/Game', 'player', 'defensive', false, true, false, 143, 'per_game_avg', true, false, NULL),
	(1194, 'NFL', 'fumbles_per_game', 'Fumbles/Game', 'player', 'general', true, true, false, 150, 'per_game_avg', true, false, NULL),
	(1195, 'NFL', 'fumbles_lost_per_game', 'Fumbles Lost/Game', 'player', 'general', true, true, false, 151, 'per_game_avg', true, false, NULL),
	(1196, 'NFL', 'field_goal_attempts_per_game', 'FG Attempts/Game', 'player', 'kicking', false, true, false, 160, 'per_game_avg', true, false, NULL),
	(1197, 'NFL', 'field_goals_made_per_game', 'FG Made/Game', 'player', 'kicking', false, true, false, 161, 'per_game_avg', true, false, NULL),
	(1198, 'NFL', 'extra_points_made_per_game', 'XP Made/Game', 'player', 'kicking', false, true, false, 162, 'per_game_avg', true, false, NULL),
	(1199, 'NFL', 'total_points_per_game', 'Points/Game', 'player', 'kicking', false, true, false, 163, 'per_game_avg', true, false, NULL),
	(1200, 'NFL', 'touchbacks_per_game', 'Touchbacks/Game', 'player', 'kicking', false, true, false, 164, 'per_game_avg', true, false, NULL),
	(1201, 'NFL', 'punts_per_game', 'Punts/Game', 'player', 'special', false, true, false, 170, 'per_game_avg', true, false, NULL),
	(1202, 'NFL', 'punt_yards_per_game', 'Punt Yards/Game', 'player', 'special', false, true, false, 171, 'per_game_avg', true, false, NULL),
	(1203, 'NFL', 'punts_inside_20_per_game', 'Punts Inside 20/Game', 'player', 'special', false, true, false, 172, 'per_game_avg', true, false, NULL),
	(1204, 'NFL', 'kick_returns_per_game', 'Kick Returns/Game', 'player', 'special', false, true, false, 173, 'per_game_avg', true, false, NULL),
	(1205, 'NFL', 'kick_return_yards_per_game', 'Kick Return Yards/Game', 'player', 'special', false, true, false, 174, 'per_game_avg', true, false, NULL),
	(1206, 'NFL', 'kick_return_touchdowns_per_game', 'Kick Return TDs/Game', 'player', 'special', false, true, false, 175, 'per_game_avg', true, false, NULL),
	(1207, 'NFL', 'punt_returner_returns_per_game', 'Punt Returns/Game', 'player', 'special', false, true, false, 176, 'per_game_avg', true, false, NULL),
	(1208, 'NFL', 'punt_returner_return_yards_per_game', 'Punt Return Yards/Game', 'player', 'special', false, true, false, 177, 'per_game_avg', true, false, NULL),
	(1209, 'NFL', 'punt_return_touchdowns_per_game', 'Punt Return TDs/Game', 'player', 'special', false, true, false, 178, 'per_game_avg', true, false, NULL),
	(1322, 'NBA', 'fantasy_points_per_36', 'Fantasy Points Per 36', 'player', 'fantasy', false, true, true, 91, NULL, false, false, NULL),
	(1323, 'NBA', 'fantasy_points_per_season', 'Total Fantasy Points', 'player', 'fantasy', false, true, true, 92, NULL, false, false, NULL),
	(1325, 'NFL', 'fantasy_points_per_game', 'Fantasy Points Per Game', 'player', 'fantasy', false, true, true, 91, NULL, false, false, NULL),
	(1321, 'NBA', 'fantasy_points', 'Fantasy Points', 'player', 'fantasy', false, true, true, 90, NULL, false, true, NULL),
	(1324, 'NFL', 'fantasy_points', 'Fantasy Points', 'player', 'fantasy', false, true, true, 90, NULL, false, true, NULL),
	(153, 'FOOTBALL', 'saves', 'Saves', 'player', 'goalkeeper', false, false, true, 70, 'cumulative_total', false, true, NULL),
	(121, 'FOOTBALL', 'goals', 'Goals', 'player', 'scoring', false, false, true, 10, 'cumulative_total', false, true, NULL),
	(122, 'FOOTBALL', 'assists', 'Assists', 'player', 'scoring', false, false, true, 11, 'cumulative_total', false, true, NULL),
	(123, 'FOOTBALL', 'expected_goals', 'Expected Goals (xG)', 'player', 'scoring', false, false, true, 12, 'cumulative_total', false, true, NULL),
	(127, 'FOOTBALL', 'shots_on_target', 'Shots on Target', 'player', 'shooting', false, false, true, 21, 'cumulative_total', false, true, NULL),
	(130, 'FOOTBALL', 'passes_total', 'Total Passes', 'player', 'passing', false, false, true, 30, 'cumulative_total', false, true, NULL),
	(131, 'FOOTBALL', 'passes_accurate', 'Accurate Passes', 'player', 'passing', false, false, true, 31, 'cumulative_total', false, true, NULL),
	(132, 'FOOTBALL', 'key_passes', 'Key Passes', 'player', 'passing', false, false, true, 32, 'cumulative_total', false, true, NULL),
	(133, 'FOOTBALL', 'crosses_total', 'Total Crosses', 'player', 'passing', false, false, true, 33, 'cumulative_total', false, true, NULL),
	(134, 'FOOTBALL', 'crosses_accurate', 'Accurate Crosses', 'player', 'passing', false, false, true, 34, 'cumulative_total', false, true, NULL),
	(137, 'FOOTBALL', 'tackles', 'Tackles', 'player', 'defensive', false, false, true, 40, 'cumulative_total', false, true, NULL),
	(138, 'FOOTBALL', 'interceptions', 'Interceptions', 'player', 'defensive', false, false, true, 41, 'cumulative_total', false, true, NULL),
	(139, 'FOOTBALL', 'clearances', 'Clearances', 'player', 'defensive', false, false, true, 42, 'cumulative_total', false, true, NULL),
	(140, 'FOOTBALL', 'blocks', 'Blocks', 'player', 'defensive', false, false, true, 43, 'cumulative_total', false, true, NULL),
	(143, 'FOOTBALL', 'duels_total', 'Total Duels', 'player', 'duels', false, false, true, 50, 'cumulative_total', false, true, NULL),
	(144, 'FOOTBALL', 'duels_won', 'Duels Won', 'player', 'duels', false, false, true, 51, 'cumulative_total', false, true, NULL),
	(146, 'FOOTBALL', 'dribbles_attempts', 'Dribble Attempts', 'player', 'dribbling', false, false, true, 55, 'cumulative_total', false, true, NULL),
	(147, 'FOOTBALL', 'dribbles_success', 'Successful Dribbles', 'player', 'dribbling', false, false, true, 56, 'cumulative_total', false, true, NULL),
	(151, 'FOOTBALL', 'fouls_committed', 'Fouls Committed', 'player', 'discipline', true, false, true, 62, 'cumulative_total', false, true, NULL),
	(152, 'FOOTBALL', 'fouls_drawn', 'Fouls Drawn', 'player', 'discipline', false, false, true, 63, 'cumulative_total', false, true, NULL),
	(154, 'FOOTBALL', 'goals_conceded', 'Goals Conceded', 'player', 'goalkeeper', true, false, true, 71, 'cumulative_total', false, true, NULL),
	(286, 'FOOTBALL', 'saves_insidebox', 'Saves Inside Box', 'player', 'goalkeeper', false, false, true, 74, 'cumulative_total', false, true, NULL),
	(300, 'FOOTBALL', 'chances_created', 'Chances Created', 'player', 'passing', false, false, true, 37, 'cumulative_total', false, true, NULL),
	(301, 'FOOTBALL', 'big_chances_created', 'Big Chances Created', 'player', 'passing', false, false, true, 38, 'cumulative_total', false, true, NULL),
	(303, 'FOOTBALL', 'long_balls', 'Long Balls', 'player', 'passing', false, false, true, 39, 'cumulative_total', false, true, NULL),
	(304, 'FOOTBALL', 'long_balls_won', 'Long Balls Won', 'player', 'passing', false, false, true, 46, 'cumulative_total', false, true, NULL),
	(306, 'FOOTBALL', 'through_balls', 'Through Balls', 'player', 'passing', false, false, true, 48, 'cumulative_total', false, true, NULL),
	(307, 'FOOTBALL', 'through_balls_won', 'Through Balls Won', 'player', 'passing', false, false, true, 49, 'cumulative_total', false, true, NULL),
	(309, 'FOOTBALL', 'passes_in_final_third', 'Passes in Final Third', 'player', 'passing', false, false, true, 67, 'cumulative_total', false, true, NULL),
	(311, 'FOOTBALL', 'tackles_won', 'Tackles Won', 'player', 'defensive', false, false, true, 78, 'cumulative_total', false, true, NULL),
	(318, 'FOOTBALL', 'aerials', 'Aerial Duels', 'player', 'duels', false, false, true, 54, 'cumulative_total', false, true, NULL),
	(319, 'FOOTBALL', 'aeriels_won', 'Aerials Won', 'player', 'duels', false, false, true, 58, 'cumulative_total', false, true, NULL),
	(322, 'FOOTBALL', 'dribbled_past', 'Dribbled Past', 'player', 'defensive', true, false, true, 84, 'cumulative_total', false, true, NULL),
	(323, 'FOOTBALL', 'dispossessed', 'Dispossessed', 'player', 'possession', true, false, true, 85, 'cumulative_total', false, true, NULL),
	(324, 'FOOTBALL', 'possession_lost', 'Possession Lost', 'player', 'possession', true, false, true, 86, 'cumulative_total', false, true, NULL),
	(325, 'FOOTBALL', 'turnovers', 'Turnovers', 'player', 'possession', true, false, true, 87, 'cumulative_total', false, true, NULL),
	(327, 'FOOTBALL', 'ball_recovery', 'Ball Recoveries', 'player', 'defensive', false, false, true, 89, 'cumulative_total', false, true, NULL),
	(126, 'FOOTBALL', 'shots_total', 'Total Shots', 'player', 'shooting', false, false, true, 20, 'cumulative_total', false, true, 'shots'),
	(43, 'NFL', 'passing_touchdowns', 'Passing TDs', 'player', 'passing', false, false, true, 13, 'cumulative_total', false, true, NULL),
	(44, 'NFL', 'passing_interceptions', 'Interceptions Thrown', 'player', 'passing', true, false, true, 14, 'cumulative_total', false, true, NULL),
	(49, 'NFL', 'rushing_attempts', 'Rush Attempts', 'player', 'rushing', false, false, true, 20, 'cumulative_total', false, true, NULL),
	(50, 'NFL', 'rushing_yards', 'Rushing Yards', 'player', 'rushing', false, false, true, 21, 'cumulative_total', false, true, NULL),
	(51, 'NFL', 'rushing_touchdowns', 'Rushing TDs', 'player', 'rushing', false, false, true, 22, 'cumulative_total', false, true, NULL),
	(54, 'NFL', 'rushing_first_downs', 'Rushing First Downs', 'player', 'rushing', false, false, true, 25, 'cumulative_total', false, true, NULL),
	(55, 'NFL', 'receptions', 'Receptions', 'player', 'receiving', false, false, true, 30, 'cumulative_total', false, true, NULL),
	(56, 'NFL', 'receiving_yards', 'Receiving Yards', 'player', 'receiving', false, false, true, 31, 'cumulative_total', false, true, NULL),
	(57, 'NFL', 'receiving_touchdowns', 'Receiving TDs', 'player', 'receiving', false, false, true, 32, 'cumulative_total', false, true, NULL),
	(58, 'NFL', 'receiving_targets', 'Targets', 'player', 'receiving', false, false, true, 33, 'cumulative_total', false, true, NULL),
	(61, 'NFL', 'receiving_first_downs', 'Receiving First Downs', 'player', 'receiving', false, false, true, 36, 'cumulative_total', false, true, NULL),
	(63, 'NFL', 'total_tackles', 'Total Tackles', 'player', 'defensive', false, false, true, 40, 'cumulative_total', false, true, NULL),
	(64, 'NFL', 'solo_tackles', 'Solo Tackles', 'player', 'defensive', false, false, true, 41, 'cumulative_total', false, true, NULL),
	(65, 'NFL', 'assist_tackles', 'Assisted Tackles', 'player', 'defensive', false, false, true, 42, 'cumulative_total', false, true, NULL),
	(66, 'NFL', 'defensive_sacks', 'Sacks', 'player', 'defensive', false, false, true, 43, 'cumulative_total', false, true, NULL),
	(67, 'NFL', 'defensive_sack_yards', 'Sack Yards', 'player', 'defensive', false, false, true, 44, 'cumulative_total', false, true, NULL),
	(68, 'NFL', 'defensive_interceptions', 'Interceptions', 'player', 'defensive', false, false, true, 45, 'cumulative_total', false, true, NULL),
	(69, 'NFL', 'interception_touchdowns', 'INT Return TDs', 'player', 'defensive', false, false, true, 46, 'cumulative_total', false, true, NULL),
	(70, 'NFL', 'fumbles_forced', 'Forced Fumbles', 'player', 'defensive', false, false, true, 47, 'cumulative_total', false, true, NULL),
	(71, 'NFL', 'fumbles_recovered', 'Fumbles Recovered', 'player', 'defensive', false, false, true, 48, 'cumulative_total', false, true, NULL),
	(72, 'NFL', 'field_goal_attempts', 'FG Attempts', 'player', 'kicking', false, false, true, 50, 'cumulative_total', false, true, NULL),
	(73, 'NFL', 'field_goals_made', 'FG Made', 'player', 'kicking', false, false, true, 51, 'cumulative_total', false, true, NULL),
	(76, 'NFL', 'punt_yards', 'Punt Yards', 'player', 'special', false, false, true, 61, 'cumulative_total', false, true, NULL),
	(77, 'NFL', 'kick_returns', 'Kick Returns', 'player', 'special', false, false, true, 62, 'cumulative_total', false, true, NULL),
	(78, 'NFL', 'kick_return_yards', 'Kick Return Yards', 'player', 'special', false, false, true, 63, 'cumulative_total', false, true, NULL),
	(79, 'NFL', 'kick_return_touchdowns', 'Kick Return TDs', 'player', 'special', false, false, true, 64, 'cumulative_total', false, true, NULL),
	(80, 'NFL', 'punt_returner_returns', 'Punt Returns', 'player', 'special', false, false, true, 65, 'cumulative_total', false, true, NULL),
	(81, 'NFL', 'punt_returner_return_yards', 'Punt Return Yards', 'player', 'special', false, false, true, 66, 'cumulative_total', false, true, NULL),
	(748, 'NFL', 'touchbacks', 'Touchbacks', 'team', 'special', false, false, true, 54, 'cumulative_total', true, false, NULL),
	(82, 'NFL', 'punt_return_touchdowns', 'Punt Return TDs', 'player', 'special', false, false, true, 67, 'cumulative_total', false, true, NULL),
	(83, 'NFL', 'tackles_for_loss', 'Tackles for Loss', 'player', 'defensive', false, false, true, 49, 'cumulative_total', false, true, NULL),
	(84, 'NFL', 'passes_defended', 'Passes Defended', 'player', 'defensive', false, false, true, 50, 'cumulative_total', false, true, NULL),
	(85, 'NFL', 'qb_hits', 'QB Hits', 'player', 'defensive', false, false, true, 51, 'cumulative_total', false, true, NULL),
	(86, 'NFL', 'fumbles_touchdowns', 'Fumble Return TDs', 'player', 'defensive', false, false, true, 52, 'cumulative_total', false, true, NULL),
	(87, 'NFL', 'fumbles', 'Fumbles', 'player', 'general', true, false, true, 6, 'cumulative_total', false, true, NULL);
INSERT INTO public.stat_definitions (id, sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order, unit, comparable, rate_sibling, rate_base) VALUES
	(88, 'NFL', 'fumbles_lost', 'Fumbles Lost', 'player', 'general', true, false, true, 7, 'cumulative_total', false, true, NULL),
	(89, 'NFL', 'extra_points_made', 'Extra Points Made', 'player', 'kicking', false, false, true, 53, 'cumulative_total', false, true, NULL),
	(90, 'NFL', 'total_points', 'Total Points', 'player', 'kicking', false, false, true, 54, 'cumulative_total', false, true, NULL),
	(91, 'NFL', 'touchbacks', 'Touchbacks', 'player', 'kicking', false, false, true, 55, 'cumulative_total', false, true, NULL),
	(92, 'NFL', 'punts_inside_20', 'Punts Inside 20', 'player', 'special', false, false, true, 68, 'cumulative_total', false, true, NULL),
	(687, 'NFL', 'sacks_taken', 'Sacks Taken', 'player', 'passing', true, false, true, 27, 'cumulative_total', false, true, NULL),
	(688, 'NFL', 'sack_yards_lost', 'Sack Yards Lost', 'player', 'passing', true, false, true, 28, 'cumulative_total', false, true, NULL),
	(699, 'NFL', 'interception_yards', 'INT Return Yards', 'player', 'defensive', false, false, true, 56, 'cumulative_total', false, true, NULL),
	(749, 'NFL', 'kick_returns', 'Kick Returns', 'team', 'special', false, false, true, 55, 'cumulative_total', true, false, NULL),
	(1, 'NBA', 'games_played', 'Games Played', 'player', 'general', false, false, false, 1, 'special', false, false, NULL),
	(2, 'NBA', 'minutes', 'Minutes Per Game', 'player', 'general', false, false, false, 2, 'per_game_avg', true, false, NULL),
	(12, 'NBA', 'plus_minus', 'Plus/Minus', 'player', 'advanced', false, false, true, 12, 'per_game_avg', true, false, NULL),
	(13, 'NBA', 'fg_pct', 'Field Goal %', 'player', 'shooting', false, false, true, 20, 'rate_pct', true, false, NULL),
	(29, 'NBA', 'wins', 'Wins', 'team', 'standings', false, false, true, 1, 'special', false, false, NULL),
	(3, 'NBA', 'pts', 'Points Per Game', 'player', 'scoring', false, false, true, 3, 'per_game_avg', true, true, NULL),
	(4, 'NBA', 'reb', 'Rebounds Per Game', 'player', 'rebounding', false, false, true, 4, 'per_game_avg', true, true, NULL),
	(5, 'NBA', 'ast', 'Assists Per Game', 'player', 'passing', false, false, true, 5, 'per_game_avg', true, true, NULL),
	(6, 'NBA', 'stl', 'Steals Per Game', 'player', 'defensive', false, false, true, 6, 'per_game_avg', true, true, NULL),
	(7, 'NBA', 'blk', 'Blocks Per Game', 'player', 'defensive', false, false, true, 7, 'per_game_avg', true, true, NULL),
	(8, 'NBA', 'oreb', 'Off Rebounds/Game', 'player', 'rebounding', false, false, true, 8, 'per_game_avg', true, true, NULL),
	(9, 'NBA', 'dreb', 'Def Rebounds/Game', 'player', 'rebounding', false, false, true, 9, 'per_game_avg', true, true, NULL),
	(11, 'NBA', 'pf', 'Fouls Per Game', 'player', 'general', true, false, true, 11, 'per_game_avg', true, true, NULL),
	(10, 'NBA', 'turnover', 'Turnovers Per Game', 'player', 'general', true, false, true, 10, 'per_game_avg', true, true, 'tov'),
	(14, 'NBA', 'fg3_pct', 'Three-Point %', 'player', 'shooting', false, false, true, 21, 'rate_pct', true, false, NULL),
	(15, 'NBA', 'ft_pct', 'Free Throw %', 'player', 'shooting', false, false, true, 22, 'rate_pct', true, false, NULL),
	(27, 'NBA', 'true_shooting_pct', 'True Shooting %', 'player', 'advanced', false, true, true, 35, 'rate_pct', true, false, NULL),
	(28, 'NBA', 'efficiency', 'Efficiency Rating', 'player', 'advanced', false, true, true, 36, 'rate_pct', true, false, NULL),
	(31, 'NBA', 'games_played', 'Games Played', 'team', 'general', false, false, false, 3, 'special', false, false, NULL),
	(32, 'NBA', 'pts', 'Points Per Game', 'team', 'scoring', false, false, true, 4, 'per_game_avg', true, false, NULL),
	(33, 'NBA', 'reb', 'Rebounds Per Game', 'team', 'rebounding', false, false, true, 5, 'per_game_avg', true, false, NULL),
	(34, 'NBA', 'ast', 'Assists Per Game', 'team', 'passing', false, false, true, 6, 'per_game_avg', true, false, NULL),
	(35, 'NBA', 'fg_pct', 'Field Goal %', 'team', 'shooting', false, false, true, 7, 'rate_pct', true, false, NULL),
	(47, 'NFL', 'qbr', 'Passer Rating', 'player', 'passing', false, false, true, 17, 'rate_pct', true, false, NULL),
	(16, 'NBA', 'fgm', 'Field Goals Made', 'player', 'shooting', false, false, true, 23, 'per_game_avg', true, true, NULL),
	(17, 'NBA', 'fga', 'Field Goals Attempted', 'player', 'shooting', false, false, true, 24, 'per_game_avg', true, true, NULL),
	(18, 'NBA', 'fg3m', 'Three-Pointers Made', 'player', 'shooting', false, false, true, 25, 'per_game_avg', true, true, NULL),
	(19, 'NBA', 'fg3a', 'Three-Pointers Att', 'player', 'shooting', false, false, true, 26, 'per_game_avg', true, true, NULL),
	(20, 'NBA', 'ftm', 'Free Throws Made', 'player', 'shooting', false, false, true, 27, 'per_game_avg', true, true, NULL),
	(21, 'NBA', 'fta', 'Free Throws Attempted', 'player', 'shooting', false, false, true, 28, 'per_game_avg', true, true, NULL),
	(30, 'NBA', 'losses', 'Losses', 'team', 'standings', true, false, true, 2, 'special', false, false, NULL),
	(22, 'NBA', 'pts_per_36', 'Points Per 36 Min', 'player', 'advanced', false, true, false, 30, 'per_game_avg', true, false, NULL),
	(23, 'NBA', 'reb_per_36', 'Rebounds Per 36 Min', 'player', 'advanced', false, true, false, 31, 'per_game_avg', true, false, NULL),
	(24, 'NBA', 'ast_per_36', 'Assists Per 36 Min', 'player', 'advanced', false, true, false, 32, 'per_game_avg', true, false, NULL),
	(25, 'NBA', 'stl_per_36', 'Steals Per 36 Min', 'player', 'advanced', false, true, false, 33, 'per_game_avg', true, false, NULL),
	(26, 'NBA', 'blk_per_36', 'Blocks Per 36 Min', 'player', 'advanced', false, true, false, 34, 'per_game_avg', true, false, NULL),
	(1151, 'FOOTBALL', 'big_chances_created_per_90', 'Big Chances Created/90', 'player', 'passing', false, true, false, 215, 'per_game_avg', true, false, NULL),
	(1152, 'FOOTBALL', 'long_balls_per_90', 'Long Balls Per 90', 'player', 'passing', false, true, false, 216, 'per_game_avg', true, false, NULL),
	(1153, 'FOOTBALL', 'long_balls_won_per_90', 'Long Balls Won Per 90', 'player', 'passing', false, true, false, 217, 'per_game_avg', true, false, NULL),
	(1154, 'FOOTBALL', 'through_balls_per_90', 'Through Balls Per 90', 'player', 'passing', false, true, false, 218, 'per_game_avg', true, false, NULL),
	(1155, 'FOOTBALL', 'through_balls_won_per_90', 'Through Balls Won Per 90', 'player', 'passing', false, true, false, 219, 'per_game_avg', true, false, NULL),
	(1156, 'FOOTBALL', 'passes_in_final_third_per_90', 'Final-Third Passes/90', 'player', 'passing', false, true, false, 220, 'per_game_avg', true, false, NULL),
	(1157, 'FOOTBALL', 'tackles_won_per_90', 'Tackles Won Per 90', 'player', 'defensive', false, true, false, 221, 'per_game_avg', true, false, NULL);


--
-- Name: stat_definitions_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.stat_definitions_id_seq', 1358, true);


--
-- PostgreSQL database dump complete
--

\unrestrict 6Qb7g7EY0dCYqzlg3exnCAgZnFSXbEDxo7jnVcAUQMEFnJb257zKotbFwX7O017

--
-- PostgreSQL database dump
--

\restrict GRS5KHntfJYNCbdXr9uQ4QPiroF4I9QRrlQJa0nUvLvNjRlq3WyMMAYSR6PAn82

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: rate_modes; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.rate_modes (sport, mode, suffix, denom_key, formula, unit, round_digits) VALUES
	('NBA', 'per_36', '_per_36', 'minutes', 'div_mul', 36, 1),
	('NBA', 'per_season', '_per_season', 'games_played', 'mul', NULL, 0),
	('FOOTBALL', 'per_90', '_per_90', 'minutes_played', 'mul_div', 90, 3),
	('FOOTBALL', 'per_game', '_per_game', 'appearances', 'div', NULL, 3),
	('NFL', 'per_game', '_per_game', 'games_played', 'div', NULL, 2);


--
-- PostgreSQL database dump complete
--

\unrestrict GRS5KHntfJYNCbdXr9uQ4QPiroF4I9QRrlQJa0nUvLvNjRlq3WyMMAYSR6PAn82

--
-- PostgreSQL database dump
--

\restrict oiFHnucK3RXrAfukMtYkkSEBurHcs1y1WuFHPpabNqqH1ZqsuyakjVpCWpyGj1h

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: rating_thresholds; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.rating_thresholds (sport, stat_key, min_value) VALUES
	('NBA', 'games_played', 30),
	('NBA', 'minutes', 20),
	('NFL', 'games_played', 8),
	('FOOTBALL', 'appearances', 10);


--
-- PostgreSQL database dump complete
--

\unrestrict oiFHnucK3RXrAfukMtYkkSEBurHcs1y1WuFHPpabNqqH1ZqsuyakjVpCWpyGj1h

--
-- PostgreSQL database dump
--

\restrict tM3fnwyltSczYihh61g5Z89WkZRbARwpbrEbx8ez0qeUNRhdAXoId4GuA0xzIJx

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: stat_templates; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.stat_templates (sport, position_group, stat_key, sort_order, facet, variant) VALUES
	('NFL', 'quarterback', 'passing_attempts', 10, NULL, 'vendor'),
	('NFL', 'quarterback', 'passing_yards', 20, NULL, 'vendor'),
	('NFL', 'quarterback', 'passing_touchdowns', 30, NULL, 'vendor'),
	('NFL', 'quarterback', 'passing_interceptions', 40, NULL, 'vendor'),
	('NFL', 'quarterback', 'rushing_yards', 50, NULL, 'vendor'),
	('NFL', 'quarterback', 'rushing_touchdowns', 60, NULL, 'vendor'),
	('NFL', 'running-back', 'rushing_attempts', 10, NULL, 'vendor'),
	('NFL', 'running-back', 'rushing_yards', 20, NULL, 'vendor'),
	('NFL', 'running-back', 'rushing_touchdowns', 30, NULL, 'vendor'),
	('NFL', 'running-back', 'receptions', 40, NULL, 'vendor'),
	('NFL', 'running-back', 'receiving_yards', 50, NULL, 'vendor'),
	('NFL', 'running-back', 'receiving_touchdowns', 60, NULL, 'vendor'),
	('NFL', 'receiver', 'receiving_targets', 10, NULL, 'vendor'),
	('NFL', 'receiver', 'receptions', 20, NULL, 'vendor'),
	('NFL', 'receiver', 'receiving_yards', 30, NULL, 'vendor'),
	('NFL', 'receiver', 'receiving_touchdowns', 40, NULL, 'vendor'),
	('NFL', 'receiver', 'receiving_first_downs', 50, NULL, 'vendor'),
	('NBA', 'ALL', 'pts', 10, NULL, 'vendor'),
	('NBA', 'ALL', 'reb', 20, NULL, 'vendor'),
	('NBA', 'ALL', 'ast', 30, NULL, 'vendor'),
	('NBA', 'ALL', 'stl', 40, NULL, 'vendor'),
	('NBA', 'ALL', 'blk', 50, NULL, 'vendor'),
	('NBA', 'ALL', 'fg3m', 60, NULL, 'vendor'),
	('NBA', 'ALL', 'turnover', 70, NULL, 'vendor'),
	('FOOTBALL', 'goalkeeper', 'saves', 10, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'save_pct', 11, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'goals_conceded', 12, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'expected_goals_conceded', 13, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'clean_sheets', 14, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'penalties_saved', 15, 'shot-stopping', 'fpl'),
	('FOOTBALL', 'defender', 'tackles', 10, 'defending', 'fpl'),
	('FOOTBALL', 'defender', 'cbi', 11, 'defending', 'fpl'),
	('FOOTBALL', 'defender', 'defensive_contribution', 12, 'defending', 'fpl'),
	('FOOTBALL', 'defender', 'ball_recovery', 13, 'defending', 'fpl'),
	('FOOTBALL', 'defender', 'clean_sheets', 14, 'defending', 'fpl'),
	('FOOTBALL', 'defender', 'expected_assists', 20, 'passing', 'fpl'),
	('FOOTBALL', 'defender', 'goals', 30, 'attacking', 'fpl'),
	('FOOTBALL', 'defender', 'assists', 31, 'attacking', 'fpl'),
	('FOOTBALL', 'defender', 'expected_goal_involvements', 32, 'attacking', 'fpl'),
	('FOOTBALL', 'midfielder', 'expected_assists', 10, 'passing', 'fpl'),
	('FOOTBALL', 'midfielder', 'goals', 20, 'attacking', 'fpl'),
	('FOOTBALL', 'midfielder', 'expected_goals', 21, 'attacking', 'fpl'),
	('FOOTBALL', 'midfielder', 'assists', 22, 'attacking', 'fpl'),
	('FOOTBALL', 'midfielder', 'expected_goal_involvements', 23, 'attacking', 'fpl'),
	('FOOTBALL', 'midfielder', 'tackles', 30, 'defending', 'fpl'),
	('FOOTBALL', 'midfielder', 'defensive_contribution', 31, 'defending', 'fpl'),
	('FOOTBALL', 'midfielder', 'ball_recovery', 32, 'defending', 'fpl'),
	('FOOTBALL', 'midfielder', 'cbi', 33, 'defending', 'fpl'),
	('FOOTBALL', 'attacker', 'goals', 10, 'attacking', 'fpl'),
	('FOOTBALL', 'attacker', 'expected_goals', 11, 'attacking', 'fpl'),
	('FOOTBALL', 'attacker', 'assists', 12, 'attacking', 'fpl'),
	('FOOTBALL', 'attacker', 'expected_goal_involvements', 13, 'attacking', 'fpl'),
	('FOOTBALL', 'attacker', 'expected_assists', 20, 'passing', 'fpl'),
	('FOOTBALL', 'attacker', 'tackles', 30, 'defending', 'fpl'),
	('FOOTBALL', 'attacker', 'defensive_contribution', 31, 'defending', 'fpl'),
	('FOOTBALL', 'attacker', 'ball_recovery', 32, 'defending', 'fpl'),
	('FOOTBALL', 'team', 'goals_for', 10, 'offense', 'fpl'),
	('FOOTBALL', 'team', 'expected_goals_for', 11, 'offense', 'fpl'),
	('FOOTBALL', 'team', 'assists', 12, 'offense', 'fpl'),
	('FOOTBALL', 'team', 'goals_against', 20, 'defense', 'fpl'),
	('FOOTBALL', 'team', 'expected_goals_against', 21, 'defense', 'fpl'),
	('FOOTBALL', 'team', 'clean_sheets', 22, 'defense', 'fpl'),
	('FOOTBALL', 'team', 'tackles', 23, 'defense', 'fpl'),
	('FOOTBALL', 'team', 'saves', 24, 'defense', 'fpl'),
	('FOOTBALL', 'team', 'ball_recovery', 25, 'defense', 'fpl'),
	('FOOTBALL', 'goalkeeper', 'saves', 10, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'save_pct', 11, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'saves_insidebox', 12, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'goals_conceded', 13, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'penalties_saved', 14, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'punches', 15, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'good_high_claim', 16, 'shot-stopping', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'passes_total', 20, 'passing', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'pass_accuracy', 21, 'passing', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'long_balls', 22, 'passing', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'long_balls_won', 23, 'passing', 'vendor'),
	('FOOTBALL', 'goalkeeper', 'long_ball_accuracy', 24, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'tackles', 10, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'interceptions', 11, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'clearances', 12, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'blocks', 13, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'aeriels_won', 14, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'ball_recovery', 15, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'duels_won', 16, 'defending', 'vendor'),
	('FOOTBALL', 'defender', 'passes_total', 20, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'pass_accuracy', 21, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'long_balls_won', 22, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'long_ball_accuracy', 23, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'passes_in_final_third', 24, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'key_passes', 25, 'passing', 'vendor'),
	('FOOTBALL', 'defender', 'goals', 30, 'attacking', 'vendor'),
	('FOOTBALL', 'defender', 'assists', 31, 'attacking', 'vendor'),
	('FOOTBALL', 'defender', 'shots_total', 32, 'attacking', 'vendor'),
	('FOOTBALL', 'defender', 'shots_on_target', 33, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'key_passes', 10, 'passing', 'vendor'),
	('FOOTBALL', 'midfielder', 'chances_created', 11, 'passing', 'vendor'),
	('FOOTBALL', 'midfielder', 'big_chances_created', 12, 'passing', 'vendor'),
	('FOOTBALL', 'midfielder', 'passes_total', 13, 'passing', 'vendor'),
	('FOOTBALL', 'midfielder', 'pass_accuracy', 14, 'passing', 'vendor'),
	('FOOTBALL', 'midfielder', 'passes_in_final_third', 15, 'passing', 'vendor');
INSERT INTO public.stat_templates (sport, position_group, stat_key, sort_order, facet, variant) VALUES
	('FOOTBALL', 'midfielder', 'goals', 20, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'assists', 21, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'shots_total', 22, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'shots_on_target', 23, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'shot_accuracy', 24, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'dribbles_success', 25, 'attacking', 'vendor'),
	('FOOTBALL', 'midfielder', 'tackles', 30, 'defending', 'vendor'),
	('FOOTBALL', 'midfielder', 'interceptions', 31, 'defending', 'vendor'),
	('FOOTBALL', 'midfielder', 'ball_recovery', 32, 'defending', 'vendor'),
	('FOOTBALL', 'midfielder', 'duels_won', 33, 'defending', 'vendor'),
	('FOOTBALL', 'midfielder', 'duel_success_rate', 34, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'goals', 10, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'assists', 11, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'shots_total', 12, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'shots_on_target', 13, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'shot_accuracy', 14, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'dribbles_success', 15, 'attacking', 'vendor'),
	('FOOTBALL', 'attacker', 'key_passes', 20, 'passing', 'vendor'),
	('FOOTBALL', 'attacker', 'chances_created', 21, 'passing', 'vendor'),
	('FOOTBALL', 'attacker', 'big_chances_created', 22, 'passing', 'vendor'),
	('FOOTBALL', 'attacker', 'crosses_accurate', 23, 'passing', 'vendor'),
	('FOOTBALL', 'attacker', 'pass_accuracy', 24, 'passing', 'vendor'),
	('FOOTBALL', 'attacker', 'tackles', 30, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'interceptions', 31, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'ball_recovery', 32, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'duels_won', 33, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'duel_success_rate', 34, 'defending', 'vendor'),
	('FOOTBALL', 'attacker', 'aeriels_won', 35, 'defending', 'vendor'),
	('NFL', 'team', 'points_for', 10, 'offense', 'vendor'),
	('NFL', 'team', 'total_yards', 11, 'offense', 'vendor'),
	('NFL', 'team', 'passing_yards', 12, 'offense', 'vendor'),
	('NFL', 'team', 'rushing_yards', 13, 'offense', 'vendor'),
	('NFL', 'team', 'passing_touchdowns', 14, 'offense', 'vendor'),
	('NFL', 'team', 'rushing_touchdowns', 15, 'offense', 'vendor'),
	('NFL', 'team', 'turnovers', 16, 'offense', 'vendor'),
	('NFL', 'team', 'points_against', 20, 'defense', 'vendor'),
	('NFL', 'team', 'defensive_sacks', 21, 'defense', 'vendor'),
	('NFL', 'team', 'defensive_interceptions', 22, 'defense', 'vendor'),
	('NFL', 'team', 'takeaways', 23, 'defense', 'vendor'),
	('NFL', 'team', 'total_tackles', 24, 'defense', 'vendor'),
	('NFL', 'team', 'passes_defended', 25, 'defense', 'vendor'),
	('NFL', 'team', 'tackles_for_loss', 26, 'defense', 'vendor'),
	('NBA', 'team', 'pts', 10, 'offense', 'vendor'),
	('NBA', 'team', 'ast', 11, 'offense', 'vendor'),
	('NBA', 'team', 'fg3m', 12, 'offense', 'vendor'),
	('NBA', 'team', 'true_shooting_pct', 13, 'offense', 'vendor'),
	('NBA', 'team', 'oreb', 14, 'offense', 'vendor'),
	('NBA', 'team', 'turnover', 15, 'offense', 'vendor'),
	('NBA', 'team', 'pts_allowed', 20, 'defense', 'vendor'),
	('NBA', 'team', 'reb', 21, 'defense', 'vendor'),
	('NBA', 'team', 'stl', 22, 'defense', 'vendor'),
	('NBA', 'team', 'blk', 23, 'defense', 'vendor'),
	('NBA', 'team', 'dreb', 24, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'goals_for', 10, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'shots_on_target', 11, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'big_chances_created', 12, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'key_passes', 13, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'assists', 14, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'possession_pct', 15, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'pass_accuracy', 16, 'offense', 'vendor'),
	('FOOTBALL', 'team', 'goals_against', 20, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'tackles', 21, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'interceptions', 22, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'clearances', 23, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'blocked_shots', 24, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'saves', 25, 'defense', 'vendor'),
	('FOOTBALL', 'team', 'aerials_won', 26, 'defense', 'vendor');


--
-- PostgreSQL database dump complete
--

\unrestrict tM3fnwyltSczYihh61g5Z89WkZRbARwpbrEbx8ez0qeUNRhdAXoId4GuA0xzIJx

--
-- PostgreSQL database dump
--

\restrict TROogUZanmMzD7t7HGUKpClHmy6rEyiG48oVWFPzb94xZn4F2A5ZEuSa4IDN5yt

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: entity_fact_policy; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.entity_fact_policy (entity_type, fact_type, tier) VALUES
	('player', 'date_of_birth', 'evidenced'),
	('player', 'weight_kg', 'evidenced'),
	('player', 'height_cm', 'evidenced'),
	('player', 'photo_url', 'evidenced'),
	('person', 'role', 'evidenced'),
	('person', 'date_of_birth', 'evidenced'),
	('person', 'photo_url', 'evidenced'),
	('person', 'team_affiliation', 'adjudicated'),
	('team', 'venue_name', 'evidenced'),
	('team', 'logo_url', 'evidenced');


--
-- PostgreSQL database dump complete
--

\unrestrict TROogUZanmMzD7t7HGUKpClHmy6rEyiG48oVWFPzb94xZn4F2A5ZEuSa4IDN5yt

--
-- PostgreSQL database dump
--

\restrict 4icgCQEtNm9QSAnrJkRzc5ulXtA77jhmqpylNIZfCDn2ATaKVkLEl4qlceFhEwp

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: transfer_identity_thresholds; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.transfer_identity_thresholds (sport, min_heat, min_deterministic_confidence, min_adjudication_confidence, allowed_event_types, updated_at) VALUES
	('NBA', 80, 0.800, 0.850, '{transfer,trade,loan,signing}', '2026-07-03 14:01:42.970311-04'),
	('NFL', 80, 0.800, 0.850, '{transfer,trade,loan,signing}', '2026-07-03 14:01:42.970311-04'),
	('FOOTBALL', 80, 0.800, 0.850, '{transfer,trade,loan,signing}', '2026-07-03 14:01:42.970311-04');


--
-- PostgreSQL database dump complete
--

\unrestrict 4icgCQEtNm9QSAnrJkRzc5ulXtA77jhmqpylNIZfCDn2ATaKVkLEl4qlceFhEwp

--
-- PostgreSQL database dump
--

\restrict 7XojuVXL1AjilvrAvwsY3VASsNO0PEyg6UaNYgZFbergwUImA4EfR4oz8G3V9a3

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: stage_routing_subscriptions; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.stage_routing_subscriptions (tag, stage, entity_type, note, created_at) VALUES
	('charged', 'vibe', 'player', 'PLAN-one-rail 7.4/7.6 — the Influencer wakes on a charged packet (E3, first-voice-capable).', '2026-08-06 10:53:30.980723-04'),
	('charged', 'vibe', 'team', 'PLAN-one-rail 7.4/7.6 — same, at team grain. Two rows because neither trigger reads ''*'' as a wildcard (D-T15).', '2026-08-06 10:53:30.980723-04'),
	('narratives', 'narratives', 'player', 'PLAN-one-rail 8.6 — the Journalist''s waker. Mig 212 made arm 2 subscription-gated, so he needs a row; arm 2 ignores the tag and reads only (stage, entity_type).', '2026-08-06 10:53:30.980723-04'),
	('narratives', 'narratives', 'team', 'PLAN-one-rail 8.6 — same, at team grain.', '2026-08-06 10:53:30.980723-04'),
	('transfer', 'transfers', 'team', 'PLAN-one-rail 7.4/7.5 — the Insider''s packet slice. Safe only because mig 175''s article-grain trigger is dropped in this same transaction (D-T15 resolution (a)).', '2026-08-06 10:53:30.980723-04'),
	('injury', 'rating', 'player', 'PLAN-one-rail — the Scout''s availability tag (Scott 2026-08-23: the Editor tags, the Scout judges legitimacy). Fires off slice_fingerprints->>''rating'', which hashes the injury/suspension claims, so the enqueue collapses per CHANGE OF FACT rather than per packet. Carries suspensions too — one tag, one consequence.', '2026-08-24 17:38:57.671882-04'),
	('injury', 'rating', 'team', 'PLAN-one-rail — same, at team grain. Two rows because neither trigger reads ''*'' as a wildcard (D-T15). A club needs to see its own absentees.', '2026-08-24 17:38:57.671882-04');


--
-- PostgreSQL database dump complete
--

\unrestrict 7XojuVXL1AjilvrAvwsY3VASsNO0PEyg6UaNYgZFbergwUImA4EfR4oz8G3V9a3

--
-- PostgreSQL database dump
--

\restrict pvohGHgJs4J7hsDSZ4Z9MnnkjGkgGuLxZv4sLznTG4WsfQuqyz5k6TW4IbV33sZ

-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'SQL_ASCII';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Data for Name: boxscore_sources; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Name: boxscore_sources_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.boxscore_sources_id_seq', 7, true);


--
-- PostgreSQL database dump complete
--

\unrestrict pvohGHgJs4J7hsDSZ4Z9MnnkjGkgGuLxZv4sLznTG4WsfQuqyz5k6TW4IbV33sZ
