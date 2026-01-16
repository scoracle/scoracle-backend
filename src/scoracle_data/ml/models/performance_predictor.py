"""
Performance Predictor Model

Predicts entity performance for upcoming games based on:
- Historical performance (LSTM for time-series)
- Opponent strength
- Contextual factors (rest days, home/away, etc.)
"""

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

from ..config import ML_CONFIG


@dataclass
class StatPrediction:
    """Prediction for a single statistic."""

    stat_name: str
    predicted_value: float
    confidence_lower: float
    confidence_upper: float
    historical_avg: float


@dataclass
class PerformancePrediction:
    """Full performance prediction for an entity."""

    entity_id: int
    entity_name: str
    entity_type: str  # 'player' or 'team'
    opponent_id: int | None
    opponent_name: str | None
    game_date: str
    sport: str
    predictions: dict[str, StatPrediction]
    confidence_score: float  # Overall confidence (0-1)
    context_factors: dict[str, Any]
    key_factors: list[str]
    model_version: str


class PerformancePredictor:
    """
    Performance predictor for players and teams.

    Uses LSTM-based architecture to predict performance
    metrics for upcoming games based on historical data
    and contextual factors.
    """

    def __init__(self, model_path: Path | str | None = None):
        """
        Initialize performance predictor.

        Args:
            model_path: Path to saved model weights
        """
        self._model = None
        self._model_path = Path(model_path) if model_path else None
        self._version = ML_CONFIG.models["performance_predictor"].version

        # Sport-specific stat configurations
        self._stat_configs = {
            "nba": {
                "player": ["points", "rebounds", "assists", "steals", "blocks", "turnovers"],
                "team": ["points", "rebounds", "assists", "fg_pct", "fg3_pct"],
            },
            "nfl": {
                "qb": ["pass_yards", "pass_tds", "interceptions", "rush_yards", "passer_rating"],
                "rb": ["rush_yards", "rush_tds", "receptions", "rec_yards"],
                "wr": ["receptions", "rec_yards", "rec_tds", "targets"],
                "team": ["points", "total_yards", "turnovers"],
            },
            "football": {
                "player": ["goals", "assists", "shots", "key_passes", "tackles"],
                "team": ["goals", "shots", "possession", "passes"],
            },
        }

        if self._model_path and self._model_path.exists():
            self._load_model()

    def _load_model(self) -> None:
        """Load model from disk."""
        try:
            import tensorflow as tf
            self._model = tf.keras.models.load_model(self._model_path)
        except ImportError:
            pass
        except Exception:
            pass

    def _build_model(self, num_stats: int, history_length: int = 10) -> Any:
        """
        Build the performance predictor model.

        Args:
            num_stats: Number of statistics to predict
            history_length: Number of historical games to use

        Returns:
            Compiled Keras model
        """
        try:
            from tensorflow import keras
        except ImportError:
            raise ImportError("TensorFlow required. Install with: pip install tensorflow")

        # Historical performance branch (LSTM)
        # Input: (history_length, num_stats + context_features)
        history_input = keras.Input(
            shape=(history_length, num_stats + 5),  # 5 context features per game
            name="game_history",
        )
        lstm_branch = keras.layers.LSTM(64, return_sequences=True)(history_input)
        lstm_branch = keras.layers.Dropout(0.2)(lstm_branch)
        lstm_branch = keras.layers.LSTM(32, return_sequences=False)(lstm_branch)

        # Contextual features branch
        # Input: rest_days, is_home, opponent_rating, season_progress, back_to_back
        context_input = keras.Input(shape=(10,), name="context_features")
        context_branch = keras.layers.Dense(32, activation="relu")(context_input)
        context_branch = keras.layers.BatchNormalization()(context_branch)

        # Opponent features branch
        opponent_input = keras.Input(shape=(15,), name="opponent_features")
        opponent_branch = keras.layers.Dense(32, activation="relu")(opponent_input)

        # Combine branches
        combined = keras.layers.concatenate([lstm_branch, context_branch, opponent_branch])
        combined = keras.layers.Dense(64, activation="relu")(combined)
        combined = keras.layers.Dropout(0.3)(combined)
        combined = keras.layers.Dense(32, activation="relu")(combined)

        # Output: predicted stats
        output = keras.layers.Dense(num_stats, activation="linear", name="predictions")(combined)

        model = keras.Model(
            inputs=[history_input, context_input, opponent_input],
            outputs=output,
        )

        model.compile(
            optimizer=keras.optimizers.Adam(learning_rate=0.001),
            loss="mse",
            metrics=["mae"],
        )

        return model

    def train(
        self,
        game_histories: np.ndarray,
        context_features: np.ndarray,
        opponent_features: np.ndarray,
        targets: np.ndarray,
        validation_split: float = 0.2,
        epochs: int = 100,
        batch_size: int = 32,
    ) -> dict[str, Any]:
        """
        Train the model.

        Args:
            game_histories: Array (N, history_length, features)
            context_features: Array (N, 10)
            opponent_features: Array (N, 15)
            targets: Array (N, num_stats)
            validation_split: Validation data fraction
            epochs: Training epochs
            batch_size: Batch size

        Returns:
            Training history
        """
        num_stats = targets.shape[1]
        history_length = game_histories.shape[1]

        if self._model is None:
            self._model = self._build_model(num_stats, history_length)

        try:
            from tensorflow import keras

            history = self._model.fit(
                [game_histories, context_features, opponent_features],
                targets,
                validation_split=validation_split,
                epochs=epochs,
                batch_size=batch_size,
                callbacks=[
                    keras.callbacks.EarlyStopping(
                        monitor="val_loss",
                        patience=10,
                        restore_best_weights=True,
                    ),
                ],
            )
            return history.history
        except Exception as e:
            return {"error": str(e)}

    def predict(
        self,
        entity_id: int,
        entity_name: str,
        entity_type: str,
        sport: str,
        recent_games: list[dict[str, Any]],
        opponent_id: int | None = None,
        opponent_name: str | None = None,
        opponent_stats: dict[str, Any] | None = None,
        context: dict[str, Any] | None = None,
        game_date: str = "",
        position: str | None = None,
    ) -> PerformancePrediction:
        """
        Predict performance for an upcoming game.

        Args:
            entity_id: Entity ID
            entity_name: Entity name
            entity_type: 'player' or 'team'
            sport: Sport name
            recent_games: List of recent game stats
            opponent_id: Opponent ID
            opponent_name: Opponent name
            opponent_stats: Opponent team/defense stats
            context: Context dict (rest_days, is_home, etc.)
            game_date: Date of the game
            position: Player position (for NFL)

        Returns:
            PerformancePrediction
        """
        context = context or {}
        opponent_stats = opponent_stats or {}

        # Get stat names for this entity
        stat_names = self._get_stat_names(sport, entity_type, position)

        # Use heuristic if model not available
        if self._model is None:
            return self._predict_heuristic(
                entity_id=entity_id,
                entity_name=entity_name,
                entity_type=entity_type,
                sport=sport,
                recent_games=recent_games,
                stat_names=stat_names,
                opponent_id=opponent_id,
                opponent_name=opponent_name,
                opponent_stats=opponent_stats,
                context=context,
                game_date=game_date,
            )

        # Prepare inputs for model
        game_history = self._prepare_game_history(recent_games, stat_names)
        context_features = self._prepare_context_features(context)
        opponent_features = self._prepare_opponent_features(opponent_stats)

        # Get predictions
        predictions_array = self._model.predict(
            [
                np.expand_dims(game_history, 0),
                np.expand_dims(context_features, 0),
                np.expand_dims(opponent_features, 0),
            ],
            verbose=0,
        )[0]

        # Build prediction result
        predictions = {}
        for i, stat_name in enumerate(stat_names):
            if i < len(predictions_array):
                pred_val = float(predictions_array[i])
                hist_avg = self._calculate_historical_avg(recent_games, stat_name)
                std = self._calculate_historical_std(recent_games, stat_name)

                predictions[stat_name] = StatPrediction(
                    stat_name=stat_name,
                    predicted_value=round(pred_val, 1),
                    confidence_lower=round(max(0, pred_val - 1.5 * std), 1),
                    confidence_upper=round(pred_val + 1.5 * std, 1),
                    historical_avg=round(hist_avg, 1),
                )

        # Calculate overall confidence
        confidence = self._calculate_confidence(recent_games, context)

        # Identify key factors
        key_factors = self._get_key_factors(context, opponent_stats)

        return PerformancePrediction(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            opponent_id=opponent_id,
            opponent_name=opponent_name,
            game_date=game_date,
            sport=sport,
            predictions=predictions,
            confidence_score=confidence,
            context_factors=context,
            key_factors=key_factors,
            model_version=self._version,
        )

    def _predict_heuristic(
        self,
        entity_id: int,
        entity_name: str,
        entity_type: str,
        sport: str,
        recent_games: list[dict[str, Any]],
        stat_names: list[str],
        opponent_id: int | None,
        opponent_name: str | None,
        opponent_stats: dict[str, Any],
        context: dict[str, Any],
        game_date: str,
    ) -> PerformancePrediction:
        """
        Heuristic prediction when model not available.

        Uses weighted average of recent games with adjustments
        for opponent strength and context.
        """
        predictions = {}

        for stat_name in stat_names:
            # Calculate weighted average (recent games weighted more)
            values = []
            weights = []
            for i, game in enumerate(recent_games[:10]):
                val = game.get(stat_name)
                if val is not None:
                    values.append(float(val))
                    # Exponential decay: more recent = higher weight
                    weights.append(np.exp(-i * 0.2))

            if values:
                weights = np.array(weights)
                weights = weights / weights.sum()
                pred_val = float(np.sum(np.array(values) * weights))
                hist_avg = float(np.mean(values))
                std = float(np.std(values)) if len(values) > 1 else hist_avg * 0.2

                # Apply context adjustments
                pred_val = self._apply_context_adjustments(
                    pred_val, stat_name, context, opponent_stats
                )

                predictions[stat_name] = StatPrediction(
                    stat_name=stat_name,
                    predicted_value=round(pred_val, 1),
                    confidence_lower=round(max(0, pred_val - 1.5 * std), 1),
                    confidence_upper=round(pred_val + 1.5 * std, 1),
                    historical_avg=round(hist_avg, 1),
                )
            else:
                # No historical data
                predictions[stat_name] = StatPrediction(
                    stat_name=stat_name,
                    predicted_value=0.0,
                    confidence_lower=0.0,
                    confidence_upper=0.0,
                    historical_avg=0.0,
                )

        confidence = self._calculate_confidence(recent_games, context)
        key_factors = self._get_key_factors(context, opponent_stats)

        return PerformancePrediction(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            opponent_id=opponent_id,
            opponent_name=opponent_name,
            game_date=game_date,
            sport=sport,
            predictions=predictions,
            confidence_score=confidence,
            context_factors=context,
            key_factors=key_factors,
            model_version=f"{self._version}-heuristic",
        )

    def _apply_context_adjustments(
        self,
        base_value: float,
        stat_name: str,
        context: dict[str, Any],
        opponent_stats: dict[str, Any],
    ) -> float:
        """Apply contextual adjustments to prediction."""
        adjusted = base_value

        # Rest days adjustment
        rest_days = context.get("rest_days", 2)
        if rest_days == 0:  # Back-to-back
            adjusted *= 0.95  # 5% reduction
        elif rest_days >= 3:  # Well rested
            adjusted *= 1.02  # 2% boost

        # Home/Away adjustment
        is_home = context.get("is_home", True)
        if is_home:
            adjusted *= 1.03  # 3% home advantage
        else:
            adjusted *= 0.98  # 2% road penalty

        # Opponent strength adjustment
        opp_def_rating = opponent_stats.get("defensive_rating")
        if opp_def_rating:
            # Higher defensive rating = worse defense = more points
            league_avg = 110  # NBA league average
            if opp_def_rating > league_avg:
                adjusted *= 1 + (opp_def_rating - league_avg) / 100
            else:
                adjusted *= 1 - (league_avg - opp_def_rating) / 150

        return adjusted

    def _get_stat_names(
        self,
        sport: str,
        entity_type: str,
        position: str | None = None,
    ) -> list[str]:
        """Get stat names for prediction."""
        sport_lower = sport.lower()
        if sport_lower not in self._stat_configs:
            return ["points", "rebounds", "assists"]  # Default

        sport_stats = self._stat_configs[sport_lower]

        # NFL has position-specific stats
        if sport_lower == "nfl" and position:
            pos_lower = position.lower()
            if pos_lower in sport_stats:
                return sport_stats[pos_lower]

        if entity_type in sport_stats:
            return sport_stats[entity_type]

        return list(sport_stats.values())[0]  # Default to first

    def _prepare_game_history(
        self,
        recent_games: list[dict[str, Any]],
        stat_names: list[str],
        history_length: int = 10,
    ) -> np.ndarray:
        """Prepare game history array for model input."""
        num_features = len(stat_names) + 5  # stats + context per game
        history = np.zeros((history_length, num_features), dtype=np.float32)

        for i, game in enumerate(recent_games[:history_length]):
            # Stats
            for j, stat in enumerate(stat_names):
                val = game.get(stat, 0)
                history[i, j] = float(val) if val else 0.0

            # Context features per game
            history[i, len(stat_names)] = float(game.get("is_home", 1))
            history[i, len(stat_names) + 1] = float(game.get("rest_days", 2))
            history[i, len(stat_names) + 2] = float(game.get("won", 0))
            history[i, len(stat_names) + 3] = float(game.get("minutes", 30))
            history[i, len(stat_names) + 4] = float(game.get("opponent_rating", 100))

        return history

    def _prepare_context_features(self, context: dict[str, Any]) -> np.ndarray:
        """Prepare context features array."""
        return np.array([
            float(context.get("rest_days", 2)),
            float(context.get("is_home", 1)),
            float(context.get("is_back_to_back", 0)),
            float(context.get("games_in_last_7_days", 3)),
            float(context.get("season_progress", 0.5)),  # 0-1
            float(context.get("is_playoff", 0)),
            float(context.get("minutes_last_5_avg", 30)),
            float(context.get("team_win_streak", 0)),
            float(context.get("player_hot_streak", 0)),
            float(context.get("injury_status", 0)),  # 0=healthy, 1=questionable
        ], dtype=np.float32)

    def _prepare_opponent_features(self, opponent_stats: dict[str, Any]) -> np.ndarray:
        """Prepare opponent features array."""
        return np.array([
            float(opponent_stats.get("defensive_rating", 110)),
            float(opponent_stats.get("offensive_rating", 110)),
            float(opponent_stats.get("pace", 100)),
            float(opponent_stats.get("opp_fg_pct", 0.45)),
            float(opponent_stats.get("opp_fg3_pct", 0.35)),
            float(opponent_stats.get("opp_ft_pct", 0.75)),
            float(opponent_stats.get("opp_rebounds", 44)),
            float(opponent_stats.get("opp_turnovers", 14)),
            float(opponent_stats.get("recent_form", 0.5)),  # Win % last 10
            float(opponent_stats.get("home_record", 0.5)),
            float(opponent_stats.get("away_record", 0.5)),
            float(opponent_stats.get("vs_position_rating", 100)),
            float(opponent_stats.get("injury_impact", 0)),
            float(opponent_stats.get("back_to_back", 0)),
            float(opponent_stats.get("travel_distance", 0)),
        ], dtype=np.float32)

    def _calculate_historical_avg(
        self,
        games: list[dict[str, Any]],
        stat_name: str,
    ) -> float:
        """Calculate historical average for a stat."""
        values = [g.get(stat_name) for g in games if g.get(stat_name) is not None]
        return float(np.mean(values)) if values else 0.0

    def _calculate_historical_std(
        self,
        games: list[dict[str, Any]],
        stat_name: str,
    ) -> float:
        """Calculate historical standard deviation for a stat."""
        values = [g.get(stat_name) for g in games if g.get(stat_name) is not None]
        if len(values) > 1:
            return float(np.std(values))
        return float(np.mean(values) * 0.2) if values else 1.0

    def _calculate_confidence(
        self,
        recent_games: list[dict[str, Any]],
        context: dict[str, Any],
    ) -> float:
        """Calculate overall confidence score."""
        confidence = 0.5  # Base confidence

        # More games = higher confidence
        num_games = len(recent_games)
        if num_games >= 10:
            confidence += 0.2
        elif num_games >= 5:
            confidence += 0.1

        # Consistent performance = higher confidence
        if recent_games:
            # Check variance in a key stat (first stat usually points)
            key_stat = list(recent_games[0].keys())[0] if recent_games[0] else "points"
            values = [g.get(key_stat) for g in recent_games if g.get(key_stat)]
            if values and len(values) > 1:
                cv = np.std(values) / np.mean(values) if np.mean(values) > 0 else 1
                if cv < 0.2:  # Low variance
                    confidence += 0.15
                elif cv < 0.3:
                    confidence += 0.1

        # Context adjustments
        rest_days = context.get("rest_days", 2)
        if rest_days >= 2:  # Normal rest
            confidence += 0.05

        is_home = context.get("is_home", True)
        if is_home:  # Home games more predictable
            confidence += 0.05

        return min(confidence, 0.95)

    def _get_key_factors(
        self,
        context: dict[str, Any],
        opponent_stats: dict[str, Any],
    ) -> list[str]:
        """Identify key factors affecting the prediction."""
        factors = []

        rest_days = context.get("rest_days", 2)
        if rest_days == 0:
            factors.append("Back-to-back game")
        elif rest_days >= 3:
            factors.append("Well rested")

        is_home = context.get("is_home")
        if is_home is not None:
            factors.append("Home game" if is_home else "Road game")

        def_rating = opponent_stats.get("defensive_rating")
        if def_rating:
            if def_rating > 115:
                factors.append("Weak opponent defense")
            elif def_rating < 105:
                factors.append("Strong opponent defense")

        pace = opponent_stats.get("pace")
        if pace:
            if pace > 102:
                factors.append("Fast-paced opponent")
            elif pace < 98:
                factors.append("Slow-paced opponent")

        return factors[:4]  # Limit to 4 factors

    def save(self, path: Path | str) -> None:
        """Save model to disk."""
        if self._model is not None:
            self._model.save(path)

    def batch_predict(
        self,
        entities: list[dict[str, Any]],
    ) -> list[PerformancePrediction]:
        """
        Batch predict for multiple entities.

        Args:
            entities: List of entity dicts with required fields

        Returns:
            List of predictions
        """
        predictions = []
        for entity in entities:
            pred = self.predict(
                entity_id=entity["entity_id"],
                entity_name=entity["entity_name"],
                entity_type=entity.get("entity_type", "player"),
                sport=entity["sport"],
                recent_games=entity.get("recent_games", []),
                opponent_id=entity.get("opponent_id"),
                opponent_name=entity.get("opponent_name"),
                opponent_stats=entity.get("opponent_stats"),
                context=entity.get("context"),
                game_date=entity.get("game_date", ""),
                position=entity.get("position"),
            )
            predictions.append(pred)
        return predictions
