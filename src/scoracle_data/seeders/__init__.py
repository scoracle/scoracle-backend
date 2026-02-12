"""
Stats database seeders.

BaseSeedRunner handles NBA/NFL directly (identical flow).
FootballSeedRunner adds per-league, per-team iteration for SportMonks.
"""

from .base import BaseSeedRunner
from .common import SeedResult, BatchSeedResult
from .football import FootballSeedRunner

__all__ = [
    "BaseSeedRunner",
    "FootballSeedRunner",
    "SeedResult",
    "BatchSeedResult",
]
