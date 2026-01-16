"""
Transfer Predictor Model

Multi-input neural network for predicting transfer likelihood:
- Text embedding branch (for headline/tweet analysis)
- Numerical features branch (mention stats, source tiers)
- Historical patterns branch (LSTM for time-series)
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

from ..config import ML_CONFIG


@dataclass
class TransferPrediction:
    """Result of a transfer prediction."""

    player_name: str
    team_name: str
    probability: float
    confidence_lower: float
    confidence_upper: float
    trend: str  # 'up', 'down', 'stable'
    trend_change_7d: float
    top_factors: list[str]
    model_version: str


class TransferPredictor:
    """
    Transfer/Trade likelihood predictor.

    Uses a multi-input neural network to predict transfer probability
    based on news mentions, social media activity, and historical patterns.
    """

    def __init__(self, model_path: Path | str | None = None):
        """
        Initialize the transfer predictor.

        Args:
            model_path: Path to saved model weights
        """
        self._model = None
        self._encoder = None
        self._model_path = Path(model_path) if model_path else None
        self._version = ML_CONFIG.models["transfer_predictor"].version

        # Try to load model if path provided
        if self._model_path and self._model_path.exists():
            self._load_model()

    def _load_model(self) -> None:
        """Load model from disk."""
        try:
            import tensorflow as tf
            self._model = tf.keras.models.load_model(self._model_path)
        except ImportError:
            pass  # TensorFlow not installed
        except Exception:
            pass  # Model file not found or corrupted

    def _build_model(self) -> Any:
        """
        Build the transfer predictor model architecture.

        Returns:
            Compiled Keras model
        """
        try:
            import tensorflow as tf
            from tensorflow import keras
        except ImportError:
            raise ImportError("TensorFlow is required for model training. Install with: pip install tensorflow")

        # Text input branch (pre-computed embeddings)
        text_input = keras.Input(shape=(512,), name="text_embedding")
        text_branch = keras.layers.Dense(128, activation="relu")(text_input)
        text_branch = keras.layers.Dropout(0.3)(text_branch)
        text_branch = keras.layers.Dense(64, activation="relu")(text_branch)

        # Numerical features branch
        numerical_input = keras.Input(shape=(20,), name="numerical_features")
        num_branch = keras.layers.Dense(64, activation="relu")(numerical_input)
        num_branch = keras.layers.BatchNormalization()(num_branch)
        num_branch = keras.layers.Dense(32, activation="relu")(num_branch)

        # Historical pattern branch (time series of mentions)
        # Shape: (30 days, 5 features per day)
        history_input = keras.Input(shape=(30, 5), name="mention_history")
        history_branch = keras.layers.LSTM(32, return_sequences=False)(history_input)
        history_branch = keras.layers.Dropout(0.2)(history_branch)

        # Combine branches
        combined = keras.layers.concatenate([text_branch, num_branch, history_branch])
        combined = keras.layers.Dense(64, activation="relu")(combined)
        combined = keras.layers.Dropout(0.3)(combined)
        combined = keras.layers.Dense(32, activation="relu")(combined)

        # Output layer
        output = keras.layers.Dense(1, activation="sigmoid", name="probability")(combined)

        model = keras.Model(
            inputs=[text_input, numerical_input, history_input],
            outputs=output,
        )

        model.compile(
            optimizer=keras.optimizers.Adam(learning_rate=0.001),
            loss="binary_crossentropy",
            metrics=["accuracy", keras.metrics.AUC(name="auc")],
        )

        return model

    def train(
        self,
        text_embeddings: np.ndarray,
        numerical_features: np.ndarray,
        mention_histories: np.ndarray,
        labels: np.ndarray,
        validation_split: float = 0.2,
        epochs: int = 50,
        batch_size: int = 32,
    ) -> dict[str, Any]:
        """
        Train the model on labeled data.

        Args:
            text_embeddings: Array of text embeddings (N, 512)
            numerical_features: Array of numerical features (N, 20)
            mention_histories: Array of mention histories (N, 30, 5)
            labels: Binary labels (N,) - 1 if transfer completed, 0 otherwise
            validation_split: Fraction of data for validation
            epochs: Number of training epochs
            batch_size: Training batch size

        Returns:
            Training history dictionary
        """
        if self._model is None:
            self._model = self._build_model()

        history = self._model.fit(
            [text_embeddings, numerical_features, mention_histories],
            labels,
            validation_split=validation_split,
            epochs=epochs,
            batch_size=batch_size,
            callbacks=[
                self._get_early_stopping(),
            ],
        )

        return history.history

    def _get_early_stopping(self) -> Any:
        """Get early stopping callback."""
        try:
            from tensorflow import keras
            return keras.callbacks.EarlyStopping(
                monitor="val_auc",
                patience=5,
                restore_best_weights=True,
                mode="max",
            )
        except ImportError:
            return None

    def predict(
        self,
        text_embedding: np.ndarray,
        numerical_features: np.ndarray,
        mention_history: np.ndarray,
        player_name: str = "",
        team_name: str = "",
        previous_probability: float | None = None,
    ) -> TransferPrediction:
        """
        Predict transfer probability for a single link.

        Args:
            text_embedding: Text embedding vector (512,)
            numerical_features: Numerical features (20,)
            mention_history: Mention history (30, 5)
            player_name: Player name for result
            team_name: Team name for result
            previous_probability: Previous prediction for trend calculation

        Returns:
            TransferPrediction result
        """
        # Use heuristic model if TensorFlow model not available
        if self._model is None:
            return self._predict_heuristic(
                numerical_features,
                player_name,
                team_name,
                previous_probability,
            )

        # Expand dims for batch
        inputs = [
            np.expand_dims(text_embedding, 0),
            np.expand_dims(numerical_features, 0),
            np.expand_dims(mention_history, 0),
        ]

        probability = float(self._model.predict(inputs, verbose=0)[0][0])

        # Calculate confidence interval (simple approximation)
        confidence_range = 0.1 + 0.1 * (1 - probability)  # Wider range for uncertain predictions
        confidence_lower = max(0.0, probability - confidence_range)
        confidence_upper = min(1.0, probability + confidence_range)

        # Calculate trend
        trend = "stable"
        trend_change = 0.0
        if previous_probability is not None:
            trend_change = probability - previous_probability
            if trend_change > 0.05:
                trend = "up"
            elif trend_change < -0.05:
                trend = "down"

        # Extract top factors from numerical features
        top_factors = self._extract_top_factors(numerical_features)

        return TransferPrediction(
            player_name=player_name,
            team_name=team_name,
            probability=probability,
            confidence_lower=confidence_lower,
            confidence_upper=confidence_upper,
            trend=trend,
            trend_change_7d=trend_change,
            top_factors=top_factors,
            model_version=self._version,
        )

    def _predict_heuristic(
        self,
        numerical_features: np.ndarray,
        player_name: str,
        team_name: str,
        previous_probability: float | None,
    ) -> TransferPrediction:
        """
        Heuristic prediction when TensorFlow model not available.

        Uses weighted scoring of key features.
        """
        # Feature indices (from TransferFeatures.to_array())
        # 0: mention_freq_24h, 1: mention_freq_7d, 2: mention_freq_30d
        # 3: mention_velocity_24h, 4: mention_velocity_7d
        # 5: tier_1_ratio, 6: tier_2_ratio, 7: tier_3_ratio, 8: tier_4_ratio
        # 9: weighted_mention_score
        # 10: avg_sentiment, 11: sentiment_std, 12: positive_ratio
        # 13: days_since_first_link, 14: days_since_last_mention
        # 15: mention_recency_score, 16: cooccurrence_strength
        # 17: transfer_window_active, 18: is_january_window, 19: is_summer_window

        score = 0.0

        # Tier 1 mentions are strong signal
        tier_1_ratio = numerical_features[5]
        score += tier_1_ratio * 0.4

        # Tier 2 also meaningful
        tier_2_ratio = numerical_features[6]
        score += tier_2_ratio * 0.2

        # Mention velocity indicates trending
        velocity_24h = numerical_features[3]
        score += min(velocity_24h * 0.1, 0.15)

        # Recent mentions more valuable
        recency = numerical_features[15]
        score += recency * 0.1

        # Transfer window boost
        window_active = numerical_features[17]
        score += window_active * 0.1

        # Sentiment boost
        sentiment = numerical_features[10]
        score += max(0, sentiment * 0.05)

        # Normalize to 0-1
        probability = min(max(score, 0.0), 1.0)

        # If no mentions at all, very low probability
        if numerical_features[2] == 0:  # mention_freq_30d
            probability = 0.01

        # Confidence interval
        confidence_range = 0.15
        confidence_lower = max(0.0, probability - confidence_range)
        confidence_upper = min(1.0, probability + confidence_range)

        # Trend
        trend = "stable"
        trend_change = 0.0
        if previous_probability is not None:
            trend_change = probability - previous_probability
            if trend_change > 0.05:
                trend = "up"
            elif trend_change < -0.05:
                trend = "down"

        top_factors = self._extract_top_factors(numerical_features)

        return TransferPrediction(
            player_name=player_name,
            team_name=team_name,
            probability=probability,
            confidence_lower=confidence_lower,
            confidence_upper=confidence_upper,
            trend=trend,
            trend_change_7d=trend_change,
            top_factors=top_factors,
            model_version=f"{self._version}-heuristic",
        )

    def _extract_top_factors(self, numerical_features: np.ndarray) -> list[str]:
        """Extract top contributing factors."""
        feature_names = [
            "mention_freq_24h", "mention_freq_7d", "mention_freq_30d",
            "mention_velocity_24h", "mention_velocity_7d",
            "tier_1_ratio", "tier_2_ratio", "tier_3_ratio", "tier_4_ratio",
            "weighted_mention_score",
            "avg_sentiment", "sentiment_std", "positive_ratio",
            "days_since_first_link", "days_since_last_mention",
            "mention_recency_score", "cooccurrence_strength",
            "transfer_window_active", "is_january_window", "is_summer_window",
        ]

        # Importance weights for each feature
        importance = [
            1.5, 1.2, 1.0,  # frequency
            2.0, 1.5,        # velocity
            3.0, 2.0, 0.5, 0.2,  # tier ratios
            2.5,             # weighted score
            1.0, 0.5, 1.0,   # sentiment
            0.3, 0.5,        # days
            1.5, 1.2,        # recency, cooccurrence
            1.5, 1.0, 1.0,   # window
        ]

        # Score each feature
        scored = []
        for i, (name, weight) in enumerate(zip(feature_names, importance)):
            if i < len(numerical_features):
                value = abs(float(numerical_features[i]))
                if value > 0:
                    scored.append((name, value * weight))

        # Sort and format
        scored.sort(key=lambda x: x[1], reverse=True)

        # Human-readable names
        readable = {
            "tier_1_ratio": "Tier 1 Sources",
            "tier_2_ratio": "Reliable Media",
            "mention_velocity_24h": "Trending Now",
            "mention_velocity_7d": "Weekly Trend",
            "weighted_mention_score": "Source Quality",
            "mention_recency_score": "Recent Activity",
            "transfer_window_active": "Transfer Window",
            "positive_ratio": "Positive Sentiment",
        }

        return [readable.get(name, name) for name, _ in scored[:3]]

    def save(self, path: Path | str) -> None:
        """Save model to disk."""
        if self._model is not None:
            self._model.save(path)

    def batch_predict(
        self,
        text_embeddings: np.ndarray,
        numerical_features: np.ndarray,
        mention_histories: np.ndarray,
    ) -> np.ndarray:
        """
        Batch prediction for multiple links.

        Args:
            text_embeddings: Array (N, 512)
            numerical_features: Array (N, 20)
            mention_histories: Array (N, 30, 5)

        Returns:
            Array of probabilities (N,)
        """
        if self._model is None:
            # Heuristic batch prediction
            probs = []
            for i in range(len(numerical_features)):
                pred = self._predict_heuristic(
                    numerical_features[i],
                    player_name="",
                    team_name="",
                    previous_probability=None,
                )
                probs.append(pred.probability)
            return np.array(probs)

        return self._model.predict(
            [text_embeddings, numerical_features, mention_histories],
            verbose=0,
        ).flatten()
