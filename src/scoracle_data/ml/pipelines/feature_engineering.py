"""
Feature Engineering for Scoracle ML

Handles:
- Numerical feature extraction and normalization
- Time-series feature computation
- Transfer-specific feature engineering
- Similarity feature preparation
"""

from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Any

import numpy as np

from ..config import (
    ML_CONFIG,
    TIER_WEIGHTS,
    get_features_for_entity,
)


@dataclass
class TransferFeatures:
    """Features for transfer prediction."""

    # Mention frequency features
    mention_freq_24h: int = 0
    mention_freq_7d: int = 0
    mention_freq_30d: int = 0

    # Mention velocity (rate of change)
    mention_velocity_24h: float = 0.0  # vs previous 24h
    mention_velocity_7d: float = 0.0   # vs previous 7d

    # Source tier distribution
    tier_1_ratio: float = 0.0
    tier_2_ratio: float = 0.0
    tier_3_ratio: float = 0.0
    tier_4_ratio: float = 0.0

    # Weighted mention score
    weighted_mention_score: float = 0.0

    # Sentiment features
    avg_sentiment: float = 0.0
    sentiment_std: float = 0.0
    positive_ratio: float = 0.0

    # Temporal features
    days_since_first_link: int = 0
    days_since_last_mention: int = 0
    mention_recency_score: float = 0.0

    # Co-occurrence strength
    cooccurrence_strength: float = 0.0

    # Context features
    transfer_window_active: bool = False
    is_january_window: bool = False
    is_summer_window: bool = False

    def to_array(self) -> np.ndarray:
        """Convert features to numpy array."""
        return np.array([
            self.mention_freq_24h,
            self.mention_freq_7d,
            self.mention_freq_30d,
            self.mention_velocity_24h,
            self.mention_velocity_7d,
            self.tier_1_ratio,
            self.tier_2_ratio,
            self.tier_3_ratio,
            self.tier_4_ratio,
            self.weighted_mention_score,
            self.avg_sentiment,
            self.sentiment_std,
            self.positive_ratio,
            self.days_since_first_link,
            self.days_since_last_mention,
            self.mention_recency_score,
            self.cooccurrence_strength,
            float(self.transfer_window_active),
            float(self.is_january_window),
            float(self.is_summer_window),
        ], dtype=np.float32)

    def to_dict(self) -> dict[str, Any]:
        """Convert features to dictionary."""
        return {
            "mention_freq_24h": self.mention_freq_24h,
            "mention_freq_7d": self.mention_freq_7d,
            "mention_freq_30d": self.mention_freq_30d,
            "mention_velocity_24h": self.mention_velocity_24h,
            "mention_velocity_7d": self.mention_velocity_7d,
            "tier_1_ratio": self.tier_1_ratio,
            "tier_2_ratio": self.tier_2_ratio,
            "tier_3_ratio": self.tier_3_ratio,
            "tier_4_ratio": self.tier_4_ratio,
            "weighted_mention_score": self.weighted_mention_score,
            "avg_sentiment": self.avg_sentiment,
            "sentiment_std": self.sentiment_std,
            "positive_ratio": self.positive_ratio,
            "days_since_first_link": self.days_since_first_link,
            "days_since_last_mention": self.days_since_last_mention,
            "mention_recency_score": self.mention_recency_score,
            "cooccurrence_strength": self.cooccurrence_strength,
            "transfer_window_active": self.transfer_window_active,
            "is_january_window": self.is_january_window,
            "is_summer_window": self.is_summer_window,
        }


@dataclass
class MentionHistory:
    """Time-series mention history for a transfer link."""

    # 30-day history: each entry has [total, tier1, tier2, tier3, tier4]
    daily_mentions: list[list[int]] = field(default_factory=lambda: [[0, 0, 0, 0, 0]] * 30)

    def to_array(self) -> np.ndarray:
        """Convert to numpy array for LSTM input."""
        return np.array(self.daily_mentions, dtype=np.float32)


