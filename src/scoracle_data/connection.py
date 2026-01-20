"""
Database connection module - PostgreSQL only.

This module provides backward-compatible aliases for code that still references
the old SQLite-based StatsDB. All database operations now use PostgreSQL.

Migration Note:
    The SQLite backend has been removed. StatsDB is now an alias for PostgresDB.
    New code should import directly from pg_connection:
    
        from scoracle_data.pg_connection import PostgresDB, get_postgres_db
"""

from __future__ import annotations

# Re-export PostgresDB as StatsDB for backward compatibility
from .pg_connection import PostgresDB as StatsDB
from .pg_connection import get_postgres_db

# Legacy path constant (no longer used, kept for compatibility)
DEFAULT_DB_PATH = None


def get_stats_db(read_only: bool = True) -> StatsDB:
    """
    Get a database connection.
    
    This function now returns a PostgresDB instance. The read_only parameter
    is ignored as PostgreSQL handles this differently.
    
    Args:
        read_only: Ignored (kept for backward compatibility)
        
    Returns:
        PostgresDB instance
    """
    return get_postgres_db()


__all__ = [
    "StatsDB",
    "get_stats_db",
    "DEFAULT_DB_PATH",
]
