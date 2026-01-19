"""
Models - re-exports from core.models for backwards compatibility.

New code should import directly from scoracle_data.core.models.
"""

# Re-export everything from core.models
from .core.models import (
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
