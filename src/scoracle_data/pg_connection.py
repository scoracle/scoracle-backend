"""
PostgreSQL connection manager for Neon.

Provides a unified interface matching StatsDB but using PostgreSQL via psycopg3.
Supports connection pooling for production workloads.

Unified Schema (v5.0):
  All sports share 4 core tables: players, player_stats, teams, team_stats.
  Each has a (id, sport) composite key to prevent cross-sport ID collisions.
  Sport-specific data lives in JSONB columns (stats, meta).
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

        self._max_pool_size = max_pool_size or int(
            os.environ.get("DATABASE_POOL_SIZE", 10)
        )
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
            max_idle=300,  # 5 minutes - close idle connections
            max_lifetime=3600,  # 1 hour - force refresh old connections
            reconnect_timeout=60,  # 1 minute - time to wait for reconnection
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

    def fetchone(
        self, query: str | sql.Composed, params: tuple = ()
    ) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                row = cur.fetchone()
                return dict(row) if row else None

    def fetchall(
        self, query: str | sql.Composed, params: tuple = ()
    ) -> list[dict[str, Any]]:
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
    # Metadata helpers (used by CLI and schema management)
    # =========================================================================

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


# =========================================================================
# Singleton access
# =========================================================================

_postgres_db: Optional[PostgresDB] = None


def get_db() -> PostgresDB:
    """Get the singleton database instance.

    Returns:
        PostgresDB instance (Neon serverless PostgreSQL with connection pooling)
    """
    global _postgres_db
    if _postgres_db is None:
        _postgres_db = PostgresDB()
    return _postgres_db
