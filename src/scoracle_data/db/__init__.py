"""
Database module for Scoracle Data.

This module provides all database-related functionality:
- Connection management (PostgreSQL/Neon)
- Repository pattern for data access
- Schema and migration management

Usage:
    from scoracle_data.db import PostgresDB, get_db
    from scoracle_data.db import get_repositories
    
    # Get database connection
    db = get_db()
    
    # Get repositories
    repos = get_repositories(db)
    repos.players.upsert("NBA", player_data, raw_response)
"""

# Connection management
from ..pg_connection import PostgresDB

# Schema management
from ..schema import init_database, run_migrations, get_schema_version, get_table_counts

# Repositories
from ..repositories import (
    get_repositories,
    PlayerRepository,
    TeamRepository,
    PlayerStatsRepository,
    TeamStatsRepository,
    RepositorySet,
)

# Singleton database instance
_db_instance: PostgresDB | None = None


def get_db() -> PostgresDB:
    """
    Get the singleton database connection.
    
    Returns:
        PostgresDB instance
    """
    global _db_instance
    if _db_instance is None:
        _db_instance = PostgresDB()
    return _db_instance


def close_db() -> None:
    """Close the database connection."""
    global _db_instance
    if _db_instance is not None:
        _db_instance.close()
        _db_instance = None


__all__ = [
    # Connection
    "PostgresDB",
    "get_db",
    "close_db",
    # Schema
    "init_database",
    "run_migrations",
    "get_schema_version",
    "get_table_counts",
    # Repositories
    "get_repositories",
    "PlayerRepository",
    "TeamRepository",
    "PlayerStatsRepository",
    "TeamStatsRepository",
    "RepositorySet",
]
