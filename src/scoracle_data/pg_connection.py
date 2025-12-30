"""
PostgreSQL connection manager for Neon.

Provides a unified interface matching StatsDB but using PostgreSQL via psycopg3.
Supports connection pooling for production workloads.
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from typing import Any, Iterator, Optional

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import ConnectionPool


class PostgresDB:
    """
    PostgreSQL database connection manager for Neon.

    Provides the same interface as StatsDB for drop-in compatibility,
    with connection pooling for production performance.
    """

    def __init__(
        self,
        connection_string: Optional[str] = None,
        min_pool_size: int = 2,
        max_pool_size: Optional[int] = None,
    ):
        """
        Initialize the PostgreSQL connection manager.

        Args:
            connection_string: PostgreSQL connection URL. Defaults to DATABASE_URL env var.
            min_pool_size: Minimum connections to keep in pool.
            max_pool_size: Maximum connections in pool. Defaults to DATABASE_POOL_SIZE env var or 10.
        """
        self.connection_string = connection_string or os.environ.get("DATABASE_URL")
        if not self.connection_string:
            raise ValueError(
                "DATABASE_URL environment variable required or connection_string must be provided"
            )

        self._max_pool_size = max_pool_size or int(os.environ.get("DATABASE_POOL_SIZE", 10))
        self._min_pool_size = min_pool_size

        # Initialize connection pool
        self._pool = ConnectionPool(
            self.connection_string,
            min_size=self._min_pool_size,
            max_size=self._max_pool_size,
            kwargs={"row_factory": dict_row},
        )

    @contextmanager
    def get_connection(self) -> Iterator[psycopg.Connection]:
        """Get a connection from the pool."""
        with self._pool.connection() as conn:
            yield conn

    @contextmanager
    def cursor(self) -> Iterator[psycopg.Cursor]:
        """Get a cursor for executing queries."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                yield cur

    @contextmanager
    def transaction(self) -> Iterator[psycopg.Connection]:
        """
        Execute queries within a transaction.

        Automatically commits on success, rolls back on failure.
        """
        with self.get_connection() as conn:
            try:
                yield conn
                conn.commit()
            except Exception:
                conn.rollback()
                raise

    def execute(self, query: str, params: tuple = ()) -> None:
        """Execute a single query without returning results."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
            conn.commit()

    def executemany(self, query: str, params_list: list[tuple]) -> None:
        """Execute a query with multiple parameter sets."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.executemany(query, params_list)
            conn.commit()

    def executescript(self, sql: str) -> None:
        """Execute a SQL script (multiple statements)."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(sql)
            conn.commit()

    def fetchone(self, query: str, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                row = cur.fetchone()
                return dict(row) if row else None

    def fetchall(self, query: str, params: tuple = ()) -> list[dict[str, Any]]:
        """Execute a query and fetch all results as dicts."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                return [dict(row) for row in cur.fetchall()]

    def close(self) -> None:
        """Close the connection pool."""
        self._pool.close()

    def is_initialized(self) -> bool:
        """Check if the database has been initialized with schema."""
        try:
            result = self.fetchone(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'meta') as exists"
            )
            return result["exists"] if result else False
        except Exception:
            return False

    # =========================================================================
    # Query Methods (matching StatsDB interface)
    # =========================================================================

    def get_season_id(self, sport_id: str, season_year: int) -> Optional[int]:
        """Get the season ID for a sport and year."""
        result = self.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year),
        )
        return result["id"] if result else None

    def get_current_season(self, sport_id: str) -> Optional[dict[str, Any]]:
        """Get the current season for a sport."""
        return self.fetchone(
            "SELECT * FROM seasons WHERE sport_id = %s AND is_current = true",
            (sport_id,),
        )

    def get_player(self, player_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get player info by ID."""
        return self.fetchone(
            "SELECT * FROM players WHERE id = %s AND sport_id = %s",
            (player_id, sport_id),
        )

    def get_team(self, team_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get team info by ID."""
        return self.fetchone(
            "SELECT * FROM teams WHERE id = %s AND sport_id = %s",
            (team_id, sport_id),
        )

    def get_player_stats(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get player statistics for a given season.

        Args:
            player_id: API-Sports player ID
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Season year

        Returns:
            Dict of stats or None if not found
        """
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",  # Unified table in PostgreSQL
            "FOOTBALL": "football_player_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return None

        return self.fetchone(
            f"SELECT * FROM {table} WHERE player_id = %s AND season_id = %s",
            (player_id, season_id),
        )

    def get_team_stats(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """Get team statistics for a given season."""
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_team_stats",
            "NFL": "nfl_team_stats",
            "FOOTBALL": "football_team_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return None

        return self.fetchone(
            f"SELECT * FROM {table} WHERE team_id = %s AND season_id = %s",
            (team_id, season_id),
        )

    def get_percentiles(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        season_year: int,
    ) -> list[dict[str, Any]]:
        """Get cached percentiles for an entity."""
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        return self.fetchall(
            """
            SELECT stat_category, stat_value, percentile, rank, sample_size, comparison_group
            FROM percentile_cache
            WHERE entity_type = %s AND entity_id = %s AND sport_id = %s AND season_id = %s
            ORDER BY stat_category
            """,
            (entity_type, entity_id, sport_id, season_id),
        )

    def get_meta(self, key: str) -> Optional[str]:
        """Get a metadata value."""
        result = self.fetchone("SELECT value FROM meta WHERE key = %s", (key,))
        return result["value"] if result else None

    def set_meta(self, key: str, value: str) -> None:
        """Set a metadata value."""
        self.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES (%s, %s, NOW())
            ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            """,
            (key, value),
        )


# Global instance
_postgres_db: Optional[PostgresDB] = None


def get_postgres_db() -> PostgresDB:
    """
    Get the global PostgreSQL database instance.

    Returns:
        PostgresDB instance
    """
    global _postgres_db

    if _postgres_db is None:
        _postgres_db = PostgresDB()

    return _postgres_db


def close_postgres_db() -> None:
    """Close the global PostgreSQL database connection."""
    global _postgres_db
    if _postgres_db is not None:
        _postgres_db.close()
        _postgres_db = None


# =========================================================================
# Database Factory - Choose between SQLite and PostgreSQL
# =========================================================================


def get_db(use_postgres: Optional[bool] = None) -> PostgresDB:
    """
    Get a database instance based on configuration.

    Args:
        use_postgres: Force PostgreSQL if True, SQLite if False.
                     If None, uses USE_POSTGRESQL env var (defaults to True).

    Returns:
        Database instance (PostgresDB for Neon)
    """
    if use_postgres is None:
        use_postgres = os.environ.get("USE_POSTGRESQL", "true").lower() in ("true", "1", "yes")

    if use_postgres:
        return get_postgres_db()
    else:
        # Fall back to SQLite for local development
        from .connection import get_stats_db
        return get_stats_db(read_only=False)
