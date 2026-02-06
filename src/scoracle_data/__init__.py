"""
Scoracle Stats Database Module

A comprehensive database system for storing and analyzing sports statistics.
Supports percentile calculations, historical data, and multi-sport extensibility.

Key Features:
- PostgreSQL backend (Neon serverless) with connection pooling
- Provider-specific seeders (BallDontLie for NBA/NFL, SportMonks for Football)
- Centralized sport config in core.types.SPORT_REGISTRY
- Per-position percentile calculations with JSONB storage
- Roster diff engine for trade/transfer detection

Usage:
    from scoracle_data import StatsDB, get_stats_db
    from scoracle_data.services.profiles import get_player_profile

    db = get_stats_db()
    profile = get_player_profile(db, player_id=123, sport="NBA")
"""

from .connection import StatsDB, get_stats_db
from .schema import init_database, run_migrations
from .api_client import (
    ApiClientProtocol,
    StandaloneApiClient,
    get_api_client,
    set_api_client,
)
from .models import (
    PlayerProfile,
    TeamProfile,
    PlayerModel,
    TeamModel,
    ProfileStatus,
    EntityMinimal,
)

__all__ = [
    # Connection
    "StatsDB",
    "get_stats_db",
    # Schema
    "init_database",
    "run_migrations",
    # API Client
    "ApiClientProtocol",
    "StandaloneApiClient",
    "get_api_client",
    "set_api_client",
    # Models
    "PlayerProfile",
    "TeamProfile",
    "PlayerModel",
    "TeamModel",
    "ProfileStatus",
    "EntityMinimal",
]
