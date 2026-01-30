"""
Similarity service for entity comparison.

Provides a clean interface over the SimilarityCalculator with
database connection management.
"""

from .service import SimilarityService, get_similarity_service

__all__ = ["SimilarityService", "get_similarity_service"]
