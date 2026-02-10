"""
Core types and constants for Scoracle Data.

This module provides:
- EntityType and Sport enums
- SportConfig dataclass for sport-specific settings
- SPORT_REGISTRY for centralized sport configurations

All sports share the same 4 unified tables (players, player_stats, teams, team_stats)
with sport-specific data in JSONB columns.
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

    All sports share the same unified tables (players, player_stats, teams,
    team_stats) — differentiated by the `sport` column.
    """

    # Identifiers
    id: str
    name: str
    api_base_url: str

    # Current season (update annually)
    current_season: int

    # API-Sports configuration
    default_league_id: Optional[int] = None

    # Season label format
    season_label_format: str = "{year}"  # e.g., "2024-25" for NBA

    def get_season_label(self, year: int) -> str:
        """Generate human-readable season label."""
        if self.season_label_format == "{year}-{next_year_short}":
            return f"{year}-{str(year + 1)[-2:]}"
        return self.season_label_format.format(year=year)


# =============================================================================
# SPORT REGISTRY - Central configuration for all sports
# =============================================================================
# All sports use the unified tables: players, player_stats, teams, team_stats.
# Sport-specific data lives in JSONB (stats, meta) columns.

SPORT_REGISTRY: dict[str, SportConfig] = {
    Sport.NBA.value: SportConfig(
        id="NBA",
        name="National Basketball Association",
        api_base_url="https://api.balldontlie.io/v1",
        current_season=2025,
        season_label_format="{year}-{next_year_short}",
    ),
    Sport.NFL.value: SportConfig(
        id="NFL",
        name="National Football League",
        api_base_url="https://api.balldontlie.io/nfl/v1",
        current_season=2025,
        default_league_id=1,
    ),
    Sport.FOOTBALL.value: SportConfig(
        id="FOOTBALL",
        name="Football (Soccer)",
        api_base_url="https://api.sportmonks.com/v3/football",
        current_season=2025,
        default_league_id=1,  # Premier League (internal ID)
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
# Unified table names — shared across all sports
# =============================================================================

PLAYERS_TABLE = "players"
PLAYER_STATS_TABLE = "player_stats"
TEAMS_TABLE = "teams"
TEAM_STATS_TABLE = "team_stats"
LEAGUES_TABLE = "leagues"
