"""
Percentiles Service for Scoracle Data.

Provides per-36 (NBA) and per-90 (Football) stat calculations with percentile rankings.

Usage:
    from scoracle_data.services.percentiles import PercentileService, get_percentile_service
    
    service = get_percentile_service(db)
    result = service.calculate_player_percentiles("NBA", player_id)
"""

# Re-export from existing percentiles module
from ...percentiles import (
    PythonPercentileCalculator,
    PERCENTILE_CATEGORIES,
    get_stat_categories,
)
from .service import PercentileService, get_percentile_service

__all__ = [
    # Service interface
    "PercentileService",
    "get_percentile_service",
    # Direct calculator access (for advanced use)
    "PythonPercentileCalculator",
    # Config
    "PERCENTILE_CATEGORIES",
    "get_stat_categories",
]
