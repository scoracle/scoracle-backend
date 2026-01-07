"""Dependency injection for API endpoints."""

from typing import Annotated
from fastapi import Depends

from ..pg_connection import PostgresDB, get_postgres_db


# Singleton DB instance
_db_instance: PostgresDB | None = None


def get_db() -> PostgresDB:
    """
    Dependency that provides database connection.

    Returns:
        PostgresDB instance with connection pooling
    """
    global _db_instance
    if _db_instance is None:
        _db_instance = get_postgres_db()
    return _db_instance


# Type alias for dependency injection
DBDependency = Annotated[PostgresDB, Depends(get_db)]
