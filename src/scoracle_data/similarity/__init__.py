"""
Similarity calculation module.

Computes entity similarity based on percentile vectors using cosine similarity.
Designed to run as a batch process chained after percentile calculation.
"""

from .calculator import SimilarityCalculator

__all__ = ["SimilarityCalculator"]
