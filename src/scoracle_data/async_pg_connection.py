"""
Async PostgreSQL connection manager for the API layer.

Provides the same query interface as PostgresDB but using psycopg's native
async support (AsyncConnectionPool). This avoids blocking the event loop
when FastAPI handles concurrent requests.

The sync PostgresDB remains for CLI/seeders where async buys nothing.
Both connect to the same Postgres instance — only the pool management differs.
"""

from __future__ import annotations

import logging
import os
from typing import Any, Optional

from dotenv import load_dotenv

load_dotenv()

import psycopg
from psycopg import sql
from psycopg.rows import dict_row
from psycopg_pool import AsyncConnectionPool

logger = logging.getLogger(__name__)


async def _async_check_connection(conn: psycopg.AsyncConnection) -> None:
    """
    Health check callback for async connection pool.

    Validates that a connection is still alive before handing it out.
    Critical for serverless databases like Neon that close idle SSL
    connections unexpectedly.
    """
    await conn.execute(sql.SQL("SELECT 1"))


class AsyncPostgresDB:
    """
    Async PostgreSQL database connection manager for the API layer.

    Mirrors the PostgresDB interface but uses AsyncConnectionPool.
    All methods are async — no thread-pool workarounds needed.
    """

    def __init__(
        self,
        connection_string: Optional[str] = None,
        min_pool_size: int = 2,
        max_pool_size: Optional[int] = None,
    ):
        self.connection_string = (
            connection_string
            or os.environ.get("NEON_DATABASE_URL_V2")
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

        # Pool is created in __init__ but not opened until open() is called.
        # open=False prevents the pool from connecting during import.
        self._pool = AsyncConnectionPool(
            self.connection_string,
            min_size=self._min_pool_size,
            max_size=self._max_pool_size,
            kwargs={"row_factory": dict_row},
            check=_async_check_connection,
            max_idle=300,
            max_lifetime=3600,
            reconnect_timeout=60,
            open=False,
        )

    async def open(self) -> None:
        """
        Open the async connection pool.

        Establishes min_size connections immediately.
        Call this at API startup (in the lifespan handler).
        """
        await self._pool.open()
        logger.info(
            f"Async DB pool opened (min={self._min_pool_size}, max={self._max_pool_size})"
        )

    async def close(self) -> None:
        """Close the async connection pool."""
        await self._pool.close()
        logger.info("Async DB pool closed")

    async def fetchone(
        self,
        query: str | sql.Composed,
        params: tuple = (),
    ) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        async with self._pool.connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                row = await cur.fetchone()
                return dict(row) if row else None

    async def fetchall(
        self,
        query: str | sql.Composed,
        params: tuple = (),
    ) -> list[dict[str, Any]]:
        """Execute a query and fetch all results as dicts."""
        async with self._pool.connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                return [dict(row) for row in await cur.fetchall()]

    async def execute(
        self,
        query: str,
        params: tuple = (),
    ) -> None:
        """Execute a single query without returning results."""
        async with self._pool.connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
            await conn.commit()


# =========================================================================
# Singleton access for the API layer
# =========================================================================

_async_db: Optional[AsyncPostgresDB] = None


def get_async_db() -> AsyncPostgresDB:
    """Get the singleton async database instance.

    Returns a pre-created AsyncPostgresDB. The pool must be opened
    separately via ``await db.open()`` during API startup.
    """
    global _async_db
    if _async_db is None:
        _async_db = AsyncPostgresDB()
    return _async_db
