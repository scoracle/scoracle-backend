"""
Stats database seeders.

Each sport has its own seeder that knows how to fetch data from API-Sports
and transform it into the database schema.
"""

from .base import BaseSeeder
from .nba_seeder import NBASeeder
from .nfl_seeder import NFLSeeder
from .football_seeder import FootballSeeder

__all__ = [
    "BaseSeeder",
    "NBASeeder",
    "NFLSeeder",
    "FootballSeeder",
]
