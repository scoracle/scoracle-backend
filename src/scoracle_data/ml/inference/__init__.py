"""
ML Inference package.

NOT YET INTEGRATED: ModelRegistry and PredictionService orchestrate
TensorFlow model loading and inference, but are not currently called
by any production code path. The API serves predictions via heuristic
fallbacks in the service layer.

See ml/models/__init__.py for integration requirements.

All imports are lazy to avoid pulling in numpy/tensorflow when not needed.
"""

__all__ = ["ModelRegistry", "PredictionService"]


def __getattr__(name: str):
    """Lazy import for inference classes that require optional dependencies."""
    if name == "ModelRegistry":
        from .model_registry import ModelRegistry
        return ModelRegistry
    if name == "PredictionService":
        from .prediction_service import PredictionService
        return PredictionService
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
