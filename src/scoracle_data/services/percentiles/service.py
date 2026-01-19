"""
Percentile Service implementation.

Provides a clean interface over the PythonPercentileCalculator with
caching and database connection management.
"""

import logging
from typing import TYPE_CHECKING, Literal

from ...percentiles import PythonPercentileCalculator, PERCENTILE_CATEGORIES

if TYPE_CHECKING:
    from ...pg_connection import PostgresDB

logger = logging.getLogger(__name__)


class PercentileService:
    """
    Percentile calculation service.
    
    Provides per-36 (NBA), per-90 (Football), and position-based (NFL)
    percentile calculations.
    
    Features:
    - Uses PythonPercentileCalculator for database-agnostic computation
    - Supports batch calculation for efficiency
    - Caches results in the stats table JSONB column
    """
    
    def __init__(self, db: "PostgresDB"):
        """
        Initialize percentile service.
        
        Args:
            db: PostgreSQL database connection
        """
        self._db = db
        self._calculator = PythonPercentileCalculator(db)
    
    def calculate_all(
        self,
        sport: Literal["NBA", "NFL", "FOOTBALL"],
        entity_type: Literal["player", "team"] = "player",
        season: str | None = None,
    ) -> dict:
        """
        Calculate percentiles for all entities of a type.
        
        Args:
            sport: Sport identifier
            entity_type: "player" or "team"
            season: Season identifier (uses current if not provided)
        
        Returns:
            Dictionary with calculation results and stats
        """
        return self._calculator.calculate_all_percentiles(
            sport_id=sport,
            entity_type=entity_type,
            season_id=season,
        )
    
    def get_entity_percentiles(
        self,
        sport: Literal["NBA", "NFL", "FOOTBALL"],
        entity_type: Literal["player", "team"],
        entity_id: int,
        season: str | None = None,
    ) -> dict | None:
        """
        Get percentiles for a specific entity.
        
        Fetches from the cached percentile_data JSONB column.
        
        Args:
            sport: Sport identifier
            entity_type: "player" or "team"
            entity_id: Entity ID
            season: Season identifier
        
        Returns:
            Percentile data dict or None if not found
        """
        return self._calculator.get_entity_percentiles(
            sport_id=sport,
            entity_type=entity_type,
            entity_id=entity_id,
            season_id=season,
        )
    
    def get_stat_categories(
        self,
        sport: Literal["NBA", "NFL", "FOOTBALL"],
        entity_type: Literal["player", "team"] = "player",
    ) -> list[str]:
        """
        Get available stat categories for percentile comparison.
        
        Args:
            sport: Sport identifier
            entity_type: "player" or "team"
        
        Returns:
            List of stat category names
        """
        key = (sport, entity_type)
        return PERCENTILE_CATEGORIES.get(key, [])
    
    def get_status(self) -> dict:
        """Get service status."""
        return {
            "service": "percentiles",
            "calculator": "PythonPercentileCalculator",
            "supported_sports": ["NBA", "NFL", "FOOTBALL"],
            "methodology": {
                "NBA": "Per-36 minute stats, position group comparison",
                "NFL": "Position-specific stat categories",
                "FOOTBALL": "Per-90 minute stats, Top 5 League comparison",
            },
        }


# Singleton instance with lazy DB binding
_percentile_service: PercentileService | None = None


def get_percentile_service(db: "PostgresDB") -> PercentileService:
    """
    Get or create the percentile service.
    
    Args:
        db: PostgreSQL database connection
    
    Returns:
        PercentileService instance
    """
    global _percentile_service
    if _percentile_service is None:
        _percentile_service = PercentileService(db)
    return _percentile_service