class FeatureEngineer:
    """
    Feature engineering for ML models.

    Computes features for:
    - Transfer prediction
    - Entity similarity
    - Performance prediction
    """

    def __init__(self):
        """Initialize feature engineer."""
        self._stats_normalizers: dict[str, dict[str, tuple[float, float]]] = {}

    def compute_transfer_features(
        self,
        mentions: list[dict[str, Any]],
        first_linked_at: datetime,
        last_mention_at: datetime,
        sport: str = "football",
    ) -> TransferFeatures:
        """
        Compute transfer prediction features from mentions.

        Args:
            mentions: List of mention records
            first_linked_at: When the link was first created
            last_mention_at: Last mention timestamp
            sport: Sport name (for window detection)

        Returns:
            TransferFeatures object
        """
        features = TransferFeatures()
        now = datetime.now()

        if not mentions:
            return features

        # Frequency features
        features.mention_freq_24h = self._count_mentions_in_window(mentions, hours=24)
        features.mention_freq_7d = self._count_mentions_in_window(mentions, hours=168)
        features.mention_freq_30d = len(mentions)

        # Velocity features
        prev_24h = self._count_mentions_in_window(mentions, hours=48) - features.mention_freq_24h
        if prev_24h > 0:
            features.mention_velocity_24h = (features.mention_freq_24h - prev_24h) / prev_24h
        elif features.mention_freq_24h > 0:
            features.mention_velocity_24h = 1.0

        prev_7d = self._count_mentions_in_window(mentions, hours=336) - features.mention_freq_7d
        if prev_7d > 0:
            features.mention_velocity_7d = (features.mention_freq_7d - prev_7d) / prev_7d
        elif features.mention_freq_7d > 0:
            features.mention_velocity_7d = 1.0

        # Tier distribution
        total = len(mentions)
        tier_counts = {1: 0, 2: 0, 3: 0, 4: 0}
        for m in mentions:
            tier = m.get("source_tier", 4)
            tier_counts[tier] = tier_counts.get(tier, 0) + 1

        features.tier_1_ratio = tier_counts[1] / total
        features.tier_2_ratio = tier_counts[2] / total
        features.tier_3_ratio = tier_counts[3] / total
        features.tier_4_ratio = tier_counts[4] / total

        # Weighted mention score
        features.weighted_mention_score = sum(
            TIER_WEIGHTS.get(f"tier_{t}", 0.15) * c
            for t, c in tier_counts.items()
        )

        # Sentiment features
        sentiments = [m.get("sentiment_score") for m in mentions if m.get("sentiment_score") is not None]
        if sentiments:
            features.avg_sentiment = float(np.mean(sentiments))
            features.sentiment_std = float(np.std(sentiments))
            features.positive_ratio = len([s for s in sentiments if s > 0]) / len(sentiments)

        # Temporal features
        features.days_since_first_link = (now - first_linked_at).days
        features.days_since_last_mention = (now - last_mention_at).days

        # Recency score: exponential decay
        features.mention_recency_score = np.exp(-features.days_since_last_mention / 7)

        # Transfer window detection
        features.transfer_window_active = self._is_transfer_window_active(now, sport)
        features.is_january_window = now.month == 1
        features.is_summer_window = now.month in [6, 7, 8]

        return features

    def _count_mentions_in_window(
        self,
        mentions: list[dict[str, Any]],
        hours: int,
    ) -> int:
        """Count mentions within a time window."""
        cutoff = datetime.now() - timedelta(hours=hours)
        return len([m for m in mentions if m.get("mentioned_at", datetime.min) >= cutoff])

    def _is_transfer_window_active(self, date: datetime, sport: str) -> bool:
        """Check if transfer window is active."""
        month = date.month
        day = date.day

        if sport.lower() == "football":
            # Summer window: July 1 - August 31
            # January window: January 1 - January 31
            if month == 1:
                return True
            if month in [7, 8]:
                return True
            if month == 6 and day >= 10:  # Early signings
                return True
        elif sport.lower() in ["nba", "nfl"]:
            # NBA trade deadline typically mid-February
            # NFL trade deadline typically early November
            if sport.lower() == "nba" and month == 2 and day <= 15:
                return True
            if sport.lower() == "nfl" and month == 11 and day <= 7:
                return True

        return False

    def compute_mention_history(
        self,
        mentions: list[dict[str, Any]],
        days: int = 30,
    ) -> MentionHistory:
        """
        Compute daily mention history for LSTM input.

        Args:
            mentions: List of mention records
            days: Number of days of history

        Returns:
            MentionHistory object
        """
        history = MentionHistory()
        now = datetime.now()

        daily_data = []
        for day_offset in range(days):
            day_start = now - timedelta(days=day_offset + 1)
            day_end = now - timedelta(days=day_offset)

            day_mentions = [
                m for m in mentions
                if day_start <= m.get("mentioned_at", datetime.min) < day_end
            ]

            tier_counts = [0, 0, 0, 0, 0]
            tier_counts[0] = len(day_mentions)
            for m in day_mentions:
                tier = m.get("source_tier", 4)
                if 1 <= tier <= 4:
                    tier_counts[tier] += 1

            daily_data.append(tier_counts)

        # Reverse to have oldest first
        history.daily_mentions = daily_data[::-1]
        return history

    def normalize_stats(
        self,
        stats: dict[str, Any],
        sport: str,
        entity_type: str,
        position: str | None = None,
    ) -> np.ndarray:
        """
        Normalize statistics for similarity/prediction.

        Args:
            stats: Raw statistics dictionary
            sport: Sport name
            entity_type: 'player' or 'team'
            position: Position (for NFL)

        Returns:
            Normalized feature vector
        """
        features = get_features_for_entity(sport, entity_type, position)
        values = []

        for feat in features:
            val = stats.get(feat)
            if val is None:
                values.append(0.0)
            else:
                # Z-score normalization if normalizers are available
                key = f"{sport}_{entity_type}_{feat}"
                if key in self._stats_normalizers:
                    mean, std = self._stats_normalizers[key]
                    if std > 0:
                        values.append((float(val) - mean) / std)
                    else:
                        values.append(0.0)
                else:
                    values.append(float(val))

        return np.array(values, dtype=np.float32)

    def fit_normalizers(
        self,
        all_stats: list[dict[str, Any]],
        sport: str,
        entity_type: str,
        position: str | None = None,
    ) -> None:
        """
        Fit normalizers from a collection of stats.

        Args:
            all_stats: List of statistics dictionaries
            sport: Sport name
            entity_type: 'player' or 'team'
            position: Position (for NFL)
        """
        features = get_features_for_entity(sport, entity_type, position)

        for feat in features:
            values = [s.get(feat) for s in all_stats if s.get(feat) is not None]
            if values:
                mean = float(np.mean(values))
                std = float(np.std(values))
                key = f"{sport}_{entity_type}_{feat}"
                self._stats_normalizers[key] = (mean, std)

    def compute_similarity_features(
        self,
        stats: dict[str, Any],
        sport: str,
        entity_type: str,
        position: str | None = None,
    ) -> np.ndarray:
        """
        Compute features for similarity calculation.

        Args:
            stats: Statistics dictionary
            sport: Sport name
            entity_type: 'player' or 'team'
            position: Position (for NFL)

        Returns:
            Normalized feature vector
        """
        return self.normalize_stats(stats, sport, entity_type, position)

    def compute_performance_features(
        self,
        recent_games: list[dict[str, Any]],
        opponent_stats: dict[str, Any] | None = None,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        Compute features for performance prediction.

        Args:
            recent_games: List of recent game stats
            opponent_stats: Opponent team statistics
            context: Contextual info (rest days, home/away, etc.)

        Returns:
            Feature dictionary
        """
        features = {}
        context = context or {}

        if not recent_games:
            return features

        # Recent performance averages (last 10 games)
        last_10 = recent_games[:10]

        # Get all stat keys from the first game
        stat_keys = [k for k in last_10[0].keys() if isinstance(last_10[0][k], (int, float))]

        for key in stat_keys:
            values = [g.get(key) for g in last_10 if g.get(key) is not None]
            if values:
                features[f"avg_{key}_10g"] = float(np.mean(values))
                features[f"std_{key}_10g"] = float(np.std(values))

        # Last 5 games (more recent form)
        last_5 = recent_games[:5]
        for key in stat_keys:
            values = [g.get(key) for g in last_5 if g.get(key) is not None]
            if values:
                features[f"avg_{key}_5g"] = float(np.mean(values))

        # Trend (last 5 vs previous 5)
        if len(recent_games) >= 10:
            for key in stat_keys:
                recent_vals = [g.get(key) for g in last_5 if g.get(key) is not None]
                prev_vals = [g.get(key) for g in recent_games[5:10] if g.get(key) is not None]
                if recent_vals and prev_vals:
                    features[f"trend_{key}"] = float(np.mean(recent_vals) - np.mean(prev_vals))

        # Context features
        features["rest_days"] = context.get("rest_days", 2)
        features["is_home"] = float(context.get("is_home", True))
        features["is_back_to_back"] = float(context.get("rest_days", 2) <= 1)

        # Opponent strength features
        if opponent_stats:
            features["opp_defensive_rating"] = opponent_stats.get("defensive_rating", 110)
            features["opp_pace"] = opponent_stats.get("pace", 100)

        return features

    def get_top_contributing_features(
        self,
        features: TransferFeatures | dict[str, Any],
        top_k: int = 3,
    ) -> list[str]:
        """
        Get the top contributing features for a prediction.

        Args:
            features: Feature object or dictionary
            top_k: Number of top features to return

        Returns:
            List of feature names
        """
        if isinstance(features, TransferFeatures):
            feature_dict = features.to_dict()
        else:
            feature_dict = features

        # Feature importance heuristics
        importance_weights = {
            "tier_1_ratio": 3.0,
            "tier_2_ratio": 2.0,
            "weighted_mention_score": 2.5,
            "mention_velocity_24h": 2.0,
            "mention_velocity_7d": 1.5,
            "mention_freq_24h": 1.5,
            "avg_sentiment": 1.2,
            "transfer_window_active": 1.5,
            "mention_recency_score": 1.3,
        }

        # Score each feature
        scored = []
        for name, value in feature_dict.items():
            if isinstance(value, (int, float)) and value != 0:
                weight = importance_weights.get(name, 1.0)
                score = abs(float(value)) * weight
                scored.append((name, score))

        # Sort and return top k
        scored.sort(key=lambda x: x[1], reverse=True)
        return [name for name, _ in scored[:top_k]]
