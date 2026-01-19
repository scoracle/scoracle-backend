"""
Core module for Scoracle Data.

This module provides the foundational components:
- Configuration management (config.py)
- Data models (models.py)
- Type definitions and sport registry (types.py)

Usage:
    from scoracle_data.core import Settings, get_settings
    from scoracle_data.core import Sport, EntityType, get_sport_config
    from scoracle_data.core import PlayerModel, TeamModel
"""

# Configuration
from .config import Settings, get_settings

# Types
from .types import (
    EntityType,
    Sport,
    SportConfig,
    SPORT_REGISTRY,
    get_sport_config,
    CURRENT_SEASONS,
    PLAYER_STATS_TABLES,
    TEAM_STATS_TABLES,
    PLAYER_PROFILE_TABLES,
    TEAM_PROFILE_TABLES,
)

# Models
from .models import (
    SportModel,
    SeasonModel,
    LeagueModel,
    TeamModel,
    PlayerModel,
    NBAPlayerStats,
    NBATeamStats,
    PercentileResult,
    EntityPercentiles,
    ProfileStatus,
    PlayerProfile,
    TeamProfile,
    EntityMinimal,
    ComparisonResult,
    RankingEntry,
    StatRankings,
    StatItem,
    StatCategory,
    EntityInfo,
    CategorizedStatsResponse,
)

__all__ = [
    # Config
    "Settings",
    "get_settings",
    # Types
    "EntityType",
    "Sport",
    "SportConfig",
    "SPORT_REGISTRY",
    "get_sport_config",
    "CURRENT_SEASONS",
    "PLAYER_STATS_TABLES",
    "TEAM_STATS_TABLES",
    "PLAYER_PROFILE_TABLES",
    "TEAM_PROFILE_TABLES",
    # Models
    "SportModel",
    "SeasonModel",
    "LeagueModel",
    "TeamModel",
    "PlayerModel",
    "NBAPlayerStats",
    "NBATeamStats",
    "PercentileResult",
    "EntityPercentiles",
    "ProfileStatus",
    "PlayerProfile",
    "TeamProfile",
    "EntityMinimal",
    "ComparisonResult",
    "RankingEntry",
    "StatRankings",
    "StatItem",
    "StatCategory",
    "EntityInfo",
    "CategorizedStatsResponse",
]
