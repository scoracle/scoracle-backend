"""
Entity Similarity Engine

Uses embeddings and statistical features to find
similar players and teams within the same sport.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

from ..config import ML_CONFIG, get_features_for_entity


@dataclass
class SimilarEntity:
    """A similar entity result."""

    entity_id: int
    entity_name: str
    similarity_score: float
    similarity_label: str
    shared_traits: list[str]
    key_differences: list[str]


@dataclass
class SimilarityResult:
    """Full similarity result for an entity."""

    entity_id: int
    entity_name: str
    entity_type: str
    sport: str
    similar_entities: list[SimilarEntity]


class SimilarityEngine:
    """
    Entity similarity engine using embeddings.

    Computes similarity between players/teams based on
    statistical profiles using autoencoder embeddings.
    """

    def __init__(self, model_path: Path | str | None = None):
        """
        Initialize similarity engine.

        Args:
            model_path: Path to saved autoencoder model
        """
        self._encoder = None
        self._autoencoder = None
        self._model_path = Path(model_path) if model_path else None
        self._version = ML_CONFIG.models["similarity_engine"].version

        # Entity embeddings cache
        self._embeddings: dict[str, np.ndarray] = {}  # key -> embedding
        self._entity_info: dict[str, dict[str, Any]] = {}  # key -> {id, name, features}

        if self._model_path and self._model_path.exists():
            self._load_model()

    def _load_model(self) -> None:
        """Load model from disk."""
        try:
            import tensorflow as tf
            self._autoencoder = tf.keras.models.load_model(self._model_path / "autoencoder")
            self._encoder = tf.keras.models.load_model(self._model_path / "encoder")
        except Exception:
            pass

    def _build_autoencoder(self, input_dim: int, latent_dim: int = 64) -> tuple[Any, Any]:
        """
        Build autoencoder for learning embeddings.

        Args:
            input_dim: Number of input features
            latent_dim: Dimension of latent space

        Returns:
            Tuple of (autoencoder, encoder)
        """
        try:
            from tensorflow import keras
        except ImportError:
            raise ImportError("TensorFlow required. Install with: pip install tensorflow")

        # Encoder
        encoder_input = keras.Input(shape=(input_dim,))
        x = keras.layers.Dense(128, activation="relu")(encoder_input)
        x = keras.layers.BatchNormalization()(x)
        x = keras.layers.Dense(64, activation="relu")(x)
        latent = keras.layers.Dense(latent_dim, activation="linear", name="embedding")(x)

        encoder = keras.Model(encoder_input, latent, name="encoder")

        # Decoder
        decoder_input = keras.Input(shape=(latent_dim,))
        x = keras.layers.Dense(64, activation="relu")(decoder_input)
        x = keras.layers.Dense(128, activation="relu")(x)
        decoder_output = keras.layers.Dense(input_dim, activation="linear")(x)

        decoder = keras.Model(decoder_input, decoder_output, name="decoder")

        # Autoencoder
        autoencoder_input = keras.Input(shape=(input_dim,))
        encoded = encoder(autoencoder_input)
        decoded = decoder(encoded)

        autoencoder = keras.Model(autoencoder_input, decoded, name="autoencoder")
        autoencoder.compile(optimizer="adam", loss="mse")

        return autoencoder, encoder

    def train(
        self,
        features: np.ndarray,
        entity_ids: list[int],
        entity_names: list[str],
        sport: str,
        entity_type: str,
        epochs: int = 100,
        batch_size: int = 32,
        latent_dim: int = 64,
    ) -> dict[str, Any]:
        """
        Train autoencoder on entity features.

        Args:
            features: Feature matrix (N, D)
            entity_ids: List of entity IDs
            entity_names: List of entity names
            sport: Sport name
            entity_type: 'player' or 'team'
            epochs: Training epochs
            batch_size: Batch size
            latent_dim: Embedding dimension

        Returns:
            Training history
        """
        input_dim = features.shape[1]

        self._autoencoder, self._encoder = self._build_autoencoder(input_dim, latent_dim)

        history = self._autoencoder.fit(
            features,
            features,
            epochs=epochs,
            batch_size=batch_size,
            validation_split=0.1,
            verbose=0,
        )

        # Compute and store embeddings
        embeddings = self._encoder.predict(features, verbose=0)

        for i, (eid, name, emb, feat) in enumerate(
            zip(entity_ids, entity_names, embeddings, features)
        ):
            key = f"{sport}_{entity_type}_{eid}"
            self._embeddings[key] = emb
            self._entity_info[key] = {
                "id": eid,
                "name": name,
                "features": feat,
                "sport": sport,
                "entity_type": entity_type,
            }

        return history.history

    def compute_embedding(self, features: np.ndarray) -> np.ndarray:
        """
        Compute embedding for a feature vector.

        Args:
            features: Feature vector (D,)

        Returns:
            Embedding vector
        """
        if self._encoder is not None:
            return self._encoder.predict(
                np.expand_dims(features, 0), verbose=0
            )[0]

        # PCA-like fallback: use normalized features
        norm = np.linalg.norm(features)
        if norm > 0:
            return features / norm
        return features

    def add_entity(
        self,
        entity_id: int,
        entity_name: str,
        features: np.ndarray,
        sport: str,
        entity_type: str,
    ) -> None:
        """
        Add an entity's embedding to the index.

        Args:
            entity_id: Entity ID
            entity_name: Entity name
            features: Feature vector
            sport: Sport name
            entity_type: 'player' or 'team'
        """
        key = f"{sport}_{entity_type}_{entity_id}"
        embedding = self.compute_embedding(features)

        self._embeddings[key] = embedding
        self._entity_info[key] = {
            "id": entity_id,
            "name": entity_name,
            "features": features,
            "sport": sport,
            "entity_type": entity_type,
        }

    def find_similar(
        self,
        entity_id: int,
        sport: str,
        entity_type: str,
        top_k: int = 3,
        exclude_ids: set[int] | None = None,
    ) -> SimilarityResult:
        """
        Find similar entities.

        Args:
            entity_id: Source entity ID
            sport: Sport name
            entity_type: 'player' or 'team'
            top_k: Number of similar entities to return
            exclude_ids: Entity IDs to exclude from results

        Returns:
            SimilarityResult
        """
        exclude_ids = exclude_ids or set()
        source_key = f"{sport}_{entity_type}_{entity_id}"

        if source_key not in self._embeddings:
            return SimilarityResult(
                entity_id=entity_id,
                entity_name="Unknown",
                entity_type=entity_type,
                sport=sport,
                similar_entities=[],
            )

        source_embedding = self._embeddings[source_key]
        source_info = self._entity_info[source_key]
        source_features = source_info["features"]

        # Calculate similarities
        similarities = []
        for key, embedding in self._embeddings.items():
            if key == source_key:
                continue

            info = self._entity_info[key]
            if info["sport"] != sport or info["entity_type"] != entity_type:
                continue
            if info["id"] in exclude_ids:
                continue

            # Cosine similarity
            sim = self._cosine_similarity(source_embedding, embedding)
            similarities.append((key, sim))

        # Sort by similarity
        similarities.sort(key=lambda x: x[1], reverse=True)

        # Build results
        similar_entities = []
        feature_names = get_features_for_entity(sport, entity_type)

        for key, sim in similarities[:top_k]:
            info = self._entity_info[key]
            target_features = info["features"]

            # Determine shared traits and differences
            shared_traits, key_differences = self._analyze_similarity(
                source_features, target_features, feature_names
            )

            # Convert similarity to label
            if sim >= 0.9:
                label = "Very Similar"
            elif sim >= 0.8:
                label = "Similar"
            elif sim >= 0.7:
                label = "Somewhat Similar"
            else:
                label = "Different"

            similar_entities.append(SimilarEntity(
                entity_id=info["id"],
                entity_name=info["name"],
                similarity_score=round(sim, 2),
                similarity_label=label,
                shared_traits=shared_traits,
                key_differences=key_differences,
            ))

        return SimilarityResult(
            entity_id=entity_id,
            entity_name=source_info["name"],
            entity_type=entity_type,
            sport=sport,
            similar_entities=similar_entities,
        )

    def _cosine_similarity(self, a: np.ndarray, b: np.ndarray) -> float:
        """Compute cosine similarity between two vectors."""
        norm_a = np.linalg.norm(a)
        norm_b = np.linalg.norm(b)
        if norm_a == 0 or norm_b == 0:
            return 0.0
        return float(np.dot(a, b) / (norm_a * norm_b))

    def _analyze_similarity(
        self,
        features_a: np.ndarray,
        features_b: np.ndarray,
        feature_names: list[str],
    ) -> tuple[list[str], list[str]]:
        """
        Analyze which features are similar/different.

        Returns:
            Tuple of (shared_traits, key_differences)
        """
        shared_traits = []
        key_differences = []

        # Readable feature names
        readable = {
            "ppg": "scoring",
            "rpg": "rebounding",
            "apg": "playmaking",
            "spg": "steals",
            "bpg": "shot blocking",
            "fg_pct": "shooting efficiency",
            "fg3_pct": "3-point shooting",
            "ft_pct": "free throw shooting",
            "ts_pct": "true shooting",
            "usg_pct": "usage rate",
            "per": "efficiency rating",
            "mpg": "minutes",
            "offensive_rating": "offense",
            "defensive_rating": "defense",
            "pace": "pace",
            "goals": "scoring",
            "assists": "playmaking",
            "pass_completion_pct": "passing",
            "tackles_pg": "tackling",
        }

        for i, name in enumerate(feature_names):
            if i >= len(features_a) or i >= len(features_b):
                continue

            val_a = features_a[i]
            val_b = features_b[i]

            if val_a == 0 and val_b == 0:
                continue

            # Calculate relative difference
            max_val = max(abs(val_a), abs(val_b))
            if max_val > 0:
                diff = abs(val_a - val_b) / max_val
            else:
                diff = 0

            readable_name = readable.get(name, name.replace("_", " "))

            if diff < 0.15:  # Within 15%
                shared_traits.append(readable_name)
            elif diff > 0.4:  # More than 40% different
                # Direction of difference
                if val_a > val_b:
                    key_differences.append(f"more {readable_name}")
                else:
                    key_differences.append(f"less {readable_name}")

        # Limit results
        return shared_traits[:3], key_differences[:3]

    def compare(
        self,
        entity_id_1: int,
        entity_id_2: int,
        sport: str,
        entity_type: str,
    ) -> dict[str, Any]:
        """
        Compare two specific entities.

        Args:
            entity_id_1: First entity ID
            entity_id_2: Second entity ID
            sport: Sport name
            entity_type: 'player' or 'team'

        Returns:
            Comparison dictionary
        """
        key1 = f"{sport}_{entity_type}_{entity_id_1}"
        key2 = f"{sport}_{entity_type}_{entity_id_2}"

        if key1 not in self._embeddings or key2 not in self._embeddings:
            return {"error": "Entity not found in index"}

        emb1 = self._embeddings[key1]
        emb2 = self._embeddings[key2]
        info1 = self._entity_info[key1]
        info2 = self._entity_info[key2]

        similarity = self._cosine_similarity(emb1, emb2)

        feature_names = get_features_for_entity(sport, entity_type)
        shared_traits, key_differences = self._analyze_similarity(
            info1["features"], info2["features"], feature_names
        )

        return {
            "entity_1": {"id": info1["id"], "name": info1["name"]},
            "entity_2": {"id": info2["id"], "name": info2["name"]},
            "similarity_score": round(similarity, 2),
            "shared_traits": shared_traits,
            "key_differences": key_differences,
            "sport": sport,
            "entity_type": entity_type,
        }

    def save(self, path: Path | str) -> None:
        """Save models and index to disk."""
        path = Path(path)
        path.mkdir(parents=True, exist_ok=True)

        if self._autoencoder is not None:
            self._autoencoder.save(path / "autoencoder")
        if self._encoder is not None:
            self._encoder.save(path / "encoder")

        # Save embeddings
        np.save(
            path / "embeddings.npy",
            {k: v for k, v in self._embeddings.items()},
        )

    def load(self, path: Path | str) -> None:
        """Load models and index from disk."""
        path = Path(path)

        try:
            import tensorflow as tf
            self._autoencoder = tf.keras.models.load_model(path / "autoencoder")
            self._encoder = tf.keras.models.load_model(path / "encoder")
        except Exception:
            pass

        try:
            loaded = np.load(path / "embeddings.npy", allow_pickle=True).item()
            self._embeddings = loaded
        except Exception:
            pass
