"""
PostgreSQL connection manager for Neon.

Provides a unified interface matching StatsDB but using PostgreSQL via psycopg3.
Supports connection pooling for production workloads.

Sport-Specific Tables (v4.0):
  Player and team profiles are stored in sport-specific tables:
  - nba_player_profiles, nfl_player_profiles, football_player_profiles
  - nba_team_profiles, nfl_team_profiles, football_team_profiles

  This prevents cross-sport ID collisions (API-Sports reuses IDs across sports).
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from typing import Any, Iterator, Optional

from dotenv import load_dotenv

# Load .env file into os.environ before accessing environment variables
load_dotenv()

import psycopg
from psycopg import sql
from psycopg.rows import dict_row
from psycopg_pool import ConnectionPool

from .core.types import (
    PLAYER_PROFILE_TABLES,
    PLAYER_STATS_TABLES,
    TEAM_PROFILE_TABLES,
    TEAM_STATS_TABLES,
)


def _check_connection(conn: psycopg.Connection) -> None:
    """
    Health check callback for connection pool.

    Validates that a connection is still alive before handing it out.
    Raises an exception if the connection is dead, causing the pool
    to discard it and create a new one.

    This is critical for serverless databases like Neon that close
    idle SSL connections unexpectedly.
    """
    conn.execute(sql.SQL("SELECT 1"))


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
        self.connection_string = (
            connection_string
            or os.environ.get("NEON_DATABASE_URL_V2")  # New sport-specific schema
            or os.environ.get("DATABASE_URL")
            or os.environ.get("NEON_DATABASE_URL")
        )
        if not self.connection_string:
            raise ValueError(
                "NEON_DATABASE_URL_V2, DATABASE_URL, or NEON_DATABASE_URL environment variable required"
            )

        # Enforce SSL for all database connections (critical for Neon serverless)
        if "sslmode" not in self.connection_string:
            separator = "&" if "?" in self.connection_string else "?"
            self.connection_string += f"{separator}sslmode=require"

        self._max_pool_size = max_pool_size or int(os.environ.get("DATABASE_POOL_SIZE", 10))
        self._min_pool_size = min_pool_size

        # Initialize connection pool with health checks for serverless databases (Neon)
        # - check: validates connection is alive before use (handles SSL disconnects)
        # - max_idle: close connections idle longer than 5 minutes
        # - max_lifetime: force refresh connections after 1 hour
        # - reconnect_timeout: time to wait for reconnection on pool exhaustion
        self._pool = ConnectionPool(
            self.connection_string,
            min_size=self._min_pool_size,
            max_size=self._max_pool_size,
            kwargs={"row_factory": dict_row},
            check=_check_connection,
            max_idle=300,         # 5 minutes - close idle connections
            max_lifetime=3600,    # 1 hour - force refresh old connections
            reconnect_timeout=60, # 1 minute - time to wait for reconnection
        )

    def open(self) -> None:
        """
        Explicitly open the connection pool.

        This establishes the minimum number of connections immediately,
        rather than waiting for the first request (lazy initialization).
        Call this at application startup to ensure the pool is ready.
        """
        self._pool.open()

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

    def fetchone(self, query: str | sql.Composed, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                row = cur.fetchone()
                return dict(row) if row else None

    def fetchall(self, query: str | sql.Composed, params: tuple = ()) -> list[dict[str, Any]]:
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

    def get_player(self, player_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get player info by ID from sport-specific table."""
        table = PLAYER_PROFILE_TABLES.get(sport_id)
        if not table:
            return None
        query = sql.SQL("SELECT * FROM {} WHERE id = %s").format(sql.Identifier(table))
        return self.fetchone(query, (player_id,))

    def get_team(self, team_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get team info by ID from sport-specific table."""
        table = TEAM_PROFILE_TABLES.get(sport_id)
        if not table:
            return None
        query = sql.SQL("SELECT * FROM {} WHERE id = %s").format(sql.Identifier(table))
        return self.fetchone(query, (team_id,))

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

        table = PLAYER_STATS_TABLES.get(sport_id)
        if not table:
            return None

        query = sql.SQL("SELECT * FROM {} WHERE player_id = %s AND season_id = %s").format(
            sql.Identifier(table)
        )
        return self.fetchone(query, (player_id, season_id))

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

        table = TEAM_STATS_TABLES.get(sport_id)
        if not table:
            return None

        query = sql.SQL("SELECT * FROM {} WHERE team_id = %s AND season_id = %s").format(
            sql.Identifier(table)
        )
        return self.fetchone(query, (team_id, season_id))

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


def get_db() -> PostgresDB:
    """
    Get the singleton database instance.

    Returns:
        PostgresDB instance (Neon serverless PostgreSQL)
    """
    return get_postgres_db()
