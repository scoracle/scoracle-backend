"""
Similarity Service implementation.

Provides a clean interface over the SimilarityCalculator with
database connection management.
"""

import logging
from typing import TYPE_CHECKING, Literal

from ...similarity import SimilarityCalculator

if TYPE_CHECKING:
    from ...pg_connection import PostgresDB

logger = logging.getLogger(__name__)


class SimilarityService:
    """
    Similarity calculation service.

    Provides percentile-based entity similarity computation.
    Uses cosine similarity to compare percentile vectors.

    Features:
    - Batch calculation for all entities in a sport/season
    - Fast lookup of pre-computed similarities
    - Shared traits and key differences generation
    """

    def __init__(self, db: "PostgresDB"):
        """
        Initialize similarity service.

        Args:
            db: PostgreSQL database connection
        """
        self._db = db
        self._calculator = SimilarityCalculator(db)

    def calculate_all(
        self,
        sport: Literal["NBA", "NFL", "FOOTBALL"],
        season_year: int,
        limit: int = 5,
    ) -> dict:
        """
        Calculate similarities for all entities in a sport/season.

        This is the main entry point for batch processing.
        Should be called after percentile calculation completes.

        Args:
            sport: Sport identifier
            season_year: Season year
            limit: Number of similar entities to store per entity

        Returns:
            Dictionary with calculation results (player/team counts)
        """
        return self._calculator.calculate_all_for_sport(
            sport_id=sport,
            season_year=season_year,
            limit=limit,
        )

    def get_similar_entities(
        self,
        sport: Literal["NBA", "NFL", "FOOTBALL"],
        entity_type: Literal["player", "team"],
        entity_id: int,
        season_year: int | None = None,
        limit: int = 5,
    ) -> list:
        """
        Get pre-computed similar entities for a single entity.

        Args:
            sport: Sport identifier
            entity_type: 'player' or 'team'
            entity_id: Entity ID
            season_year: Season year (uses latest if not provided)
            limit: Max number of similar entities to return

        Returns:
            List of SimilarEntity objects
        """
        return self._calculator.get_similar_entities(
            entity_type=entity_type,
            entity_id=entity_id,
            sport_id=sport,
            season_year=season_year,
            limit=limit,
        )

    def get_status(self) -> dict:
        """Get service status."""
        return {
            "service": "similarity",
            "calculator": "SimilarityCalculator",
            "supported_sports": ["NBA", "NFL", "FOOTBALL"],
            "methodology": {
                "algorithm": "Cosine similarity on percentile vectors",
                "scope": "All players/teams in sport (cross-position/cross-league)",
                "output": "Top 5 similar entities with shared traits and differences",
            },
        }


# Singleton instance with lazy DB binding
_similarity_service: SimilarityService | None = None


def get_similarity_service(db: "PostgresDB") -> SimilarityService:
    """
    Get or create the similarity service.

    Args:
        db: PostgreSQL database connection

    Returns:
        SimilarityService instance
    """
    global _similarity_service
    if _similarity_service is None:
        _similarity_service = SimilarityService(db)
    return _similarity_service
