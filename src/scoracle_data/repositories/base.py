"""
Base repository protocols.

Defines abstract interfaces for data persistence operations,
enabling database-agnostic data access.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from ..connection import StatsDB


class PlayerRepository(ABC):
    """
    Abstract interface for player data access.
    
    Implementations handle sport-specific table routing
    (e.g., nba_player_profiles vs nfl_player_profiles).
    """
    
    @abstractmethod
    def upsert(
        self,
        sport: str,
        player: dict[str, Any],
        raw_response: dict[str, Any],
        *,
        mark_profile_fetched: bool = False,
    ) -> int:
        """
        Insert or update a player record.
        
        Args:
            sport: Sport identifier (NBA, NFL, FOOTBALL)
            player: Player data with canonical field names
            raw_response: Full API response for JSONB storage
            mark_profile_fetched: If True, set profile_fetched_at to now
            
        Returns:
            Player ID
        """
        ...
    
    @abstractmethod
    def batch_upsert(
        self,
        sport: str,
        players: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """
        Batch insert or update player records.
        
        Args:
            sport: Sport identifier
            players: List of player data dicts
            raw_responses: List of raw API responses (same order as players)
            
        Returns:
            Number of players upserted
        """
        ...
    
    @abstractmethod
    def find_by_id(self, sport: str, player_id: int) -> dict[str, Any] | None:
        """
        Find a player by ID.
        
        Args:
            sport: Sport identifier
            player_id: Player ID
            
        Returns:
            Player record dict, or None if not found
        """
        ...
    
    @abstractmethod
    def find_needing_profiles(self, sport: str) -> list[int]:
        """
        Find players that need profile fetch.
        
        Returns players where profile_fetched_at is NULL.
        
        Args:
            sport: Sport identifier
            
        Returns:
            List of player IDs needing profile fetch
        """
        ...
    
    @abstractmethod
    def mark_profile_fetched(self, sport: str, player_id: int) -> None:
        """
        Mark a player's profile as fetched.
        
        Args:
            sport: Sport identifier
            player_id: Player ID
        """
        ...
    
    @abstractmethod
    def get_all_ids(self, sport: str) -> list[int]:
        """
        Get all player IDs for a sport.
        
        Args:
            sport: Sport identifier
            
        Returns:
            List of all player IDs
        """
        ...
    
    def extract_raw_field(
        self,
        sport: str,
        player_id: int,
        json_path: str,
    ) -> Any:
        """
        Extract a field from stored raw_response.
        
        Useful for accessing fields that weren't originally mapped.
        
        Args:
            sport: Sport identifier
            player_id: Player ID
            json_path: PostgreSQL JSON path (e.g., "birth->>'country'")
            
        Returns:
            Extracted value, or None if not found
        """
        raise NotImplementedError("Raw field extraction not implemented")


class TeamRepository(ABC):
    """
    Abstract interface for team data access.
    
    Implementations handle sport-specific table routing.
    """
    
    @abstractmethod
    def upsert(
        self,
        sport: str,
        team: dict[str, Any],
        raw_response: dict[str, Any],
        *,
        mark_profile_fetched: bool = False,
    ) -> int:
        """Insert or update a team record."""
        ...
    
    @abstractmethod
    def batch_upsert(
        self,
        sport: str,
        teams: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update team records."""
        ...
    
    @abstractmethod
    def find_by_id(self, sport: str, team_id: int) -> dict[str, Any] | None:
        """Find a team by ID."""
        ...
    
    @abstractmethod
    def find_needing_profiles(self, sport: str) -> list[int]:
        """Find teams that need profile fetch."""
        ...
    
    @abstractmethod
    def mark_profile_fetched(self, sport: str, team_id: int) -> None:
        """Mark a team's profile as fetched."""
        ...
    
    @abstractmethod
    def get_all_ids(self, sport: str) -> list[int]:
        """Get all team IDs for a sport."""
        ...


class PlayerStatsRepository(ABC):
    """
    Abstract interface for player statistics data access.
    """
    
    @abstractmethod
    def upsert(
        self,
        sport: str,
        stats: dict[str, Any],
        raw_response: dict[str, Any],
    ) -> None:
        """
        Insert or update player stats.
        
        Args:
            sport: Sport identifier
            stats: Stats data with canonical field names
            raw_response: Full API response for JSONB storage
        """
        ...
    
    @abstractmethod
    def batch_upsert(
        self,
        sport: str,
        stats_list: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update player stats."""
        ...
    
    @abstractmethod
    def find_by_player_season(
        self,
        sport: str,
        player_id: int,
        season_id: int,
        team_id: int | None = None,
    ) -> dict[str, Any] | None:
        """Find player stats for a specific season."""
        ...
    
    @abstractmethod
    def get_season_stats(
        self,
        sport: str,
        season_id: int,
        *,
        min_games: int = 0,
    ) -> list[dict[str, Any]]:
        """
        Get all player stats for a season.
        
        Args:
            sport: Sport identifier
            season_id: Season ID
            min_games: Minimum games played filter
            
        Returns:
            List of player stat records
        """
        ...


class TeamStatsRepository(ABC):
    """
    Abstract interface for team statistics data access.
    """
    
    @abstractmethod
    def upsert(
        self,
        sport: str,
        stats: dict[str, Any],
        raw_response: dict[str, Any],
    ) -> None:
        """Insert or update team stats."""
        ...
    
    @abstractmethod
    def batch_upsert(
        self,
        sport: str,
        stats_list: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update team stats."""
        ...
    
    @abstractmethod
    def find_by_team_season(
        self,
        sport: str,
        team_id: int,
        season_id: int,
    ) -> dict[str, Any] | None:
        """Find team stats for a specific season."""
        ...
    
    @abstractmethod
    def get_season_stats(
        self,
        sport: str,
        season_id: int,
    ) -> list[dict[str, Any]]:
        """Get all team stats for a season."""
        ...


@dataclass
class RepositorySet:
    """
    Collection of all repositories.
    
    Provides convenient access to all repository implementations.
    """
    players: PlayerRepository
    teams: TeamRepository
    player_stats: PlayerStatsRepository
    team_stats: TeamStatsRepository
