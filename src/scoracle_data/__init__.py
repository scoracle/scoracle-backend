"""
Scoracle Stats Database Module

A comprehensive local database system for storing and analyzing sports statistics
from API-Sports. Supports percentile calculations, historical data, and
multi-sport extensibility.

Key Features:
- Local-first architecture (<10ms query times)
- Two-phase seeding (discovery -> profile fetch -> stats)
- Tiered coverage (priority vs non-priority leagues)
- Roster diff engine for trade/transfer detection
- EntityRepository for unified profile access

Usage:
    from scoracle_data import StatsDB, get_stats_db, EntityRepository

    # Get database instance
    db = get_stats_db()

    # Get entity repository for profile access
    repo = EntityRepository(db)
    profile = repo.get_player_profile(player_id=123, sport_id="NBA")

    # Search entities
    results = repo.search_entities("Curry", sport_id="NBA")
"""

from .connection import StatsDB, get_stats_db
from .schema import init_database, run_migrations
from .entity_repository import EntityRepository, get_entity_repository
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
    # Repository
    "EntityRepository",
    "get_entity_repository",
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
