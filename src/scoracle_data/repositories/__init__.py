"""
Repository layer for database persistence.

Currently uses PostgresDB directly via sport-specific table lookups.
The abstract repository interfaces (PlayerRepository, TeamRepository, etc.)
have been removed in favor of direct PostgresDB usage.
"""

from .postgres import (
    PostgresPlayerRepository,
    PostgresTeamRepository,
    PostgresPlayerStatsRepository,
    PostgresTeamStatsRepository,
)

__all__ = [
    "PostgresPlayerRepository",
    "PostgresTeamRepository",
    "PostgresPlayerStatsRepository",
    "PostgresTeamStatsRepository",
]
