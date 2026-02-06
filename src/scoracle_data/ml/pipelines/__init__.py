"""
ML Pipelines package.

TextProcessor is actively used by ml/jobs/mention_scanner.py for
entity extraction and transfer mention detection.

FeatureEngineer is NOT YET INTEGRATED -- it is only referenced by
the unwired PredictionService in ml/inference/. It requires numpy,
so it is lazily imported to avoid pulling in optional dependencies.
"""

from .text_processing import TextProcessor

__all__ = ["FeatureEngineer", "TextProcessor"]


def __getattr__(name: str):
    """Lazy import for unwired modules that require optional dependencies."""
    if name == "FeatureEngineer":
        from .feature_engineering import FeatureEngineer
        return FeatureEngineer
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
