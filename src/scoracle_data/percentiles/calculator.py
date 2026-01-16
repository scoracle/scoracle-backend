"""
Percentile calculation engine for sports statistics.

DEPRECATED: Use PythonPercentileCalculator instead.
This module was the original SQLite-based calculator. It has been
superseded by PythonPercentileCalculator which provides:
- Database-agnostic operation
- Top 5 Leagues comparison for FOOTBALL
- Per-36/per-90 normalized stat computation
- Archive functionality for historical preservation
"""

from __future__ import annotations

import logging
import time
from typing import TYPE_CHECKING, Any, Optional

from .config import get_min_sample_size, get_stat_categories, is_inverse_stat

if TYPE_CHECKING:
    from ..connection import StatsDB

logger = logging.getLogger(__name__)


class PercentileCalculator:
    """
    Calculates and caches percentile ranks for player and team statistics.

    Percentiles are calculated within appropriate comparison groups:
    - Players: By position within the same sport and season
    - Teams: Within the same league/conference and season
    """

    def __init__(self, db: "StatsDB"):
        """
        Initialize the calculator.

        Args:
            db: Stats database connection
        """
        self.db = db

    # =========================================================================
    # Main Calculation Methods
    # =========================================================================

    def calculate_percentile(
        self,
        values: list[float],
        target_value: float,
        inverse: bool = False,
    ) -> float:
        """
        Calculate percentile rank for a value within a distribution.

        Args:
            values: List of all values in the comparison group
            target_value: The value to calculate percentile for
            inverse: If True, lower values get higher percentiles

        Returns:
            Percentile rank (0-100)
        """
        if not values:
            return 0.0

        sorted_values = sorted(values, reverse=inverse)
        n = len(sorted_values)

        # Count values below target
        if inverse:
            below = sum(1 for v in sorted_values if v > target_value)
        else:
            below = sum(1 for v in sorted_values if v < target_value)

        # Percentile formula: (count below / total) * 100
        percentile = (below / n) * 100

        return round(percentile, 1)

    def calculate_player_percentiles(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
        position_group: Optional[str] = None,
        force_recalculate: bool = False,
    ) -> list[dict[str, Any]]:
        """
        Calculate all percentiles for a player.

        Args:
            player_id: Player ID
            sport_id: Sport identifier
            season_year: Season year
            position_group: Position group for comparison (optional)
            force_recalculate: If True, recalculate even if cached

        Returns:
            List of percentile results
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            logger.warning("No season found for %s %d", sport_id, season_year)
            return []

        # Check cache unless forcing recalculation
        if not force_recalculate:
            cached = self.db.get_percentiles("player", player_id, sport_id, season_year)
            if cached:
                return cached

        # Get player info
        player = self.db.get_player(player_id, sport_id)
        if not player:
            logger.warning("Player %d not found in %s", player_id, sport_id)
            return []

        # Use player's position group if not specified
        if not position_group:
            position_group = player.get("position_group")

        # Get player's stats
        player_stats = self.db.get_player_stats(player_id, sport_id, season_year)
        if not player_stats:
            logger.warning("No stats found for player %d in %s %d", player_id, sport_id, season_year)
            return []

        # Get stat categories for this player
        categories = get_stat_categories(sport_id, "player", position_group)

        # Calculate percentiles for each category
        results = []
        for stat_name in categories:
            if stat_name not in player_stats:
                continue

            stat_value = player_stats[stat_name]
            if stat_value is None:
                continue

            # Get all values for this stat in the comparison group
            comparison_values = self._get_player_stat_distribution(
                sport_id,
                season_id,
                stat_name,
                position_group,
            )

            if len(comparison_values) < get_min_sample_size(sport_id, "player"):
                logger.debug(
                    "Insufficient sample size for %s (got %d)",
                    stat_name,
                    len(comparison_values),
                )
                continue

            # Calculate percentile
            percentile = self.calculate_percentile(
                comparison_values,
                float(stat_value),
                inverse=is_inverse_stat(stat_name),
            )

            # Calculate rank
            rank = self._calculate_rank(comparison_values, float(stat_value), is_inverse_stat(stat_name))

            result = {
                "stat_category": stat_name,
                "stat_value": float(stat_value),
                "percentile": percentile,
                "rank": rank,
                "sample_size": len(comparison_values),
                "comparison_group": self._build_comparison_group_name(
                    sport_id, "player", position_group, season_year
                ),
            }
            results.append(result)

            # Cache the result
            self._cache_percentile(
                "player",
                player_id,
                sport_id,
                season_id,
                result,
            )

        return results

    def calculate_team_percentiles(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
        league_id: Optional[int] = None,
        force_recalculate: bool = False,
    ) -> list[dict[str, Any]]:
        """
        Calculate all percentiles for a team.

        Args:
            team_id: Team ID
            sport_id: Sport identifier
            season_year: Season year
            league_id: League ID for comparison (optional, for FOOTBALL)
            force_recalculate: If True, recalculate even if cached

        Returns:
            List of percentile results
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        # Check cache
        if not force_recalculate:
            cached = self.db.get_percentiles("team", team_id, sport_id, season_year)
            if cached:
                return cached

        # Get team stats
        team_stats = self.db.get_team_stats(team_id, sport_id, season_year)
        if not team_stats:
            return []

        # Get league_id from team if not provided
        if not league_id and sport_id == "FOOTBALL":
            team = self.db.get_team(team_id, sport_id)
            league_id = team.get("league_id") if team else None

        # Get stat categories
        categories = get_stat_categories(sport_id, "team")

        results = []
        for stat_name in categories:
            if stat_name not in team_stats:
                continue

            stat_value = team_stats[stat_name]
            if stat_value is None:
                continue

            # Get distribution
            comparison_values = self._get_team_stat_distribution(
                sport_id,
                season_id,
                stat_name,
                league_id,
            )

            if len(comparison_values) < get_min_sample_size(sport_id, "team"):
                continue

            percentile = self.calculate_percentile(
                comparison_values,
                float(stat_value),
                inverse=is_inverse_stat(stat_name),
            )

            rank = self._calculate_rank(comparison_values, float(stat_value), is_inverse_stat(stat_name))

            result = {
                "stat_category": stat_name,
                "stat_value": float(stat_value),
                "percentile": percentile,
                "rank": rank,
                "sample_size": len(comparison_values),
                "comparison_group": self._build_comparison_group_name(
                    sport_id, "team", None, season_year, league_id
                ),
            }
            results.append(result)

            self._cache_percentile("team", team_id, sport_id, season_id, result)

        return results

    # =========================================================================
    # Batch Calculation
    # =========================================================================

    def recalculate_all_percentiles(
        self,
        sport_id: str,
        season_year: int,
    ) -> dict[str, int]:
        """
        Recalculate all percentiles for a sport and season.

        Args:
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            Summary of calculations performed
        """
        season_id = self.db.get_season_id(sport_id, season_year)
        if not season_id:
            return {"players": 0, "teams": 0}

        # Clear existing cache for this sport/season
        self.db.execute(
            "DELETE FROM percentile_cache WHERE sport_id = ? AND season_id = ?",
            (sport_id, season_id),
        )

        # Recalculate for all players
        players = self.db.fetchall(
            "SELECT id, position_group FROM players WHERE sport_id = ?",
            (sport_id,),
        )

        player_count = 0
        for player in players:
            try:
                results = self.calculate_player_percentiles(
                    player["id"],
                    sport_id,
                    season_year,
                    player.get("position_group"),
                    force_recalculate=True,
                )
                if results:
                    player_count += 1
            except Exception as e:
                logger.warning("Failed to calculate percentiles for player %d: %s", player["id"], e)

        # Recalculate for all teams
        teams = self.db.fetchall(
            "SELECT id, league_id FROM teams WHERE sport_id = ?",
            (sport_id,),
        )

        team_count = 0
        for team in teams:
            try:
                results = self.calculate_team_percentiles(
                    team["id"],
                    sport_id,
                    season_year,
                    team.get("league_id"),
                    force_recalculate=True,
                )
                if results:
                    team_count += 1
            except Exception as e:
                logger.warning("Failed to calculate percentiles for team %d: %s", team["id"], e)

        logger.info(
            "Recalculated percentiles for %s %d: %d players, %d teams",
            sport_id,
            season_year,
            player_count,
            team_count,
        )

        return {"players": player_count, "teams": team_count}

    # =========================================================================
    # Distribution Queries
    # =========================================================================

    def _get_player_stat_distribution(
        self,
        sport_id: str,
        season_id: int,
        stat_name: str,
        position_group: Optional[str] = None,
    ) -> list[float]:
        """Get all values for a stat within the comparison group."""
        # Determine the correct table
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": self._get_nfl_stat_table(stat_name),
            "FOOTBALL": "football_player_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return []

        # Build query with optional position filter
        if position_group:
            query = f"""
                SELECT s.{stat_name}
                FROM {table} s
                JOIN players p ON s.player_id = p.id
                WHERE s.season_id = ?
                  AND p.sport_id = ?
                  AND p.position_group = ?
                  AND s.{stat_name} IS NOT NULL
            """
            params = (season_id, sport_id, position_group)
        else:
            query = f"""
                SELECT s.{stat_name}
                FROM {table} s
                JOIN players p ON s.player_id = p.id
                WHERE s.season_id = ?
                  AND p.sport_id = ?
                  AND s.{stat_name} IS NOT NULL
            """
            params = (season_id, sport_id)

        try:
            rows = self.db.fetchall(query, params)
            return [float(row[stat_name]) for row in rows if row[stat_name] is not None]
        except Exception as e:
            logger.warning("Failed to get distribution for %s: %s", stat_name, e)
            return []

    def _get_team_stat_distribution(
        self,
        sport_id: str,
        season_id: int,
        stat_name: str,
        league_id: Optional[int] = None,
    ) -> list[float]:
        """Get all values for a team stat within the comparison group."""
        table_map = {
            "NBA": "nba_team_stats",
            "NFL": "nfl_team_stats",
            "FOOTBALL": "football_team_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return []

        # Build query with optional league filter
        if league_id and sport_id == "FOOTBALL":
            query = f"""
                SELECT {stat_name}
                FROM {table}
                WHERE season_id = ? AND league_id = ? AND {stat_name} IS NOT NULL
            """
            params = (season_id, league_id)
        else:
            query = f"""
                SELECT {stat_name}
                FROM {table}
                WHERE season_id = ? AND {stat_name} IS NOT NULL
            """
            params = (season_id,)

        try:
            rows = self.db.fetchall(query, params)
            return [float(row[stat_name]) for row in rows if row[stat_name] is not None]
        except Exception as e:
            logger.warning("Failed to get team distribution for %s: %s", stat_name, e)
            return []

    def _get_nfl_stat_table(self, stat_name: str) -> str:
        """Determine which NFL table contains a stat."""
        passing_stats = {"pass_yards", "pass_touchdowns", "passer_rating", "completion_pct", "yards_per_attempt"}
        rushing_stats = {"rush_yards", "rush_touchdowns", "yards_per_carry"}
        receiving_stats = {"receiving_yards", "receiving_touchdowns", "receptions", "yards_per_reception"}
        defense_stats = {"tackles_total", "sacks", "interceptions", "passes_defended"}

        if stat_name in passing_stats:
            return "nfl_player_passing"
        elif stat_name in rushing_stats:
            return "nfl_player_rushing"
        elif stat_name in receiving_stats:
            return "nfl_player_receiving"
        elif stat_name in defense_stats:
            return "nfl_player_defense"

        return "nfl_player_passing"  # Default fallback

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _calculate_rank(
        self,
        values: list[float],
        target_value: float,
        inverse: bool = False,
    ) -> int:
        """Calculate rank within the distribution."""
        sorted_values = sorted(values, reverse=not inverse)

        for i, v in enumerate(sorted_values, 1):
            if v == target_value:
                return i

        # If exact value not found, find position
        if inverse:
            return sum(1 for v in values if v < target_value) + 1
        else:
            return sum(1 for v in values if v > target_value) + 1

    def _build_comparison_group_name(
        self,
        sport_id: str,
        entity_type: str,
        position_group: Optional[str],
        season_year: int,
        league_id: Optional[int] = None,
    ) -> str:
        """Build a human-readable comparison group name."""
        parts = [sport_id]

        if entity_type == "player" and position_group:
            parts.append(position_group)
        elif entity_type == "team":
            parts.append("Teams")

        if league_id:
            # Look up league name
            league = self.db.fetchone(
                "SELECT name FROM leagues WHERE id = ?",
                (league_id,),
            )
            if league:
                parts.append(league["name"])

        parts.append(str(season_year))

        return " ".join(parts)

    def _cache_percentile(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        season_id: int,
        result: dict[str, Any],
    ) -> None:
        """Cache a percentile calculation result."""
        self.db.execute(
            """
            INSERT INTO percentile_cache (
                entity_type, entity_id, sport_id, season_id,
                stat_category, stat_value, percentile, rank, sample_size, comparison_group,
                calculated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(entity_type, entity_id, sport_id, season_id, stat_category) DO UPDATE SET
                stat_value = excluded.stat_value,
                percentile = excluded.percentile,
                rank = excluded.rank,
                sample_size = excluded.sample_size,
                comparison_group = excluded.comparison_group,
                calculated_at = excluded.calculated_at
            """,
            (
                entity_type,
                entity_id,
                sport_id,
                season_id,
                result["stat_category"],
                result["stat_value"],
                result["percentile"],
                result["rank"],
                result["sample_size"],
                result.get("comparison_group"),
                int(time.time()),
            ),
        )
