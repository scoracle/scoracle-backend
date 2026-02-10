"""
Player-related queries for stats database.

Uses the unified players/player_stats tables with JSONB stats.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional

from ..core.types import PLAYERS_TABLE, PLAYER_STATS_TABLE, TEAMS_TABLE

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
        player = self.db.fetchone(
            f"SELECT * FROM {PLAYERS_TABLE} WHERE id = %s AND sport = %s",
            (player_id, sport_id),
        )
        if not player:
            return None

        stats = self.db.fetchone(
            f"SELECT * FROM {PLAYER_STATS_TABLE} WHERE player_id = %s AND sport = %s AND season = %s",
            (player_id, sport_id, season_year),
        )

        # Get team info if available
        team = None
        if player.get("team_id"):
            team = self.db.fetchone(
                f"SELECT * FROM {TEAMS_TABLE} WHERE id = %s AND sport = %s",
                (player["team_id"], sport_id),
            )

        return {
            "player": dict(player),
            "team": dict(team) if team else None,
            "stats": dict(stats) if stats else None,
            "percentiles": stats.get("percentiles") if stats else None,
        }

    def get_stat_leaders(
        self,
        sport_id: str,
        season_year: int,
        stat_name: str,
        limit: int = 25,
        position: Optional[str] = None,
        league_id: int = 0,
    ) -> list[dict[str, Any]]:
        """
        Get top players for a specific stat (from JSONB).

        Args:
            sport_id: Sport identifier
            season_year: Season year
            stat_name: Stat key within the JSONB stats column
            limit: Number of results
            position: Optional position filter
            league_id: League filter (0 for NBA/NFL, >0 for football)

        Returns:
            List of player stats ranked by the stat
        """
        conditions = [
            "s.sport = %s",
            "s.season = %s",
            "s.league_id = %s",
            f"(s.stats->>'{stat_name}') IS NOT NULL",
        ]
        params: list[Any] = [sport_id, season_year, league_id]

        if position:
            conditions.append("p.position = %s")
            params.append(position)

        params.append(limit)

        query = f"""
            SELECT
                p.id as player_id,
                p.name,
                p.position,
                t.name as team_name,
                (s.stats->>'{stat_name}')::NUMERIC as stat_value
            FROM {PLAYER_STATS_TABLE} s
            JOIN {PLAYERS_TABLE} p ON s.player_id = p.id AND s.sport = p.sport
            LEFT JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
            WHERE {" AND ".join(conditions)}
            ORDER BY (s.stats->>'{stat_name}')::NUMERIC DESC
            LIMIT %s
        """

        rows = self.db.fetchall(query, tuple(params))

        return [{**dict(row), "rank": i + 1} for i, row in enumerate(rows)]
