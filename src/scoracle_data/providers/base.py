"""
Base data provider protocol and types.

Defines the interface that all data providers must implement,
ensuring provider-agnostic data fetching.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Literal


class ProviderError(Exception):
    """Base exception for provider errors."""
    pass


class RateLimitError(ProviderError):
    """Raised when provider rate limit is exceeded."""
    
    def __init__(self, message: str, retry_after: int | None = None):
        super().__init__(message)
        self.retry_after = retry_after


class AuthenticationError(ProviderError):
    """Raised when provider authentication fails."""
    pass


@dataclass
class RawEntityData:
    """
    Provider-agnostic entity data container.
    
    Contains both the mapped canonical data (for immediate use)
    and the full raw API response (for JSONB storage and future field extraction).
    
    Attributes:
        entity_type: Type of entity (team, player, player_stats, team_stats)
        provider_id: Original ID from the data provider
        canonical_data: Dict of mapped fields using our canonical names
        raw_response: Complete original API response for JSONB storage
        provider_name: Name of the provider that fetched this data
        fetched_at: Timestamp when data was fetched
        context: Optional context data (e.g., team_id when fetching players)
    """
    entity_type: Literal["team", "player", "player_stats", "team_stats"]
    provider_id: str
    canonical_data: dict[str, Any]
    raw_response: dict[str, Any]
    provider_name: str
    fetched_at: datetime = field(default_factory=datetime.now)
    context: dict[str, Any] = field(default_factory=dict)
    
    def __post_init__(self):
        # Ensure provider_id is always a string
        self.provider_id = str(self.provider_id)


@dataclass
class FetchResult:
    """
    Result of a batch fetch operation.
    
    Attributes:
        entities: List of fetched entities
        total_count: Total number of entities available (for pagination)
        page: Current page number (1-indexed)
        has_more: Whether more pages are available
        errors: List of errors encountered during fetch
    """
    entities: list[RawEntityData]
    total_count: int = 0
    page: int = 1
    has_more: bool = False
    errors: list[str] = field(default_factory=list)


class DataProviderProtocol(ABC):
    """
    Abstract interface for data providers.
    
    All data providers (API-Sports, SportRadar, etc.) must implement
    this interface to ensure consistent behavior across providers.
    
    The provider is responsible for:
    1. Making API calls to the external service
    2. Mapping API response fields to canonical field names (using config)
    3. Returning both canonical data and raw response for JSONB storage
    
    The provider is NOT responsible for:
    - Database operations (handled by repositories)
    - Calculated stats (handled by post-processing)
    - Business logic (handled by seeders)
    """
    
    # Provider identifier (e.g., "api_sports", "sportradar")
    provider_name: str = ""
    
    # ==========================================================================
    # Team Operations
    # ==========================================================================
    
    @abstractmethod
    async def fetch_teams(
        self,
        sport: str,
        season: int,
        *,
        league_id: int | None = None,
    ) -> list[RawEntityData]:
        """
        Fetch teams for discovery phase.
        
        Returns minimal team data needed for roster discovery.
        Full team profiles are fetched separately via fetch_team_profile().
        
        Args:
            sport: Sport identifier (NBA, NFL, FOOTBALL)
            season: Season year
            league_id: Optional league filter (required for FOOTBALL)
            
        Returns:
            List of RawEntityData with team discovery fields
        """
        ...
    
    @abstractmethod
    async def fetch_team_profile(
        self,
        sport: str,
        team_id: int,
    ) -> RawEntityData | None:
        """
        Fetch complete team profile.
        
        Returns full team details including venue info, founded year, etc.
        
        Args:
            sport: Sport identifier
            team_id: Team ID to fetch
            
        Returns:
            RawEntityData with full profile, or None if not found
        """
        ...
    
    @abstractmethod
    async def fetch_team_stats(
        self,
        sport: str,
        team_id: int,
        season: int,
        *,
        league_id: int | None = None,
    ) -> RawEntityData | None:
        """
        Fetch team statistics for a season.
        
        Args:
            sport: Sport identifier
            team_id: Team ID
            season: Season year
            league_id: Optional league filter
            
        Returns:
            RawEntityData with team stats, or None if not found
        """
        ...
    
    # ==========================================================================
    # Player Operations
    # ==========================================================================
    
    @abstractmethod
    async def fetch_players(
        self,
        sport: str,
        season: int,
        *,
        team_id: int | None = None,
        league_id: int | None = None,
    ) -> list[RawEntityData]:
        """
        Fetch players for discovery phase.
        
        Returns minimal player data from roster. Full profiles are
        fetched separately via fetch_player_profile().
        
        Args:
            sport: Sport identifier
            season: Season year
            team_id: Optional team filter
            league_id: Optional league filter
            
        Returns:
            List of RawEntityData with player discovery fields
        """
        ...
    
    @abstractmethod
    async def fetch_player_profile(
        self,
        sport: str,
        player_id: int,
    ) -> RawEntityData | None:
        """
        Fetch complete player profile.
        
        Returns full biographical data including birth info, physical
        attributes, career history, etc.
        
        Args:
            sport: Sport identifier
            player_id: Player ID to fetch
            
        Returns:
            RawEntityData with full profile, or None if not found
        """
        ...
    
    @abstractmethod
    async def fetch_player_stats(
        self,
        sport: str,
        player_id: int,
        season: int,
        *,
        league_id: int | None = None,
    ) -> RawEntityData | None:
        """
        Fetch player statistics for a season.
        
        For sports with game-level data (NBA), this should aggregate
        game logs into season totals.
        
        Args:
            sport: Sport identifier
            player_id: Player ID
            season: Season year
            league_id: Optional league filter
            
        Returns:
            RawEntityData with player stats, or None if not found
        """
        ...
    
    # ==========================================================================
    # Optional Operations (may not be supported by all providers)
    # ==========================================================================
    
    async def fetch_standings(
        self,
        sport: str,
        season: int,
        *,
        league_id: int | None = None,
    ) -> list[RawEntityData]:
        """
        Fetch league standings.
        
        Optional - not all providers support this directly.
        
        Args:
            sport: Sport identifier
            season: Season year
            league_id: Optional league filter
            
        Returns:
            List of RawEntityData with standings data
        """
        raise NotImplementedError(f"{self.provider_name} does not support standings")
    
    async def fetch_fixtures(
        self,
        sport: str,
        season: int,
        *,
        league_id: int | None = None,
        team_id: int | None = None,
        from_date: datetime | None = None,
        to_date: datetime | None = None,
    ) -> list[RawEntityData]:
        """
        Fetch game fixtures/schedule.
        
        Optional - not all providers support this.
        
        Args:
            sport: Sport identifier
            season: Season year
            league_id: Optional league filter
            team_id: Optional team filter
            from_date: Start date filter
            to_date: End date filter
            
        Returns:
            List of RawEntityData with fixture data
        """
        raise NotImplementedError(f"{self.provider_name} does not support fixtures")
    
    # ==========================================================================
    # Utility Methods
    # ==========================================================================
    
    async def health_check(self) -> bool:
        """
        Check if provider is accessible and authenticated.
        
        Returns:
            True if provider is healthy
        """
        return True
    
    def get_rate_limit_info(self) -> dict[str, Any]:
        """
        Get current rate limit status.
        
        Returns:
            Dict with rate limit info (requests_remaining, reset_time, etc.)
        """
        return {}
