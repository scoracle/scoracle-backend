"""
Pydantic models for stats database entities.

These models are used for:
- Validating data from API-Sports before insertion
- Type-safe query results
- API response serialization
"""

from __future__ import annotations

from datetime import date, datetime
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


class SeasonModel(BaseModel):
    """Season registry entry."""

    id: int
    sport_id: str
    season_year: int
    season_label: Optional[str] = None
    is_current: bool = False
    start_date: Optional[str] = None
    end_date: Optional[str] = None
    games_played: int = 0


class LeagueModel(BaseModel):
    """League registry entry (for multi-league sports)."""

    id: int
    sport_id: str
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
    """Team master record."""

    id: int
    sport_id: str
    league_id: Optional[int] = None
    name: str
    abbreviation: Optional[str] = None
    logo_url: Optional[str] = None
    conference: Optional[str] = None
    division: Optional[str] = None
    country: Optional[str] = None
    city: Optional[str] = None
    founded: Optional[int] = None
    venue_name: Optional[str] = None
    venue_city: Optional[str] = None
    venue_capacity: Optional[int] = None
    venue_surface: Optional[str] = None
    venue_image: Optional[str] = None
    profile_fetched_at: Optional[int] = None  # NULL = needs fetch, timestamp = fetched
    is_active: bool = True

    @computed_field
    @property
    def needs_profile_fetch(self) -> bool:
        """Whether this team needs its profile fetched."""
        return self.profile_fetched_at is None


class PlayerModel(BaseModel):
    """Player master record."""

    id: int
    sport_id: str
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    full_name: str
    position: Optional[str] = None
    position_group: Optional[str] = None
    nationality: Optional[str] = None
    birth_date: Optional[str] = None
    birth_place: Optional[str] = None
    height_inches: Optional[int] = None  # Height in inches
    weight_lbs: Optional[int] = None  # Weight in pounds
    photo_url: Optional[str] = None
    current_team_id: Optional[int] = None
    current_league_id: Optional[int] = None  # For Football players (percentile filtering)
    jersey_number: Optional[int] = None
    college: Optional[str] = None  # College attended (NFL/NBA)
    experience_years: Optional[int] = None  # Years of professional experience (NFL)
    profile_fetched_at: Optional[int] = None  # NULL = needs fetch, timestamp = fetched
    is_active: bool = True

    @computed_field
    @property
    def needs_profile_fetch(self) -> bool:
        """Whether this player needs its profile fetched."""
        return self.profile_fetched_at is None

    @computed_field
    @property
    def height_display(self) -> Optional[str]:
        """Height formatted as feet-inches (e.g., 6'2\")."""
        if self.height_inches is None:
            return None
        feet = self.height_inches // 12
        inches = self.height_inches % 12
        return f"{feet}'{inches}\""

    @computed_field
    @property
    def weight_display(self) -> Optional[str]:
        """Weight formatted with units (e.g., 238 lbs)."""
        if self.weight_lbs is None:
            return None
        return f"{self.weight_lbs} lbs"


# =============================================================================
# NBA Statistics Models
# =============================================================================


class NBAPlayerStats(BaseModel):
    """NBA player season statistics."""

    player_id: int
    season_id: int
    team_id: Optional[int] = None

    # Games & Minutes
    games_played: int = 0
    games_started: int = 0
    minutes_total: int = 0
    minutes_per_game: float = 0.0

    # Scoring
    points_total: int = 0
    points_per_game: float = 0.0

    # Field Goals
    fgm: int = 0
    fga: int = 0
    fg_pct: float = 0.0

    # Three Pointers
    tpm: int = 0
    tpa: int = 0
    tp_pct: float = 0.0

    # Free Throws
    ftm: int = 0
    fta: int = 0
    ft_pct: float = 0.0

    # Rebounds
    offensive_rebounds: int = 0
    defensive_rebounds: int = 0
    total_rebounds: int = 0
    rebounds_per_game: float = 0.0

    # Assists & Turnovers
    assists: int = 0
    assists_per_game: float = 0.0
    turnovers: int = 0
    turnovers_per_game: float = 0.0

    # Defense
    steals: int = 0
    steals_per_game: float = 0.0
    blocks: int = 0
    blocks_per_game: float = 0.0

    # Fouls
    personal_fouls: int = 0
    fouls_per_game: float = 0.0

    # Advanced
    plus_minus: int = 0
    plus_minus_per_game: float = 0.0
    efficiency: float = 0.0
    true_shooting_pct: float = 0.0
    effective_fg_pct: float = 0.0
    assist_turnover_ratio: float = 0.0

    double_doubles: int = 0
    triple_doubles: int = 0


class NBATeamStats(BaseModel):
    """NBA team season statistics."""

    team_id: int
    season_id: int

    # Record
    games_played: int = 0
    wins: int = 0
    losses: int = 0
    win_pct: float = 0.0
    home_wins: int = 0
    home_losses: int = 0
    away_wins: int = 0
    away_losses: int = 0

    # Scoring
    points_per_game: float = 0.0
    opponent_ppg: float = 0.0
    point_differential: float = 0.0

    # Shooting
    fg_pct: float = 0.0
    tp_pct: float = 0.0
    ft_pct: float = 0.0

    # Rebounds
    total_rebounds_per_game: float = 0.0

    # Other
    assists_per_game: float = 0.0
    steals_per_game: float = 0.0
    blocks_per_game: float = 0.0
    turnovers_per_game: float = 0.0

    # Advanced
    offensive_rating: float = 0.0
    defensive_rating: float = 0.0
    net_rating: float = 0.0
    pace: float = 0.0


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
    sport_id: str
    season_year: int
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
    sport_id: str
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

    sport_id: str
    season_year: int
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
