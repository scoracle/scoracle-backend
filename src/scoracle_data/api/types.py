"""
Shared types and constants for API routers.

This module provides a central registry for all sport-specific configurations.
Update SPORT_REGISTRY when adding new sports or changing sport configurations.
"""

from dataclasses import dataclass
from enum import Enum
from typing import Optional


class EntityType(str, Enum):
    """Entity types."""
    player = "player"
    team = "team"


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


@dataclass(frozen=True)
class SportConfig:
    """
    Configuration for a sport.

    Centralizes all sport-specific settings to avoid scattered hardcoded values.
    """
    # Identifiers
    id: str
    name: str
    api_base_url: str

    # Current season (update annually)
    current_season: int

    # Database tables - Profiles (sport-specific, v4.0 schema)
    player_profile_table: str
    team_profile_table: str

    # Database tables - Stats
    player_stats_table: str
    team_stats_table: str

    # API-Sports configuration
    default_league_id: Optional[int] = None

    # Season label format
    season_label_format: str = "{year}"  # e.g., "2024-25" for NBA

    # Whether this sport uses league_id in player profiles
    has_league_in_profiles: bool = False

    def get_season_label(self, year: int) -> str:
        """Generate human-readable season label."""
        if self.season_label_format == "{year}-{next_year_short}":
            return f"{year}-{str(year + 1)[-2:]}"
        return self.season_label_format.format(year=year)


# =============================================================================
# SPORT REGISTRY - Central configuration for all sports
# =============================================================================
# Update this registry when:
# - Adding a new sport
# - Changing current season (annually)
# - Modifying API configuration

SPORT_REGISTRY: dict[str, SportConfig] = {
    Sport.NBA.value: SportConfig(
        id="NBA",
        name="National Basketball Association",
        api_base_url="https://v2.nba.api-sports.io",
        current_season=2025,
        player_profile_table="nba_player_profiles",
        team_profile_table="nba_team_profiles",
        player_stats_table="nba_player_stats",
        team_stats_table="nba_team_stats",
        season_label_format="{year}-{next_year_short}",
    ),
    Sport.NFL.value: SportConfig(
        id="NFL",
        name="National Football League",
        api_base_url="https://v1.american-football.api-sports.io",
        current_season=2025,
        player_profile_table="nfl_player_profiles",
        team_profile_table="nfl_team_profiles",
        player_stats_table="nfl_player_stats",
        team_stats_table="nfl_team_stats",
        default_league_id=1,
    ),
    Sport.FOOTBALL.value: SportConfig(
        id="FOOTBALL",
        name="Football (Soccer)",
        api_base_url="https://v3.football.api-sports.io",
        current_season=2024,
        player_profile_table="football_player_profiles",
        team_profile_table="football_team_profiles",
        player_stats_table="football_player_stats",
        team_stats_table="football_team_stats",
        default_league_id=39,  # Premier League
        has_league_in_profiles=True,
    ),
}


def get_sport_config(sport: str | Sport) -> SportConfig:
    """
    Get configuration for a sport.

    Args:
        sport: Sport ID string or Sport enum

    Returns:
        SportConfig for the requested sport

    Raises:
        KeyError: If sport is not in registry
    """
    sport_id = sport.value if isinstance(sport, Sport) else sport
    return SPORT_REGISTRY[sport_id]


# =============================================================================
# Legacy compatibility - derived from SPORT_REGISTRY
# =============================================================================
# These are provided for backwards compatibility with existing code.
# New code should use SPORT_REGISTRY or get_sport_config() directly.

CURRENT_SEASONS = {
    sport_id: config.current_season
    for sport_id, config in SPORT_REGISTRY.items()
}

PLAYER_STATS_TABLES = {
    sport_id: config.player_stats_table
    for sport_id, config in SPORT_REGISTRY.items()
}

TEAM_STATS_TABLES = {
    sport_id: config.team_stats_table
    for sport_id, config in SPORT_REGISTRY.items()
}

PLAYER_PROFILE_TABLES = {
    sport_id: config.player_profile_table
    for sport_id, config in SPORT_REGISTRY.items()
}

TEAM_PROFILE_TABLES = {
    sport_id: config.team_profile_table
    for sport_id, config in SPORT_REGISTRY.items()
}
