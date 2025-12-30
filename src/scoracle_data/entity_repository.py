"""
EntityRepository: Local-first data access for entity profiles.

This module provides a unified interface for accessing player and team data
from the local stats database, eliminating the need for live API calls
during user requests.

Features:
- Local-first data access (<10ms response times)
- Priority league full profiles with stats and percentiles
- Non-priority league "building" status with minimal data
- Autocomplete/search functionality
- Fallback handling for missing data
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any, Optional

from .models import (
    EntityMinimal,
    EntityPercentiles,
    PercentileResult,
    PlayerModel,
    PlayerProfile,
    ProfileStatus,
    TeamModel,
    TeamProfile,
)

if TYPE_CHECKING:
    from .connection import StatsDB

logger = logging.getLogger(__name__)


class EntityRepository:
    """
    Repository for accessing entity profiles from the local stats database.

    This class replaces live API calls with local database lookups, providing
    fast (<10ms) access to player and team data for widget rendering.
    """

    def __init__(self, db: "StatsDB"):
        """
        Initialize the EntityRepository.

        Args:
            db: StatsDB connection instance
        """
        self.db = db

    # =========================================================================
    # Player Profiles
    # =========================================================================

    def get_player_profile(
        self,
        player_id: int,
        sport_id: str,
        season_year: Optional[int] = None,
    ) -> Optional[PlayerProfile]:
        """
        Get a complete player profile with stats and percentiles.

        For priority league players: Full profile with all data
        For non-priority league players: Minimal data with "building" status

        Args:
            player_id: Player ID
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Optional season year. If None, uses current season.

        Returns:
            PlayerProfile or None if not found
        """
        # Get player base info
        player_data = self.db.get_player(player_id, sport_id)
        if not player_data:
            return None

        # Get current season if not specified
        if season_year is None:
            current_season = self.db.get_current_season(sport_id)
            if current_season:
                season_year = current_season["season_year"]
            else:
                season_year = 2024  # Fallback

        # Check league priority (for Football)
        league_info = None
        is_priority_league = True  # NBA/NFL are always priority
        has_percentiles = True

        if sport_id == "FOOTBALL" and player_data.get("current_league_id"):
            league_info = self._get_league_info(player_data["current_league_id"])
            if league_info:
                is_priority_league = league_info.get("priority_tier", 0) == 1
                has_percentiles = league_info.get("include_in_percentiles", 0) == 1

        # Build PlayerModel
        player = PlayerModel(**{
            k: player_data.get(k) for k in PlayerModel.model_fields.keys()
            if k in player_data
        })

        # Get team if available
        team = None
        if player_data.get("current_team_id"):
            team_data = self.db.get_team(player_data["current_team_id"], sport_id)
            if team_data:
                team = TeamModel(**{
                    k: team_data.get(k) for k in TeamModel.model_fields.keys()
                    if k in team_data
                })

        # For non-priority leagues, return minimal profile
        if not is_priority_league:
            return PlayerProfile(
                player=player,
                team=team,
                stats=None,
                percentiles=None,
                comparison_group=None,
                status=ProfileStatus.BUILDING,
            )

        # Get stats
        stats = self.db.get_player_stats(player_id, sport_id, season_year)

        # Get percentiles (only if league has percentiles)
        percentiles = None
        comparison_group = None
        if has_percentiles:
            percentile_data = self.db.get_percentiles("player", player_id, sport_id, season_year)
            if percentile_data:
                percentiles = EntityPercentiles(
                    entity_type="player",
                    entity_id=player_id,
                    sport_id=sport_id,
                    season_year=season_year,
                    percentiles=[
                        PercentileResult(**p) for p in percentile_data
                    ],
                )
                # Get comparison group from first percentile
                if percentile_data:
                    comparison_group = percentile_data[0].get("comparison_group")

        return PlayerProfile(
            player=player,
            team=team,
            stats=stats,
            percentiles=percentiles,
            comparison_group=comparison_group,
            status=ProfileStatus.COMPLETE,
        )

    def get_player_minimal(
        self,
        player_id: int,
        sport_id: str,
    ) -> Optional[EntityMinimal]:
        """
        Get minimal player data (for autocomplete/search).

        Args:
            player_id: Player ID
            sport_id: Sport identifier

        Returns:
            EntityMinimal or None if not found
        """
        player = self.db.get_player(player_id, sport_id)
        if not player:
            return None

        return EntityMinimal(
            id=player["id"],
            entity_type="player",
            sport_id=sport_id,
            league_id=player.get("current_league_id"),
            name=player["full_name"],
            normalized_name=self._normalize_name(player["full_name"]),
            tokens=self._tokenize_name(player["full_name"]),
        )

    # =========================================================================
    # Team Profiles
    # =========================================================================

    def get_team_profile(
        self,
        team_id: int,
        sport_id: str,
        season_year: Optional[int] = None,
    ) -> Optional[TeamProfile]:
        """
        Get a complete team profile with stats and percentiles.

        Args:
            team_id: Team ID
            sport_id: Sport identifier
            season_year: Optional season year

        Returns:
            TeamProfile or None if not found
        """
        team_data = self.db.get_team(team_id, sport_id)
        if not team_data:
            return None

        # Get current season if not specified
        if season_year is None:
            current_season = self.db.get_current_season(sport_id)
            if current_season:
                season_year = current_season["season_year"]
            else:
                season_year = 2024

        # Check league priority (for Football)
        is_priority_league = True
        has_percentiles = True

        if sport_id == "FOOTBALL" and team_data.get("league_id"):
            league_info = self._get_league_info(team_data["league_id"])
            if league_info:
                is_priority_league = league_info.get("priority_tier", 0) == 1
                has_percentiles = league_info.get("include_in_percentiles", 0) == 1

        # Build TeamModel
        team = TeamModel(**{
            k: team_data.get(k) for k in TeamModel.model_fields.keys()
            if k in team_data
        })

        # For non-priority leagues, return minimal profile
        if not is_priority_league:
            return TeamProfile(
                team=team,
                stats=None,
                percentiles=None,
                status=ProfileStatus.BUILDING,
            )

        # Get stats
        stats = self.db.get_team_stats(team_id, sport_id, season_year)

        # Get percentiles
        percentiles = None
        if has_percentiles:
            percentile_data = self.db.get_percentiles("team", team_id, sport_id, season_year)
            if percentile_data:
                percentiles = EntityPercentiles(
                    entity_type="team",
                    entity_id=team_id,
                    sport_id=sport_id,
                    season_year=season_year,
                    percentiles=[
                        PercentileResult(**p) for p in percentile_data
                    ],
                )

        return TeamProfile(
            team=team,
            stats=stats,
            percentiles=percentiles,
            status=ProfileStatus.COMPLETE,
        )

    def get_team_minimal(
        self,
        team_id: int,
        sport_id: str,
    ) -> Optional[EntityMinimal]:
        """
        Get minimal team data (for autocomplete/search).

        Args:
            team_id: Team ID
            sport_id: Sport identifier

        Returns:
            EntityMinimal or None if not found
        """
        team = self.db.get_team(team_id, sport_id)
        if not team:
            return None

        return EntityMinimal(
            id=team["id"],
            entity_type="team",
            sport_id=sport_id,
            league_id=team.get("league_id"),
            name=team["name"],
            normalized_name=self._normalize_name(team["name"]),
            tokens=self._tokenize_name(team["name"]),
        )

    # =========================================================================
    # Search & Autocomplete
    # =========================================================================

    def search_entities(
        self,
        query: str,
        sport_id: Optional[str] = None,
        entity_type: Optional[str] = None,
        league_id: Optional[int] = None,
        limit: int = 20,
    ) -> list[EntityMinimal]:
        """
        Search for entities by name.

        Args:
            query: Search query
            sport_id: Optional sport filter
            entity_type: Optional entity type filter ("player" or "team")
            league_id: Optional league filter
            limit: Max results

        Returns:
            List of matching entities
        """
        results = []
        normalized_query = self._normalize_name(query)

        if entity_type in (None, "player"):
            results.extend(self._search_players(normalized_query, sport_id, league_id, limit))

        if entity_type in (None, "team"):
            results.extend(self._search_teams(normalized_query, sport_id, league_id, limit))

        # Sort by relevance (exact match first, then prefix, then contains)
        def sort_key(e: EntityMinimal) -> tuple:
            normalized = self._normalize_name(e.name)
            if normalized == normalized_query:
                return (0, len(e.name))
            if normalized.startswith(normalized_query):
                return (1, len(e.name))
            return (2, len(e.name))

        results.sort(key=sort_key)
        return results[:limit]

    def _search_players(
        self,
        normalized_query: str,
        sport_id: Optional[str],
        league_id: Optional[int],
        limit: int,
    ) -> list[EntityMinimal]:
        """Search players by normalized name."""
        conditions = ["1=1"]
        params: list[Any] = []

        if sport_id:
            conditions.append("sport_id = %s")
            params.append(sport_id)

        if league_id:
            conditions.append("current_league_id = %s")
            params.append(league_id)

        # Use LIKE for prefix/contains matching
        conditions.append("LOWER(full_name) LIKE %s")
        params.append(f"%{normalized_query}%")

        params.append(limit)

        query = f"""
            SELECT id, sport_id, current_league_id as league_id, full_name as name
            FROM players
            WHERE {' AND '.join(conditions)}
            LIMIT %s
        """

        rows = self.db.fetchall(query, tuple(params))

        return [
            EntityMinimal(
                id=row["id"],
                entity_type="player",
                sport_id=row["sport_id"],
                league_id=row.get("league_id"),
                name=row["name"],
                normalized_name=self._normalize_name(row["name"]),
                tokens=self._tokenize_name(row["name"]),
            )
            for row in rows
        ]

    def _search_teams(
        self,
        normalized_query: str,
        sport_id: Optional[str],
        league_id: Optional[int],
        limit: int,
    ) -> list[EntityMinimal]:
        """Search teams by normalized name."""
        conditions = ["1=1"]
        params: list[Any] = []

        if sport_id:
            conditions.append("sport_id = %s")
            params.append(sport_id)

        if league_id:
            conditions.append("league_id = %s")
            params.append(league_id)

        conditions.append("LOWER(name) LIKE %s")
        params.append(f"%{normalized_query}%")

        params.append(limit)

        query = f"""
            SELECT id, sport_id, league_id, name
            FROM teams
            WHERE {' AND '.join(conditions)}
            LIMIT %s
        """

        rows = self.db.fetchall(query, tuple(params))

        return [
            EntityMinimal(
                id=row["id"],
                entity_type="team",
                sport_id=row["sport_id"],
                league_id=row.get("league_id"),
                name=row["name"],
                normalized_name=self._normalize_name(row["name"]),
                tokens=self._tokenize_name(row["name"]),
            )
            for row in rows
        ]

    # =========================================================================
    # Batch Operations
    # =========================================================================

    def get_player_profiles_batch(
        self,
        player_ids: list[int],
        sport_id: str,
        season_year: Optional[int] = None,
    ) -> dict[int, PlayerProfile]:
        """
        Get multiple player profiles efficiently.

        Args:
            player_ids: List of player IDs
            sport_id: Sport identifier
            season_year: Optional season year

        Returns:
            Dict mapping player_id to PlayerProfile
        """
        return {
            pid: profile
            for pid in player_ids
            if (profile := self.get_player_profile(pid, sport_id, season_year))
        }

    def get_team_profiles_batch(
        self,
        team_ids: list[int],
        sport_id: str,
        season_year: Optional[int] = None,
    ) -> dict[int, TeamProfile]:
        """
        Get multiple team profiles efficiently.

        Args:
            team_ids: List of team IDs
            sport_id: Sport identifier
            season_year: Optional season year

        Returns:
            Dict mapping team_id to TeamProfile
        """
        return {
            tid: profile
            for tid in team_ids
            if (profile := self.get_team_profile(tid, sport_id, season_year))
        }

    # =========================================================================
    # League & Priority Info
    # =========================================================================

    def _get_league_info(self, league_id: int) -> Optional[dict[str, Any]]:
        """Get league info including priority tier."""
        return self.db.fetchone(
            "SELECT * FROM leagues WHERE id = %s",
            (league_id,),
        )

    def get_priority_leagues(self, sport_id: str) -> list[dict[str, Any]]:
        """Get all priority leagues for a sport."""
        return self.db.fetchall(
            "SELECT * FROM leagues WHERE sport_id = %s AND priority_tier = 1",
            (sport_id,),
        )

    def get_percentile_leagues(self, sport_id: str) -> list[dict[str, Any]]:
        """Get leagues included in percentile calculations."""
        return self.db.fetchall(
            "SELECT * FROM leagues WHERE sport_id = %s AND include_in_percentiles = true",
            (sport_id,),
        )

    def is_priority_entity(
        self,
        entity_id: int,
        entity_type: str,
        sport_id: str,
    ) -> bool:
        """
        Check if an entity belongs to a priority league.

        Args:
            entity_id: Entity ID
            entity_type: "player" or "team"
            sport_id: Sport identifier

        Returns:
            True if entity is in a priority league
        """
        # NBA/NFL are always priority
        if sport_id in ("NBA", "NFL"):
            return True

        if entity_type == "player":
            player = self.db.get_player(entity_id, sport_id)
            if player and player.get("current_league_id"):
                league = self._get_league_info(player["current_league_id"])
                return league and league.get("priority_tier", 0) == 1

        elif entity_type == "team":
            team = self.db.get_team(entity_id, sport_id)
            if team and team.get("league_id"):
                league = self._get_league_info(team["league_id"])
                return league and league.get("priority_tier", 0) == 1

        return True  # Default to priority if can't determine

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _normalize_name(self, name: str) -> str:
        """Normalize a name for search matching."""
        import unicodedata
        # Remove accents
        normalized = unicodedata.normalize("NFKD", name)
        normalized = "".join(c for c in normalized if not unicodedata.combining(c))
        # Lowercase and strip
        return normalized.lower().strip()

    def _tokenize_name(self, name: str) -> str:
        """Tokenize a name for search indexing."""
        normalized = self._normalize_name(name)
        # Split on spaces and common separators
        tokens = normalized.replace("-", " ").replace("'", "").split()
        return " ".join(sorted(set(tokens)))


# Global instance factory
def get_entity_repository(db=None) -> EntityRepository:
    """
    Get an EntityRepository instance.

    Args:
        db: Optional database instance. If None, uses PostgreSQL by default.

    Returns:
        EntityRepository instance
    """
    if db is None:
        from .pg_connection import get_postgres_db
        db = get_postgres_db()

    return EntityRepository(db)
