"""
Sport-specific statistics aggregators.

These aggregators handle the conversion from raw API responses to aggregated
season totals. For example, the NBA API returns game-by-game stats that need
to be summed into season totals.

Design: Self-contained modules with zero dependencies on main app.
This module is designed to be extracted to scoracle-data repo.
"""

from .nba import NBAStatsAggregator

__all__ = ["NBAStatsAggregator"]
