"""Football table constants - ONLY Football tables, no cross-sport contamination."""

# Football Tables
LEAGUES_TABLE = "football_leagues"
TEAM_PROFILE_TABLE = "football_team_profiles"
PLAYER_PROFILE_TABLE = "football_player_profiles"
PLAYER_STATS_TABLE = "football_player_stats"
TEAM_STATS_TABLE = "football_team_stats"

# Top 5 European Leagues with SportMonks IDs
LEAGUES = {
    1: {"sportmonks_id": 8, "name": "Premier League", "country": "England"},
    2: {"sportmonks_id": 564, "name": "La Liga", "country": "Spain"},
    3: {"sportmonks_id": 82, "name": "Bundesliga", "country": "Germany"},
    4: {"sportmonks_id": 384, "name": "Serie A", "country": "Italy"},
    5: {"sportmonks_id": 301, "name": "Ligue 1", "country": "France"},
}

# Season IDs for Premier League (from 2020 onwards)
PREMIER_LEAGUE_SEASONS = {
    2020: 17420,  # 2020-21
    2021: 18378,  # 2021-22
    2022: 19734,  # 2022-23
    2023: 21646,  # 2023-24
    2024: 23614,  # 2024-25
    2025: 25583,  # Current/Future
}
