"""
Dependency injection for API endpoints.

Provides synchronous database access via connection pooling.
FastAPI handles concurrency via thread pool for sync dependencies.
"""

from typing import Annotated
from fastapi import Depends

from ..pg_connection import PostgresDB, get_postgres_db


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


# Type alias for dependency injection
# Usage: db: DBDependency
DBDependency = Annotated[PostgresDB, Depends(get_db)]
