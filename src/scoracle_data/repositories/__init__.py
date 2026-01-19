"""
Repository abstraction layer.

Provides database-agnostic interfaces for data persistence,
allowing easy switching between database implementations.

Usage:
    from scoracle_data.repositories import get_repositories
    
    repos = get_repositories(db)
    repos.players.upsert("NBA", player_data, raw_response)
    repos.teams.batch_upsert("NBA", teams, raw_responses)
"""

from .base import (
    PlayerRepository,
    TeamRepository,
    PlayerStatsRepository,
    TeamStatsRepository,
    RepositorySet,
)

__all__ = [
    "PlayerRepository",
    "TeamRepository", 
    "PlayerStatsRepository",
    "TeamStatsRepository",
    "RepositorySet",
]


def get_repositories(db: "StatsDB") -> RepositorySet:
    """
    Get repository set for the given database connection.
    
    Currently always returns PostgreSQL implementations.
    Future: Could detect DB type and return appropriate implementations.
    
    Args:
        db: Database connection
        
    Returns:
        RepositorySet with all repository implementations
    """
    from .postgres import (
        PostgresPlayerRepository,
        PostgresTeamRepository,
        PostgresPlayerStatsRepository,
        PostgresTeamStatsRepository,
    )
    from ..sport_configs import get_config
    
    config = get_config()
    
    return RepositorySet(
        players=PostgresPlayerRepository(db, config),
        teams=PostgresTeamRepository(db, config),
        player_stats=PostgresPlayerStatsRepository(db, config),
        team_stats=PostgresTeamStatsRepository(db, config),
    )
