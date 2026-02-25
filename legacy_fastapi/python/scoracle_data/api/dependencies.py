"""
Dependency injection for API endpoints.

Provides async database access via AsyncPostgresDB connection pooling.
No thread-pool workarounds needed — all DB calls are natively async.
"""

from typing import Annotated
from fastapi import Depends

from ..async_pg_connection import AsyncPostgresDB, get_async_db


async def close_db() -> None:
    """Close the global async database connection. Called at app shutdown."""
    from .. import async_pg_connection

    if async_pg_connection._async_db is not None:
        await async_pg_connection._async_db.close()
        async_pg_connection._async_db = None


# Type alias for dependency injection
# Usage: db: DBDependency
DBDependency = Annotated[AsyncPostgresDB, Depends(get_async_db)]
