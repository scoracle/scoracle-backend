"""
Percentiles for Scoracle Data.

Provides per-36 (NBA), per-90 (Football), and position-based (NFL)
percentile calculations via PythonPercentileCalculator.

Usage:
    from scoracle_data.services.percentiles import PythonPercentileCalculator

    calculator = PythonPercentileCalculator(db)
    result = calculator.calculate_all_percentiles("NBA", "player", season_id)
"""

from ...percentiles import (
    PythonPercentileCalculator,
    PERCENTILE_CATEGORIES,
    get_stat_categories,
)

__all__ = [
    "PythonPercentileCalculator",
    "PERCENTILE_CATEGORIES",
    "get_stat_categories",
]
