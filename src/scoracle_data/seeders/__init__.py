"""
Stats database seeders.

Two generations of seeders exist:

1. Legacy (API-Sports, psycopg sync):
   - BaseSeeder, NBASeeder, NFLSeeder, FootballSeeder
   - Still wired into cli.py seed commands
   - Will be removed in a future dead-code sweep

2. Current (BallDontLie/SportMonks, asyncpg async):
   - NBASeedRunner, NFLSeedRunner, FootballSeedRunner
   - Use the canonical provider clients from providers/
   - Share SeedResult from common.py
"""

# Legacy seeders (still used by cli.py)
from .base import BaseSeeder
from .nba_seeder import NBASeeder
from .nfl_seeder import NFLSeeder
from .football_seeder import FootballSeeder

# New provider-specific seeders
from .common import SeedResult
from .seed_nba import NBASeedRunner
from .seed_nfl import NFLSeedRunner
from .seed_football import FootballSeedRunner

__all__ = [
    # Legacy
    "BaseSeeder",
    "NBASeeder",
    "NFLSeeder",
    "FootballSeeder",
    # Current
    "SeedResult",
    "NBASeedRunner",
    "NFLSeedRunner",
    "FootballSeedRunner",
]
