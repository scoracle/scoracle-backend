"""
Dependency injection for API endpoints.

Provides both synchronous and asynchronous database access.
"""

from typing import Annotated, AsyncGenerator
from fastapi import Depends

from ..pg_connection import PostgresDB, get_postgres_db


# Singleton DB instance for sync operations
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


# Type alias for sync dependency injection
DBDependency = Annotated[PostgresDB, Depends(get_db)]


# Async database support
_async_db_instance = None


async def get_async_db():
    """
    Dependency that provides async database connection.

    Returns:
        AsyncPostgresDB instance with async connection pooling
    """
    global _async_db_instance
    if _async_db_instance is None:
        from ..pg_async import AsyncPostgresDB
        _async_db_instance = AsyncPostgresDB()
        await _async_db_instance.initialize()
    return _async_db_instance


# Type alias for async dependency injection
# Usage: db: AsyncDBDependency
async def async_db_dependency():
    """FastAPI dependency for async database access."""
    db = await get_async_db()
    return db


AsyncDBDependency = Annotated[any, Depends(async_db_dependency)]
