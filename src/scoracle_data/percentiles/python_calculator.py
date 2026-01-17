"""
Pure Python percentile calculator - database agnostic.

All business logic lives in Python, making the implementation:
- Portable across any database backend
- Testable with mock data
- Version controlled with the codebase

Methodology:
- NBA: Per-36 minute stats, compared by position group
- FOOTBALL: Per-90 minute stats, compared across Top 5 Leagues by position
- NFL: Position-specific stat categories
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Optional

from .config import (
    INVERSE_STATS,
    get_min_sample_size,
)

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


# =============================================================================
# NORMALIZED STAT COMPUTATION RULES
# =============================================================================

# NBA: Per-36 minute stats
# Formula: (total_stat / minutes_total) * 36
# Minimum: 100 minutes to avoid small sample noise
NBA_PER36_STATS = {
    "points_per_36": {"raw": "points_total", "minutes": "minutes_total", "min_minutes": 100},
    "rebounds_per_36": {"raw": "total_rebounds", "minutes": "minutes_total", "min_minutes": 100},
    "assists_per_36": {"raw": "assists", "minutes": "minutes_total", "min_minutes": 100},
    "steals_per_36": {"raw": "steals", "minutes": "minutes_total", "min_minutes": 100},
    "blocks_per_36": {"raw": "blocks", "minutes": "minutes_total", "min_minutes": 100},
    "turnovers_per_36": {"raw": "turnovers", "minutes": "minutes_total", "min_minutes": 100},
}

# FOOTBALL: Per-90 minute stats (already in database, but we can recompute if needed)
# These are already computed by the seeder, so we just read them
FOOTBALL_PER90_STATS = {
    "goals_per_90": {"raw": "goals", "minutes": "minutes_played", "min_minutes": 450},
    "assists_per_90": {"raw": "assists", "minutes": "minutes_played", "min_minutes": 450},
    "key_passes_per_90": {"raw": "key_passes", "minutes": "minutes_played", "min_minutes": 450},
    "shots_per_90": {"raw": "shots_total", "minutes": "minutes_played", "min_minutes": 450},
    "tackles_per_90": {"raw": "tackles", "minutes": "minutes_played", "min_minutes": 450},
    "interceptions_per_90": {"raw": "interceptions", "minutes": "minutes_played", "min_minutes": 450},
}

# Top 5 European Leagues for FOOTBALL comparison
TOP_5_LEAGUE_IDS = {39, 140, 78, 135, 61}  # EPL, La Liga, Bundesliga, Serie A, Ligue 1


@dataclass
class PercentileResult:
    """Result of a percentile calculation."""
    entity_type: str
    entity_id: int
    sport_id: str
    season_id: int
    stat_category: str
    stat_value: float
    percentile: float
    rank: int
    sample_size: int
    comparison_group: str


class PythonPercentileCalculator:
    """
    Pure Python percentile calculator.

    Fetches raw data from database, computes all derived stats and percentiles
    in Python, then writes results back to the database.
    
    Key design principle: ALL numeric stats are calculated, not just a curated subset.
    The frontend decides which stats to display. This keeps the backend DB-agnostic
    and ensures all data is available for analysis.
    """

    STATS_TABLE_MAP = {
        ("NBA", "player"): "nba_player_stats",
        ("NBA", "team"): "nba_team_stats",
        ("NFL", "player"): "nfl_player_stats",
        ("NFL", "team"): "nfl_team_stats",
        ("FOOTBALL", "player"): "football_player_stats",
        ("FOOTBALL", "team"): "football_team_stats",
    }
    
    # Columns to exclude from percentile calculation (internal IDs and metadata)
    EXCLUDED_COLUMNS = {
        "id", "player_id", "team_id", "season_id", "league_id", "updated_at"
    }

    def __init__(self, db: Any):
        """
        Initialize the calculator.

        Args:
            db: Database connection (any backend that supports execute/fetchall)
        """
        self.db = db
        self._column_cache: dict[tuple[str, str], list[str]] = {}

    # =========================================================================
    # Dynamic Column Discovery
    # =========================================================================

    def _get_all_numeric_stat_columns(self, sport_id: str, entity_type: str) -> list[str]:
        """
        Dynamically discover ALL numeric stat columns from the stats table.
        
        This allows percentiles to be calculated for ALL stats without
        maintaining a curated list. The frontend decides what to display.
        
        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            entity_type: 'player' or 'team'
            
        Returns:
            List of all numeric column names (excluding internal IDs)
        """
        cache_key = (sport_id, entity_type)
        if cache_key in self._column_cache:
            return self._column_cache[cache_key]
        
        table = self.STATS_TABLE_MAP.get((sport_id, entity_type))
        if not table:
            return []
        
        # Query information_schema for numeric columns
        result = self.db.fetchall("""
            SELECT column_name 
            FROM information_schema.columns 
            WHERE table_name = %s 
              AND data_type IN ('integer', 'real', 'numeric', 'double precision', 'smallint', 'bigint')
            ORDER BY ordinal_position
        """, (table,))
        
        # Filter out excluded columns (IDs, metadata)
        columns = [
            r["column_name"] for r in result 
            if r["column_name"] not in self.EXCLUDED_COLUMNS
        ]
        
        # Cache the result
        self._column_cache[cache_key] = columns
        
        logger.debug(
            "Discovered %d numeric columns for %s %s: %s",
            len(columns), sport_id, entity_type, columns[:5]  # Log first 5
        )
        
        return columns

    # =========================================================================
    # Core Percentile Calculation (Pure Python)
    # =========================================================================

    def calculate_percentile(
        self,
        values: list[float],
        target_value: float,
        inverse: bool = False,
    ) -> tuple[float, int]:
        """
        Calculate percentile rank and rank for a value within a distribution.

        Uses the "percentage of values below" method:
        - Best performer gets 100%
        - Worst performer gets 0%

        Args:
            values: All values in the comparison group
            target_value: The value to rank
            inverse: If True, lower values are better (e.g., turnovers)

        Returns:
            Tuple of (percentile, rank)
        """
        if not values:
            return 0.0, 0

        n = len(values)

        if inverse:
            # Lower is better: count how many are WORSE (higher)
            below = sum(1 for v in values if v > target_value)
            sorted_values = sorted(values)  # Ascending for rank
        else:
            # Higher is better: count how many are WORSE (lower)
            below = sum(1 for v in values if v < target_value)
            sorted_values = sorted(values, reverse=True)  # Descending for rank

        # Percentile: percentage of values that are worse
        percentile = round((below / n) * 100, 1)

        # Rank: position in sorted list (1 = best)
        rank = 1
        for i, v in enumerate(sorted_values, 1):
            if v == target_value:
                rank = i
                break

        return percentile, rank

    def compute_normalized_stat(
        self,
        stat_name: str,
        row: dict[str, Any],
        sport_id: str,
    ) -> Optional[float]:
        """
        Compute a normalized stat (per-36 or per-90) from raw data.

        Args:
            stat_name: Name of the normalized stat (e.g., "points_per_36")
            row: Raw stat row from database
            sport_id: Sport identifier

        Returns:
            Computed value or None if below minimum threshold
        """
        if sport_id == "NBA" and stat_name in NBA_PER36_STATS:
            config = NBA_PER36_STATS[stat_name]
            raw_value = row.get(config["raw"])
            minutes = row.get(config["minutes"])
            min_minutes = config["min_minutes"]

            if raw_value is None or minutes is None or minutes < min_minutes:
                return None

            return round((raw_value / minutes) * 36, 1)

        elif sport_id == "FOOTBALL" and stat_name in FOOTBALL_PER90_STATS:
            # FOOTBALL per-90 stats are pre-computed in DB, but we can recompute
            # First try to read pre-computed value
            if stat_name in row and row[stat_name] is not None:
                return row[stat_name]

            # Otherwise compute from raw
            config = FOOTBALL_PER90_STATS[stat_name]
            raw_value = row.get(config["raw"])
            minutes = row.get(config["minutes"])
            min_minutes = config["min_minutes"]

            if raw_value is None or minutes is None or minutes < min_minutes:
                return None

            return round((raw_value / minutes) * 90, 2)

        # Not a normalized stat, return raw value
        return row.get(stat_name)

    # =========================================================================
    # Data Fetching
    # =========================================================================

    def _fetch_player_stats(
        self,
        sport_id: str,
        season_id: int,
    ) -> list[dict[str, Any]]:
        """Fetch all player stats with position info."""
        table = self.STATS_TABLE_MAP.get((sport_id, "player"))
        if not table:
            return []

        # FOOTBALL: Filter to Top 5 Leagues
        if sport_id == "FOOTBALL":
            query = f"""
                SELECT DISTINCT ON (s.player_id)
                    s.*, p.id as player_id, p.position_group, p.full_name, s.league_id
                FROM {table} s
                JOIN players p ON s.player_id = p.id AND p.sport_id = %s
                JOIN leagues l ON s.league_id = l.id
                WHERE s.season_id = %s
                  AND l.include_in_percentiles = true
                ORDER BY s.player_id, s.id DESC
            """
        else:
            query = f"""
                SELECT DISTINCT ON (s.player_id)
                    s.*, p.id as player_id, p.position_group, p.full_name
                FROM {table} s
                JOIN players p ON s.player_id = p.id AND p.sport_id = %s
                WHERE s.season_id = %s
                ORDER BY s.player_id, s.id DESC
            """

        return self.db.fetchall(query, (sport_id, season_id))

    def _fetch_team_stats(
        self,
        sport_id: str,
        season_id: int,
    ) -> list[dict[str, Any]]:
        """Fetch all team stats."""
        table = self.STATS_TABLE_MAP.get((sport_id, "team"))
        if not table:
            return []

        # FOOTBALL: Filter to Top 5 Leagues
        if sport_id == "FOOTBALL":
            query = f"""
                SELECT s.*, t.id as team_id, t.name as team_name, s.league_id
                FROM {table} s
                JOIN teams t ON s.team_id = t.id AND t.sport_id = %s
                JOIN leagues l ON s.league_id = l.id
                WHERE s.season_id = %s
                  AND l.include_in_percentiles = true
            """
        else:
            query = f"""
                SELECT s.*, t.id as team_id, t.name as team_name
                FROM {table} s
                JOIN teams t ON s.team_id = t.id AND t.sport_id = %s
                WHERE s.season_id = %s
            """

        return self.db.fetchall(query, (sport_id, season_id))

    def _get_season_year(self, season_id: int) -> int:
        """Get the year for a season ID."""
        result = self.db.fetchone(
            "SELECT season_year FROM seasons WHERE id = %s",
            (season_id,)
        )
        return result["season_year"] if result else season_id

    # =========================================================================
    # Player Percentile Calculation
    # =========================================================================

    def calculate_all_player_percentiles(
        self,
        sport_id: str,
        season_id: int,
    ) -> int:
        """
        Calculate percentiles for ALL players in a sport/season.
        
        Calculates percentiles for ALL numeric stat columns (not just a curated subset).
        Filters out zero and null values. Groups players by position for fair comparison.

        Args:
            sport_id: Sport identifier
            season_id: Season ID

        Returns:
            Number of percentile records created
        """
        stats = self._fetch_player_stats(sport_id, season_id)
        if not stats:
            logger.warning("No player stats found for %s season %d", sport_id, season_id)
            return 0

        # Get ALL numeric columns dynamically (not curated subset)
        categories = self._get_all_numeric_stat_columns(sport_id, "player")
        if not categories:
            logger.warning("No numeric columns found for %s player stats", sport_id)
            return 0
            
        min_sample = get_min_sample_size(sport_id, "player")
        season_year = self._get_season_year(season_id)

        results: list[PercentileResult] = []

        for stat_name in categories:
            is_inverse = stat_name in INVERSE_STATS

            # Group players by position for comparison
            position_groups: dict[str, list[tuple[int, float]]] = {}

            for row in stats:
                position = row.get("position_group") or "Unknown"

                # Compute or fetch the stat value
                value = self.compute_normalized_stat(stat_name, row, sport_id)
                
                # Filter out None and zero values
                if value is None or value == 0:
                    continue

                if position not in position_groups:
                    position_groups[position] = []
                position_groups[position].append((row["player_id"], value))

            # Calculate percentiles within each position group
            for position, player_values in position_groups.items():
                if len(player_values) < min_sample:
                    logger.debug(
                        "Skipping %s/%s: only %d players (need %d)",
                        stat_name, position, len(player_values), min_sample
                    )
                    continue

                all_values = [v for _, v in player_values]

                for player_id, value in player_values:
                    percentile, rank = self.calculate_percentile(
                        all_values, value, inverse=is_inverse
                    )

                    # Build comparison group name
                    if sport_id == "FOOTBALL":
                        comparison_group = f"Top 5 Leagues {position} {season_year}"
                    else:
                        comparison_group = f"{position} {season_year}"

                    results.append(PercentileResult(
                        entity_type="player",
                        entity_id=player_id,
                        sport_id=sport_id,
                        season_id=season_id,
                        stat_category=stat_name,
                        stat_value=value,
                        percentile=percentile,
                        rank=rank,
                        sample_size=len(all_values),
                        comparison_group=comparison_group,
                    ))

        # Batch write results
        self._write_percentile_results(results)

        logger.info(
            "Calculated %d player percentile records for %s (%d stat categories)",
            len(results), sport_id, len(categories)
        )
        return len(results)

    # =========================================================================
    # Team Percentile Calculation
    # =========================================================================

    def calculate_all_team_percentiles(
        self,
        sport_id: str,
        season_id: int,
    ) -> int:
        """
        Calculate percentiles for ALL teams in a sport/season.
        
        Calculates percentiles for ALL numeric stat columns (not just a curated subset).
        Filters out zero and null values.

        For FOOTBALL, teams are compared across all Top 5 Leagues.
        For NBA/NFL, teams are compared within the league.

        Args:
            sport_id: Sport identifier
            season_id: Season ID

        Returns:
            Number of percentile records created
        """
        stats = self._fetch_team_stats(sport_id, season_id)
        if not stats:
            logger.warning("No team stats found for %s season %d", sport_id, season_id)
            return 0

        # Get ALL numeric columns dynamically (not curated subset)
        categories = self._get_all_numeric_stat_columns(sport_id, "team")
        if not categories:
            logger.warning("No numeric columns found for %s team stats", sport_id)
            return 0
            
        min_sample = get_min_sample_size(sport_id, "team")
        season_year = self._get_season_year(season_id)

        results: list[PercentileResult] = []

        for stat_name in categories:
            is_inverse = stat_name in INVERSE_STATS

            # Collect all team values for this stat
            team_values: list[tuple[int, float]] = []

            for row in stats:
                value = row.get(stat_name)
                
                # Filter out None and zero values
                if value is None or value == 0:
                    continue
                    
                team_values.append((row["team_id"], value))

            if len(team_values) < min_sample:
                logger.debug(
                    "Skipping team %s: only %d teams (need %d)",
                    stat_name, len(team_values), min_sample
                )
                continue

            all_values = [v for _, v in team_values]

            for team_id, value in team_values:
                percentile, rank = self.calculate_percentile(
                    all_values, value, inverse=is_inverse
                )

                # Build comparison group name
                if sport_id == "FOOTBALL":
                    comparison_group = f"Top 5 Leagues {season_year}"
                else:
                    comparison_group = f"{sport_id} Teams {season_year}"

                results.append(PercentileResult(
                    entity_type="team",
                    entity_id=team_id,
                    sport_id=sport_id,
                    season_id=season_id,
                    stat_category=stat_name,
                    stat_value=value,
                    percentile=percentile,
                    rank=rank,
                    sample_size=len(all_values),
                    comparison_group=comparison_group,
                ))

        # Batch write results
        self._write_percentile_results(results)

        logger.info(
            "Calculated %d team percentile records for %s (%d stat categories)",
            len(results), sport_id, len(categories)
        )
        return len(results)

    # =========================================================================
    # Database Writing
    # =========================================================================

    def _write_percentile_results(self, results: list[PercentileResult]) -> None:
        """Write percentile results to the cache table using batch inserts."""
        if not results:
            return

        from datetime import datetime, timezone

        now = datetime.now(timezone.utc)

        # Build batch of tuples for executemany
        values = [
            (
                r.entity_type,
                r.entity_id,
                r.sport_id,
                r.season_id,
                r.stat_category,
                r.stat_value,
                r.percentile,
                r.rank,
                r.sample_size,
                r.comparison_group,
                now,
            )
            for r in results
        ]

        # Use executemany for batch insert (much faster than individual inserts)
        # Process in chunks of 1000 to avoid memory issues
        chunk_size = 1000
        query = """
            INSERT INTO percentile_cache (
                entity_type, entity_id, sport_id, season_id, stat_category,
                stat_value, percentile, rank, sample_size, comparison_group,
                calculated_at
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (entity_type, entity_id, sport_id, season_id, stat_category)
            DO UPDATE SET
                stat_value = EXCLUDED.stat_value,
                percentile = EXCLUDED.percentile,
                rank = EXCLUDED.rank,
                sample_size = EXCLUDED.sample_size,
                comparison_group = EXCLUDED.comparison_group,
                calculated_at = EXCLUDED.calculated_at
        """

        for i in range(0, len(values), chunk_size):
            chunk = values[i:i + chunk_size]
            self.db.executemany(query, chunk)

    # =========================================================================
    # Main Entry Points
    # =========================================================================

    def recalculate_all_percentiles(
        self,
        sport_id: str,
        season_year: int,
    ) -> dict[str, int]:
        """
        Recalculate all percentiles for a sport and season.

        This is the main entry point for batch recalculation.

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Season year (e.g., 2025)

        Returns:
            Dict with player and team counts
        """
        # Get season ID from year
        result = self.db.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year)
        )
        if not result:
            logger.warning("No season found for %s %d", sport_id, season_year)
            return {"players": 0, "teams": 0}

        season_id = result["id"]

        # Clear existing cache for this sport/season
        self.db.execute(
            "DELETE FROM percentile_cache WHERE sport_id = %s AND season_id = %s",
            (sport_id, season_id),
        )

        # Calculate player percentiles
        player_count = self.calculate_all_player_percentiles(sport_id, season_id)

        # Calculate team percentiles
        team_count = self.calculate_all_team_percentiles(sport_id, season_id)

        logger.info(
            "Recalculated percentiles for %s %d: %d player records, %d team records",
            sport_id, season_year, player_count, team_count
        )

        return {"players": player_count, "teams": team_count}

    # =========================================================================
    # Archive Functionality
    # =========================================================================

    def archive_season(
        self,
        sport_id: str,
        season_year: int,
        is_final: bool = True,
    ) -> int:
        """
        Archive current percentiles to the percentile_archive table.

        Use this at end of season to preserve historical data.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            is_final: True if this is the final end-of-season snapshot

        Returns:
            Number of records archived
        """
        result = self.db.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year)
        )
        if not result:
            logger.warning("No season found for %s %d", sport_id, season_year)
            return 0

        season_id = result["id"]

        # Copy from cache to archive
        self.db.execute(
            """
            INSERT INTO percentile_archive (
                entity_type, entity_id, sport_id, season_id, stat_category,
                stat_value, percentile, rank, sample_size, comparison_group,
                calculated_at, archived_at, is_final
            )
            SELECT
                entity_type, entity_id, sport_id, season_id, stat_category,
                stat_value, percentile, rank, sample_size, comparison_group,
                calculated_at, %s, %s
            FROM percentile_cache
            WHERE sport_id = %s AND season_id = %s
            ON CONFLICT (entity_type, entity_id, sport_id, season_id, stat_category, archived_at)
            DO NOTHING
            """,
            (int(time.time()), is_final, sport_id, season_id),
        )

        # Get count of archived records
        count_result = self.db.fetchone(
            """
            SELECT COUNT(*) as count FROM percentile_archive
            WHERE sport_id = %s AND season_id = %s AND archived_at = (
                SELECT MAX(archived_at) FROM percentile_archive
                WHERE sport_id = %s AND season_id = %s
            )
            """,
            (sport_id, season_id, sport_id, season_id),
        )

        count = count_result["count"] if count_result else 0
        logger.info(
            "Archived %d percentile records for %s %d (final=%s)",
            count, sport_id, season_year, is_final
        )

        return count
