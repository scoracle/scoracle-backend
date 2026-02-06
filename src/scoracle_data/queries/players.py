"""
Player-related queries for stats database.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional

from ..core.types import PLAYER_PROFILE_TABLES, PLAYER_STATS_TABLES, TEAM_PROFILE_TABLES

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB


class PlayerQueries:
    """Query utilities for player statistics."""

    def __init__(self, db: "PostgresDB"):
        self.db = db

    def get_player_profile(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete player profile with stats and percentiles.

        Args:
            player_id: Player ID
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            Dict with player info, stats, and percentiles
        """
        player = self.db.get_player(player_id, sport_id)
        if not player:
            return None

        stats = self.db.get_player_stats(player_id, sport_id, season_year)
        # Percentiles stored as JSONB in the stats row
        percentiles = stats.get("percentiles") if stats else None

        # Get team info if available
        team = None
        if player.get("current_team_id"):
            team = self.db.get_team(player["current_team_id"], sport_id)

        return {
            "player": dict(player),
            "team": dict(team) if team else None,
            "stats": stats,
            "percentiles": percentiles,
        }

    def get_stat_leaders(
        self,
        sport_id: str,
        season_year: int,
        stat_name: str,
        limit: int = 25,
        position_group: Optional[str] = None,
    ) -> list[dict[str, Any]]:
        """
        Get top players for a specific stat.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            stat_name: Stat to rank by
            limit: Number of results
            position_group: Optional position filter

        Returns:
            List of player stats ranked by the stat
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        # Determine table (NFL may use position-specific table)
        if sport_id == "NFL":
            stats_table = self._get_nfl_table_for_stat(stat_name)
        else:
            stats_table = PLAYER_STATS_TABLES.get(sport_id)
        player_profile_table = PLAYER_PROFILE_TABLES.get(sport_id)
        team_profile_table = TEAM_PROFILE_TABLES.get(sport_id)
        
        if not stats_table or not player_profile_table or not team_profile_table:
            return []

        # Build query using sport-specific profile tables
        if position_group:
            query = f"""
                SELECT
                    p.id as player_id,
                    p.full_name,
                    p.position,
                    t.name as team_name,
                    s.{stat_name} as stat_value
                FROM {stats_table} s
                JOIN {player_profile_table} p ON s.player_id = p.id
                LEFT JOIN {team_profile_table} t ON p.current_team_id = t.id
                WHERE s.season_id = %s
                  AND p.position_group = %s
                  AND s.{stat_name} IS NOT NULL
                ORDER BY s.{stat_name} DESC
                LIMIT %s
            """
            params = (season_id, position_group, limit)
        else:
            query = f"""
                SELECT
                    p.id as player_id,
                    p.full_name,
                    p.position,
                    t.name as team_name,
                    s.{stat_name} as stat_value
                FROM {stats_table} s
                JOIN {player_profile_table} p ON s.player_id = p.id
                LEFT JOIN {team_profile_table} t ON p.current_team_id = t.id
                WHERE s.season_id = %s
                  AND s.{stat_name} IS NOT NULL
                ORDER BY s.{stat_name} DESC
                LIMIT %s
            """
            params = (season_id, limit)

        rows = self.db.fetchall(query, params)

        # Add rank
        return [
            {**dict(row), "rank": i + 1}
            for i, row in enumerate(rows)
        ]

    def _get_nfl_table_for_stat(self, stat_name: str) -> str:
        """Get the NFL table containing a stat.

        Returns the unified nfl_player_stats table for PostgreSQL.
        """
        # All NFL stats are now in the unified table
        return "nfl_player_stats"
