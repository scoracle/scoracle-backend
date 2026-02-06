"""
Stats database seeders.

Provider-specific seeders using BallDontLie (NBA/NFL) and SportMonks (Football).
DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

from .common import SeedResult
from .seed_nba import NBASeedRunner
from .seed_nfl import NFLSeedRunner
from .seed_football import FootballSeedRunner

__all__ = [
    "SeedResult",
    "NBASeedRunner",
    "NFLSeedRunner",
    "FootballSeedRunner",
]
