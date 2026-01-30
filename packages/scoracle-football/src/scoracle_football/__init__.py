"""Scoracle Football - Football (Soccer) data seeder using SportMonks API."""

from .client import SportMonksClient
from .seeder import FootballSeeder, SeedResult
from .cli import cli

__all__ = ["SportMonksClient", "FootballSeeder", "SeedResult", "cli"]
