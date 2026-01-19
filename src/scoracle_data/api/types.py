"""
API types - re-exports from core.types for backwards compatibility.

New code should import directly from scoracle_data.core.types.
"""

# Re-export everything from core.types
from ..core.types import (
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

__all__ = [
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
]
