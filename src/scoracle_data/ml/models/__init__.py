"""
ML Models package.

NOT YET INTEGRATED: These TensorFlow model classes are scaffolding for
future ML inference. They are not currently called by any production code
path (API, CLI, or jobs). The API serves ML data via heuristic fallbacks
and direct SQL queries in the service layer.

Integration will require:
- Training data and trained model weights
- Wiring PredictionService into the API routers / CLI jobs
- TensorFlow added to production dependencies (currently optional)

All imports are lazy to avoid pulling in numpy/tensorflow when not needed.
"""

__all__ = [
    "PerformancePredictor",
    "SentimentAnalyzer",
    "SimilarityEngine",
    "TransferPredictor",
]


def __getattr__(name: str):
    """Lazy import for model classes that require optional dependencies (numpy, tensorflow)."""
    if name == "PerformancePredictor":
        from .performance_predictor import PerformancePredictor
        return PerformancePredictor
    if name == "SentimentAnalyzer":
        from .sentiment_analyzer import SentimentAnalyzer
        return SentimentAnalyzer
    if name == "SimilarityEngine":
        from .similarity_engine import SimilarityEngine
        return SimilarityEngine
    if name == "TransferPredictor":
        from .transfer_predictor import TransferPredictor
        return TransferPredictor
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
