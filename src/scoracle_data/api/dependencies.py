"""
Dependency injection for API endpoints.

Design Decision: Sync vs Async Database Access
----------------------------------------------
This module provides both synchronous and asynchronous database access.

Currently, API routes use the SYNC database connection (`DBDependency`) because:
1. psycopg3's sync connection pooling is already highly efficient
2. The API is I/O-bound on external calls, not DB queries
3. Simpler code with fewer async context management concerns
4. FastAPI handles concurrency via thread pool for sync dependencies

The async implementation (`AsyncDBDependency`) is available for future use
if profiling reveals DB queries as bottlenecks, or for routes that need
to parallelize multiple DB calls.
"""

from typing import Annotated, Any, TYPE_CHECKING
from fastapi import Depends

from ..pg_connection import PostgresDB, get_postgres_db

if TYPE_CHECKING:
    from ..pg_async import AsyncPostgresDB


# =============================================================================
# Synchronous Database Access (Primary - used by all routes)
# =============================================================================

_db_instance: PostgresDB | None = None


def get_db() -> PostgresDB:
    """
    Dependency that provides synchronous database connection.

    Returns:
        PostgresDB instance with connection pooling
    """
    global _db_instance
    if _db_instance is None:
        _db_instance = get_postgres_db()
    return _db_instance


def close_db() -> None:
    """Close the global database connection. Called at app shutdown."""
    global _db_instance
    if _db_instance is not None:
        _db_instance.close()
        _db_instance = None


# Type alias for sync dependency injection
DBDependency = Annotated[PostgresDB, Depends(get_db)]


# =============================================================================
# Asynchronous Database Access (Available for future optimization)
# =============================================================================

_async_db_instance: "AsyncPostgresDB | None" = None


async def get_async_db() -> "AsyncPostgresDB":
    """
    Dependency that provides async database connection.

    Use this for routes that need to parallelize multiple DB calls
    or when profiling shows sync DB is a bottleneck.

    Returns:
        AsyncPostgresDB instance with async connection pooling
    """
    global _async_db_instance
    if _async_db_instance is None:
        from ..pg_async import AsyncPostgresDB
        _async_db_instance = AsyncPostgresDB()
        await _async_db_instance.initialize()
    return _async_db_instance


async def close_async_db() -> None:
    """Close the global async database connection. Called at app shutdown."""
    global _async_db_instance
    if _async_db_instance is not None:
        await _async_db_instance.close()
        _async_db_instance = None


async def async_db_dependency() -> "AsyncPostgresDB":
    """FastAPI dependency for async database access."""
    return await get_async_db()


# Type alias for async dependency injection
# Usage: db: AsyncDBDependency
AsyncDBDependency = Annotated[Any, Depends(async_db_dependency)]
