"""
Scoracle Stats Database Module

A comprehensive database system for storing and analyzing sports statistics.
Supports percentile calculations, historical data, and multi-sport extensibility.

Key Features:
- PostgreSQL backend (Neon serverless) with connection pooling
- API handlers (BallDontLie for NBA/NFL, SportMonks for Football) with generic seeders
- Centralized sport config in core.types.SPORT_REGISTRY
- Per-position percentile calculations with JSONB storage
- Roster diff engine for trade/transfer detection

Usage:
    from scoracle_data.pg_connection import PostgresDB, get_db
    from scoracle_data.services.profiles import get_player_profile

    db = get_db()
    profile = get_player_profile(db, player_id=123, sport="NBA")
"""

from .pg_connection import PostgresDB, get_db
from .schema import init_database, run_migrations
from .core.models import (
    PlayerModel,
    TeamModel,
    ProfileStatus,
    EntityMinimal,
)

__all__ = [
    # Connection
    "PostgresDB",
    "get_db",
    # Schema
    "init_database",
    "run_migrations",
    # Models
    "PlayerModel",
    "TeamModel",
    "ProfileStatus",
    "EntityMinimal",
]
