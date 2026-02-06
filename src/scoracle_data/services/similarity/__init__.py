"""
Similarity service for entity comparison.

Uses cosine similarity on percentile vectors to find similar entities.
Access the calculator directly:

    from scoracle_data.similarity import SimilarityCalculator

    calculator = SimilarityCalculator(db)
    result = calculator.calculate_all_for_sport("NBA", season_year=2025)
"""

from ...similarity import SimilarityCalculator

__all__ = ["SimilarityCalculator"]
