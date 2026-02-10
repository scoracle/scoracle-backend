"""
Team-related queries for stats database.

Uses the unified teams/team_stats tables with JSONB stats.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional

from ..core.types import TEAMS_TABLE, TEAM_STATS_TABLE

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB


class TeamQueries:
    """Query utilities for team statistics."""

    def __init__(self, db: "PostgresDB"):
        self.db = db

    def get_team_profile(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete team profile with stats and percentiles.

        Args:
            team_id: Team ID
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            Dict with team info, stats, and percentiles
        """
        team = self.db.fetchone(
            f"SELECT * FROM {TEAMS_TABLE} WHERE id = %s AND sport = %s",
            (team_id, sport_id),
        )
        if not team:
            return None

        stats = self.db.fetchone(
            f"SELECT * FROM {TEAM_STATS_TABLE} WHERE team_id = %s AND sport = %s AND season = %s",
            (team_id, sport_id, season_year),
        )

        return {
            "team": dict(team),
            "stats": dict(stats) if stats else None,
            "percentiles": stats.get("percentiles") if stats else None,
        }

    def get_standings(
        self,
        sport_id: str,
        season_year: int,
        league_id: int = 0,
    ) -> list[dict[str, Any]]:
        """
        Get team standings from JSONB stats.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            league_id: League filter (0 for NBA/NFL, >0 for football)

        Returns:
            List of teams with standings info
        """
        if sport_id == "FOOTBALL":
            query = f"""
                SELECT
                    t.id,
                    t.name,
                    t.logo_url,
                    l.name as league_name,
                    s.stats
                FROM {TEAM_STATS_TABLE} s
                JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
                LEFT JOIN leagues l ON s.league_id = l.id
                WHERE s.sport = %s AND s.season = %s AND s.league_id = %s
                ORDER BY
                    (s.stats->>'points')::INTEGER DESC NULLS LAST,
                    (s.stats->>'goal_difference')::INTEGER DESC NULLS LAST,
                    (s.stats->>'goals_for')::INTEGER DESC NULLS LAST
            """
            params: tuple = (sport_id, season_year, league_id)
        else:
            # NBA/NFL
            query = f"""
                SELECT
                    t.id,
                    t.name,
                    t.logo_url,
                    t.meta,
                    s.stats
                FROM {TEAM_STATS_TABLE} s
                JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
                WHERE s.sport = %s AND s.season = %s AND s.league_id = %s
                ORDER BY
                    (s.stats->>'wins')::INTEGER DESC NULLS LAST
            """
            params = (sport_id, season_year, league_id)

        rows = self.db.fetchall(query, params)

        return [{**dict(row), "rank": i + 1} for i, row in enumerate(rows)]
