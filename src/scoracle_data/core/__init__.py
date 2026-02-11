"""
Core module for Scoracle Data.

This module provides the foundational components:
- Configuration management (config.py)
- Data models (models.py)
- Type definitions and sport registry (types.py)
- Shared HTTP client infrastructure (http.py)

Usage:
    from scoracle_data.core import Settings, get_settings
    from scoracle_data.core import Sport, EntityType, get_sport_config
    from scoracle_data.core import PlayerModel, TeamModel
    from scoracle_data.core.http import BaseApiClient, ExternalAPIError
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
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
    LEAGUES_TABLE,
)

# Models
from .models import (
    TeamModel,
    PlayerModel,
    PercentileResult,
    EntityPercentiles,
    ProfileStatus,
    PlayerProfile,
    TeamProfile,
    EntityMinimal,
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
    # Unified table name constants
    "PLAYERS_TABLE",
    "PLAYER_STATS_TABLE",
    "TEAMS_TABLE",
    "TEAM_STATS_TABLE",
    "LEAGUES_TABLE",
    # Models
    "TeamModel",
    "PlayerModel",
    "PercentileResult",
    "EntityPercentiles",
    "ProfileStatus",
    "PlayerProfile",
    "TeamProfile",
    "EntityMinimal",
]
