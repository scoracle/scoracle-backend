"""
Percentile calculator — thin Python orchestrator over Postgres SQL function.

The heavy lifting (percent_rank() window functions, JSONB aggregation) runs
entirely inside Postgres via the recalculate_percentiles() SQL function
defined in 001_schema.sql.

This module:
- Passes configuration (inverse stats list) from Python config to SQL
- Handles logging, timing, error reporting
- Provides the CLI entry point (recalculate_all_percentiles)
- Manages season archiving to percentile_archive table
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .config import INVERSE_STATS

logger = logging.getLogger(__name__)


class PythonPercentileCalculator:
    """
    Postgres-native percentile calculator.

    Delegates computation to a SQL function that uses percent_rank()
    window functions operating directly on JSONB stat keys.
    """

    def __init__(self, db: Any):
        self.db = db

    def recalculate_all_percentiles(
        self,
        sport_id: str,
        season_year: int,
    ) -> dict[str, int]:
        """
        Recalculate all percentiles for a sport and season.

        Calls the Postgres recalculate_percentiles() function which:
        1. Discovers JSONB stat keys dynamically
        2. Computes percent_rank() per stat, partitioned by position (players)
        3. Handles inverse stats (lower-is-better)
        4. Writes results into the percentiles JSONB column

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Season year (e.g., 2025)

        Returns:
            Dict with player and team counts
        """
        start = time.time()

        # Pass inverse stats list from Python config to SQL function
        inverse_list = list(INVERSE_STATS)

        try:
            result = self.db.fetchone(
                "SELECT * FROM recalculate_percentiles(%s, %s, %s)",
                (sport_id, season_year, inverse_list),
            )

            player_count = result["players_updated"] if result else 0
            team_count = result["teams_updated"] if result else 0

            elapsed = time.time() - start
            logger.info(
                "Recalculated percentiles for %s %d: %d players, %d teams (%.1fs)",
                sport_id,
                season_year,
                player_count,
                team_count,
                elapsed,
            )

            return {"players": player_count, "teams": team_count}

        except Exception as e:
            elapsed = time.time() - start
            logger.error(
                "Failed to recalculate percentiles for %s %d (%.1fs): %s",
                sport_id,
                season_year,
                elapsed,
                e,
            )
            raise

    def archive_season(
        self,
        sport_id: str,
        season_year: int,
        is_final: bool = True,
    ) -> int:
        """
        Archive current percentiles to the percentile_archive table.

        Uses a single INSERT ... SELECT per entity type that unpacks the
        percentiles JSONB directly in Postgres via jsonb_each_text(), avoiding
        round-tripping all rows through Python.

        Args:
            sport_id: Sport identifier
            season_year: Season year
            is_final: True if this is the final end-of-season snapshot

        Returns:
            Number of records archived
        """
        start = time.time()
        total_count = 0

        for entity_type in ("player", "team"):
            table = "player_stats" if entity_type == "player" else "team_stats"
            id_column = "player_id" if entity_type == "player" else "team_id"

            # Single SQL statement: unpack JSONB percentiles, extract metadata,
            # and insert directly into percentile_archive.
            result = self.db.fetchone(
                f"""
                WITH archived AS (
                    INSERT INTO percentile_archive (
                        entity_type, entity_id, sport, season, stat_category,
                        stat_value, percentile, rank, sample_size, comparison_group,
                        calculated_at, archived_at, is_final
                    )
                    SELECT
                        %s,
                        s.{id_column},
                        %s,
                        %s,
                        kv.key,
                        NULL,
                        kv.value::numeric,
                        NULL,
                        (s.percentiles->>'_sample_size')::integer,
                        s.percentiles->>'_position_group',
                        NOW(),
                        NOW(),
                        %s
                    FROM {table} s,
                         jsonb_each_text(
                            s.percentiles - '_position_group' - '_sample_size'
                         ) AS kv(key, value)
                    WHERE s.sport = %s
                      AND s.season = %s
                      AND s.percentiles IS NOT NULL
                      AND s.percentiles != '{{}}'::jsonb
                      AND kv.key NOT LIKE '\\_%'
                    ON CONFLICT (entity_type, entity_id, sport, season, stat_category, archived_at)
                    DO NOTHING
                    RETURNING 1
                )
                SELECT count(*) AS cnt FROM archived
                """,
                (entity_type, sport_id, season_year, is_final, sport_id, season_year),
            )

            total_count += result["cnt"] if result else 0

        elapsed = time.time() - start
        logger.info(
            "Archived %d percentile records for %s %d (final=%s, %.1fs)",
            total_count,
            sport_id,
            season_year,
            is_final,
            elapsed,
        )

        return total_count
