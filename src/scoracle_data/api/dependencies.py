"""
Dependency injection for API endpoints.

Provides synchronous database access via connection pooling.
FastAPI handles concurrency via thread pool for sync dependencies.
"""

from typing import Annotated
from fastapi import Depends

from ..pg_connection import PostgresDB, get_db


def close_db() -> None:
    """Close the global database connection. Called at app shutdown."""
    from .. import pg_connection

    if pg_connection._postgres_db is not None:
        pg_connection._postgres_db.close()
        pg_connection._postgres_db = None


# Type alias for dependency injection
# Usage: db: DBDependency
DBDependency = Annotated[PostgresDB, Depends(get_db)]
