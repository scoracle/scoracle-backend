"""
Percentile calculation and caching for stats database.

Primary calculator: PythonPercentileCalculator (database-agnostic)
Legacy calculators kept for reference but not recommended.
"""

from .python_calculator import PythonPercentileCalculator
from .config import PERCENTILE_CATEGORIES, get_stat_categories

# Legacy imports (deprecated - use PythonPercentileCalculator)
from .calculator import PercentileCalculator
from .pg_calculator import PostgresPercentileCalculator

__all__ = [
    # Primary (recommended)
    "PythonPercentileCalculator",
    # Config
    "PERCENTILE_CATEGORIES",
    "get_stat_categories",
    # Legacy (deprecated)
    "PercentileCalculator",
    "PostgresPercentileCalculator",
]
