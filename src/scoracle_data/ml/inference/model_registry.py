"""
Model Registry for ML Models

Manages model loading, versioning, and lifecycle.
"""

import logging
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

from ..config import ML_CONFIG

logger = logging.getLogger(__name__)


@dataclass
class ModelInfo:
    """Information about a registered model."""

    model_type: str
    version: str
    sport: str | None
    is_active: bool
    model_path: str | None
    loaded_at: datetime | None
    metrics: dict[str, Any] | None


class ModelRegistry:
    """
    Central registry for ML models.

    Handles:
    - Model loading and caching
    - Version management
    - Active model selection
    """

    def __init__(self, storage_path: Path | str | None = None):
        """
        Initialize model registry.

        Args:
            storage_path: Base path for model storage
        """
        self._storage_path = Path(storage_path) if storage_path else ML_CONFIG.model_storage_local
        self._models: dict[str, Any] = {}  # type -> model instance
        self._model_info: dict[str, ModelInfo] = {}  # type -> info

    def register(
        self,
        model_type: str,
        model: Any,
        version: str,
        sport: str | None = None,
        metrics: dict[str, Any] | None = None,
    ) -> None:
        """
        Register a model instance.

        Args:
            model_type: Type of model (e.g., 'transfer_predictor')
            model: Model instance
            version: Model version
            sport: Sport-specific model (optional)
            metrics: Model metrics (optional)
        """
        key = self._make_key(model_type, sport)

        self._models[key] = model
        self._model_info[key] = ModelInfo(
            model_type=model_type,
            version=version,
            sport=sport,
            is_active=True,
            model_path=None,
            loaded_at=datetime.now(),
            metrics=metrics,
        )

        logger.info(f"Registered model: {key} version {version}")

    def get(self, model_type: str, sport: str | None = None) -> Any | None:
        """
        Get a model by type.

        Args:
            model_type: Type of model
            sport: Sport filter (optional)

        Returns:
            Model instance or None
        """
        key = self._make_key(model_type, sport)

        # Try sport-specific first
        if key in self._models:
            return self._models[key]

        # Fall back to general model
        general_key = self._make_key(model_type, None)
        return self._models.get(general_key)

    def get_info(self, model_type: str, sport: str | None = None) -> ModelInfo | None:
        """
        Get model info.

        Args:
            model_type: Type of model
            sport: Sport filter (optional)

        Returns:
            ModelInfo or None
        """
        key = self._make_key(model_type, sport)

        if key in self._model_info:
            return self._model_info[key]

        general_key = self._make_key(model_type, None)
        return self._model_info.get(general_key)

    def load(
        self,
        model_type: str,
        sport: str | None = None,
        version: str | None = None,
    ) -> Any | None:
        """
        Load a model from storage.

        Args:
            model_type: Type of model
            sport: Sport filter (optional)
            version: Specific version to load (optional)

        Returns:
            Loaded model or None
        """
        version = version or ML_CONFIG.models.get(model_type, {}).version

        # Build path
        if sport:
            model_dir = self._storage_path / model_type / sport / version
        else:
            model_dir = self._storage_path / model_type / version

        if not model_dir.exists():
            logger.warning(f"Model directory not found: {model_dir}")
            return None

        try:
            model = self._load_model(model_type, model_dir)
            if model:
                self.register(model_type, model, version, sport)
                return model
        except Exception as e:
            logger.error(f"Failed to load model {model_type}: {e}")

        return None

    def _load_model(self, model_type: str, path: Path) -> Any | None:
        """Load model from path based on type."""
        if model_type == "transfer_predictor":
            from ..models.transfer_predictor import TransferPredictor
            return TransferPredictor(model_path=path)

        elif model_type == "sentiment_analyzer":
            from ..models.sentiment_analyzer import SentimentAnalyzer
            return SentimentAnalyzer()

        elif model_type == "similarity_engine":
            from ..models.similarity_engine import SimilarityEngine
            engine = SimilarityEngine()
            engine.load(path)
            return engine

        return None

    def save(
        self,
        model_type: str,
        sport: str | None = None,
    ) -> bool:
        """
        Save a model to storage.

        Args:
            model_type: Type of model
            sport: Sport filter (optional)

        Returns:
            True if saved successfully
        """
        model = self.get(model_type, sport)
        if model is None:
            return False

        info = self.get_info(model_type, sport)
        if info is None:
            return False

        # Build path
        if sport:
            model_dir = self._storage_path / model_type / sport / info.version
        else:
            model_dir = self._storage_path / model_type / info.version

        model_dir.mkdir(parents=True, exist_ok=True)

        try:
            model.save(model_dir)
            logger.info(f"Saved model {model_type} to {model_dir}")
            return True
        except Exception as e:
            logger.error(f"Failed to save model {model_type}: {e}")
            return False

    def unload(self, model_type: str, sport: str | None = None) -> None:
        """
        Unload a model from memory.

        Args:
            model_type: Type of model
            sport: Sport filter (optional)
        """
        key = self._make_key(model_type, sport)

        if key in self._models:
            del self._models[key]
            logger.info(f"Unloaded model: {key}")

        if key in self._model_info:
            self._model_info[key].is_active = False

    def list_models(self) -> list[ModelInfo]:
        """List all registered models."""
        return list(self._model_info.values())

    def _make_key(self, model_type: str, sport: str | None) -> str:
        """Create a unique key for model lookup."""
        if sport:
            return f"{model_type}:{sport.lower()}"
        return model_type


# Global registry instance
_registry: ModelRegistry | None = None


def get_model_registry() -> ModelRegistry:
    """Get the global model registry instance."""
    global _registry
    if _registry is None:
        _registry = ModelRegistry()
    return _registry
