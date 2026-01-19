"""
Player-related queries for stats database.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional

from ..pg_connection import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

if TYPE_CHECKING:
    from ..connection import StatsDB


class PlayerQueries:
    """Query utilities for player statistics."""

    def __init__(self, db: "StatsDB"):
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
        percentiles = self.db.get_percentiles("player", player_id, sport_id, season_year)

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

        # Determine table
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": self._get_nfl_table_for_stat(stat_name),
            "FOOTBALL": "football_player_stats",
        }

        stats_table = table_map.get(sport_id)
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

    def compare_players(
        self,
        player_ids: list[int],
        sport_id: str,
        season_year: int,
        stat_categories: Optional[list[str]] = None,
    ) -> list[dict[str, Any]]:
        """
        Compare multiple players across stats.

        Args:
            player_ids: List of player IDs to compare
            sport_id: Sport identifier
            season_year: Season year
            stat_categories: Optional specific stats to compare

        Returns:
            List of player profiles for comparison
        """
        profiles = []
        for player_id in player_ids:
            profile = self.get_player_profile(player_id, sport_id, season_year)
            if profile:
                profiles.append(profile)

        return profiles

    def search_players_by_stats(
        self,
        sport_id: str,
        season_year: int,
        filters: dict[str, tuple[str, float]],
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        """
        Search players by stat criteria.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            filters: Dict of {stat_name: (operator, value)}
                     e.g., {"points_per_game": (">=", 20)}
            limit: Max results

        Returns:
            List of matching players
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        table_map = {
            "NBA": "nba_player_stats",
            "FOOTBALL": "football_player_stats",
        }

        stats_table = table_map.get(sport_id)
        player_profile_table = PLAYER_PROFILE_TABLES.get(sport_id)
        team_profile_table = TEAM_PROFILE_TABLES.get(sport_id)
        
        if not stats_table or not player_profile_table or not team_profile_table:
            return []

        # Build WHERE conditions (no sport_id filter needed - tables are sport-specific)
        conditions = ["s.season_id = %s"]
        params: list[Any] = [season_id]

        for stat_name, (operator, value) in filters.items():
            if operator in (">=", "<=", ">", "<", "="):
                conditions.append(f"s.{stat_name} {operator} %s")
                params.append(value)

        params.append(limit)

        query = f"""
            SELECT
                p.id as player_id,
                p.full_name,
                p.position,
                t.name as team_name,
                s.*
            FROM {stats_table} s
            JOIN {player_profile_table} p ON s.player_id = p.id
            LEFT JOIN {team_profile_table} t ON p.current_team_id = t.id
            WHERE {' AND '.join(conditions)}
            LIMIT %s
        """

        return self.db.fetchall(query, tuple(params))

    def _get_nfl_table_for_stat(self, stat_name: str) -> str:
        """Get the NFL table containing a stat.

        Returns the unified nfl_player_stats table for PostgreSQL.
        """
        # All NFL stats are now in the unified table
        return "nfl_player_stats"
