"""
Stats database seeders.

Two seeding approaches are available:

1. Legacy (sport-specific classes):
   - NBASeeder, NFLSeeder, FootballSeeder
   - Tightly coupled to API-Sports
   - Use for backwards compatibility

2. Generic (provider-agnostic):
   - GenericSeeder with DataProviderProtocol
   - Config-driven via YAML files
   - Supports any data provider
   - Stores raw_response for future field extraction
"""

from .base import BaseSeeder
from .nba_seeder import NBASeeder
from .nfl_seeder import NFLSeeder
from .football_seeder import FootballSeeder
from .generic_seeder import GenericSeeder, SeedingResult, DiscoveryResult

__all__ = [
    # Legacy seeders
    "BaseSeeder",
    "NBASeeder",
    "NFLSeeder",
    "FootballSeeder",
    # Generic seeder (recommended for new code)
    "GenericSeeder",
    "SeedingResult",
    "DiscoveryResult",
]
