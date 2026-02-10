"""
Pydantic models for stats database entities.

These models are used for:
- Validating data from API-Sports before insertion
- Type-safe query results
- API response serialization
"""

from __future__ import annotations

from typing import Any, Optional

from pydantic import BaseModel, Field, computed_field


# =============================================================================
# Core Entity Models
# =============================================================================


class SportModel(BaseModel):
    """Sport registry entry."""

    id: str
    display_name: str
    api_base_url: str
    current_season: int
    is_active: bool = True


class LeagueModel(BaseModel):
    """League registry entry (for multi-league sports)."""

    id: int
    sport: str
    name: str
    country: Optional[str] = None
    country_code: Optional[str] = None
    logo_url: Optional[str] = None
    priority_tier: int = 0  # 1 = full data, 0 = minimal
    include_in_percentiles: int = 0  # 1 = used in percentile calcs, 0 = excluded
    is_active: bool = True

    @computed_field
    @property
    def is_priority(self) -> bool:
        """Whether this league has full data coverage."""
        return self.priority_tier == 1

    @computed_field
    @property
    def has_percentiles(self) -> bool:
        """Whether this league is included in percentile calculations."""
        return self.include_in_percentiles == 1


class TeamModel(BaseModel):
    """Team master record (unified teams table)."""

    id: int
    sport: str
    league_id: Optional[int] = None
    name: str
    short_code: Optional[str] = None
    logo_url: Optional[str] = None
    country: Optional[str] = None
    city: Optional[str] = None
    founded: Optional[int] = None
    venue_name: Optional[str] = None
    venue_capacity: Optional[int] = None
    meta: Optional[dict[str, Any]] = None


class PlayerModel(BaseModel):
    """Player master record (unified players table)."""

    id: int
    sport: str
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    name: str
    position: Optional[str] = None
    team_id: Optional[int] = None
    league_id: Optional[int] = None
    meta: Optional[dict[str, Any]] = None


# =============================================================================
# Percentile Models
# =============================================================================


class PercentileResult(BaseModel):
    """Result of a percentile calculation for a single stat."""

    stat_category: str
    stat_value: float
    percentile: float = Field(ge=0, le=100)
    rank: int
    sample_size: int
    comparison_group: Optional[str] = None


class EntityPercentiles(BaseModel):
    """All percentiles for an entity."""

    entity_type: str  # 'player' or 'team'
    entity_id: int
    sport: str
    season: int
    percentiles: list[PercentileResult]

    @computed_field
    @property
    def percentile_map(self) -> dict[str, float]:
        """Get percentiles as a simple stat -> value map."""
        return {p.stat_category: p.percentile for p in self.percentiles}


# =============================================================================
# API Response Models
# =============================================================================


class ProfileStatus:
    """Status constants for entity profiles."""

    COMPLETE = "complete"  # Full data available
    BUILDING = "building"  # Non-priority league, minimal data


class PlayerProfile(BaseModel):
    """Complete player profile with stats and percentiles."""

    player: PlayerModel
    team: Optional[TeamModel] = None
    stats: Optional[dict[str, Any]] = None
    percentiles: Optional[EntityPercentiles] = None
    comparison_group: Optional[str] = None
    status: str = ProfileStatus.COMPLETE  # "complete" or "building"


class TeamProfile(BaseModel):
    """Complete team profile with stats and percentiles."""

    team: TeamModel
    stats: Optional[dict[str, Any]] = None
    percentiles: Optional[EntityPercentiles] = None
    status: str = ProfileStatus.COMPLETE  # "complete" or "building"


class EntityMinimal(BaseModel):
    """Minimal entity data for non-priority leagues (autocomplete only)."""

    id: int
    entity_type: str  # "team" or "player"
    sport: str
    league_id: Optional[int] = None
    name: str
    normalized_name: Optional[str] = None
    tokens: Optional[str] = None


class ComparisonResult(BaseModel):
    """Result of comparing two or more entities."""

    entities: list[PlayerProfile | TeamProfile]
    stat_categories: list[str]
    comparison_data: list[dict[str, Any]]


class RankingEntry(BaseModel):
    """Single entry in a ranking list."""

    rank: int
    entity_id: int
    entity_name: str
    team_name: Optional[str] = None
    stat_value: float
    percentile: float


class StatRankings(BaseModel):
    """Rankings for a specific stat."""

    sport: str
    season: int
    stat_category: str
    position_filter: Optional[str] = None
    league_filter: Optional[int] = None
    total_count: int
    rankings: list[RankingEntry]


# =============================================================================
# Categorized Stats Response Models (for Stats Widget)
# =============================================================================


class StatItem(BaseModel):
    """Individual stat with optional percentile data."""

    key: str
    label: str
    value: float | int | str | None
    percentile: Optional[float] = None
    rank: Optional[int] = None
    sample_size: Optional[int] = None


class StatCategory(BaseModel):
    """Group of related stats with category metadata."""

    id: str
    label: str
    volume: int  # Number of non-null stats in this category
    stats: list[StatItem]


class EntityInfo(BaseModel):
    """Basic entity identification for stats response."""

    id: str
    name: str
    type: str  # 'player' or 'team'
    position: Optional[str] = None
    team: Optional[str] = None
    photo_url: Optional[str] = None
    logo_url: Optional[str] = None


class CategorizedStatsResponse(BaseModel):
    """Stats response with category grouping and percentiles for widgets."""

    season: str
    entity: EntityInfo
    categories: list[StatCategory]
    comparison_group: Optional[str] = None
    source: str = "local"  # 'local' or 'api'
