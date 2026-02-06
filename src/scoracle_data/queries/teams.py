"""
Team-related queries for stats database.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Optional

from ..core.types import TEAM_PROFILE_TABLES, TEAM_STATS_TABLES

if TYPE_CHECKING:
    from ..connection import StatsDB


class TeamQueries:
    """Query utilities for team statistics."""

    def __init__(self, db: "StatsDB"):
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
        team = self.db.get_team(team_id, sport_id)
        if not team:
            return None

        stats = self.db.get_team_stats(team_id, sport_id, season_year)
        percentiles = self.db.get_percentiles("team", team_id, sport_id, season_year)

        return {
            "team": dict(team),
            "stats": stats,
            "percentiles": percentiles,
        }

    def get_standings(
        self,
        sport_id: str,
        season_year: int,
        league_id: Optional[int] = None,
        conference: Optional[str] = None,
        division: Optional[str] = None,
    ) -> list[dict[str, Any]]:
        """
        Get team standings.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            league_id: League filter (for FOOTBALL)
            conference: Conference filter (for NBA/NFL)
            division: Division filter

        Returns:
            List of teams with standings info
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        stats_table = TEAM_STATS_TABLES.get(sport_id)
        team_profile_table = TEAM_PROFILE_TABLES.get(sport_id)
        
        if not stats_table or not team_profile_table:
            return []

        # Build query based on sport (using sport-specific profile tables)
        if sport_id == "FOOTBALL":
            conditions = ["s.season_id = %s"]
            params: list[Any] = [season_id]

            if league_id:
                conditions.append("s.league_id = %s")
                params.append(league_id)

            query = f"""
                SELECT
                    t.id,
                    t.name,
                    t.logo_url,
                    l.name as league_name,
                    s.matches_played as games_played,
                    s.wins,
                    s.draws,
                    s.losses,
                    s.points,
                    s.goals_for,
                    s.goals_against,
                    s.goal_difference,
                    s.league_position as rank
                FROM {stats_table} s
                JOIN {team_profile_table} t ON s.team_id = t.id
                LEFT JOIN leagues l ON s.league_id = l.id
                WHERE {' AND '.join(conditions)}
                ORDER BY s.points DESC, s.goal_difference DESC, s.goals_for DESC
            """
        else:
            # NBA/NFL (no sport_id filter needed - tables are sport-specific)
            conditions = ["s.season_id = %s"]
            params = [season_id]

            if conference:
                conditions.append("t.conference = %s")
                params.append(conference)

            if division:
                conditions.append("t.division = %s")
                params.append(division)

            query = f"""
                SELECT
                    t.id,
                    t.name,
                    t.logo_url,
                    t.conference,
                    t.division,
                    s.games_played,
                    s.wins,
                    s.losses,
                    {'s.ties,' if sport_id == 'NFL' else ''}
                    s.win_pct,
                    s.points_per_game,
                    s.opponent_ppg,
                    s.point_differential
                FROM {stats_table} s
                JOIN {team_profile_table} t ON s.team_id = t.id
                WHERE {' AND '.join(conditions)}
                ORDER BY s.win_pct DESC, s.point_differential DESC
            """

        rows = self.db.fetchall(query, tuple(params))

        # Add rank
        return [
            {**dict(row), "rank": i + 1}
            for i, row in enumerate(rows)
        ]

    def get_stat_rankings(
        self,
        sport_id: str,
        season_year: int,
        stat_name: str,
        ascending: bool = False,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """
        Get teams ranked by a specific stat.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            stat_name: Stat to rank by
            ascending: If True, lower values rank higher
            league_id: League filter (for FOOTBALL)

        Returns:
            List of teams ranked by the stat
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        stats_table = TEAM_STATS_TABLES.get(sport_id)
        team_profile_table = TEAM_PROFILE_TABLES.get(sport_id)
        
        if not stats_table or not team_profile_table:
            return []

        order = "ASC" if ascending else "DESC"

        conditions = ["s.season_id = %s", f"s.{stat_name} IS NOT NULL"]
        params: list[Any] = [season_id]

        if league_id:
            conditions.append("s.league_id = %s")
            params.append(league_id)

        query = f"""
            SELECT
                t.id,
                t.name,
                t.logo_url,
                s.{stat_name} as stat_value
            FROM {stats_table} s
            JOIN {team_profile_table} t ON s.team_id = t.id
            WHERE {' AND '.join(conditions)}
            ORDER BY s.{stat_name} {order}
        """

        rows = self.db.fetchall(query, tuple(params))

        return [
            {**dict(row), "rank": i + 1}
            for i, row in enumerate(rows)
        ]

    def compare_teams(
        self,
        team_ids: list[int],
        sport_id: str,
        season_year: int,
    ) -> list[dict[str, Any]]:
        """
        Compare multiple teams.

        Args:
            team_ids: List of team IDs to compare
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            List of team profiles for comparison
        """
        profiles = []
        for team_id in team_ids:
            profile = self.get_team_profile(team_id, sport_id, season_year)
            if profile:
                profiles.append(profile)

        return profiles
