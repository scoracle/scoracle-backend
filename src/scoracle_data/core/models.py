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
