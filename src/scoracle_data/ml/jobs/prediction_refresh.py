"""
Prediction Refresh Job

Refreshes transfer predictions based on accumulated mentions.
Runs periodically to update prediction scores.
"""

import logging
import time
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Any

from ..config import ML_CONFIG, TIER_WEIGHTS

logger = logging.getLogger(__name__)


@dataclass
class PredictionUpdate:
    """A single prediction update."""

    player_id: int
    team_id: int
    previous_score: float | None
    new_score: float
    mention_count: int
    top_tier: int
    confidence: float


@dataclass
class RefreshResult:
    """Result of a prediction refresh run."""

    predictions_updated: int
    predictions_created: int
    links_updated: int
    errors: list[str] = field(default_factory=list)
    duration_seconds: float = 0.0


class PredictionRefreshJob:
    """
    Refreshes transfer predictions based on recent mentions.

    Algorithm:
    1. Find all player-team pairs with recent mentions
    2. Aggregate mention scores by source tier
    3. Calculate weighted prediction score
    4. Update or create transfer_links and transfer_predictions
    """

    def __init__(self, db: Any, config: dict | None = None):
        """
        Initialize prediction refresh job.

        Args:
            db: Database connection
            config: Optional configuration overrides
        """
        self.db = db
        self.config = config or {}

        # Refresh settings
        self.mention_window_hours = self.config.get("mention_window_hours", 168)  # 7 days
        self.min_mentions = self.config.get("min_mentions", 2)
        self.decay_factor = self.config.get("decay_factor", 0.95)  # Per-day decay

    def run(self, sport_id: str | None = None) -> RefreshResult:
        """
        Run prediction refresh for all or specific sport.

        Args:
            sport_id: Optional sport filter

        Returns:
            Refresh result with counts and errors
        """
        start_time = time.time()
        result = RefreshResult(
            predictions_updated=0,
            predictions_created=0,
            links_updated=0,
        )

        try:
            # Get all player-team pairs with recent mentions
            pairs = self._get_mention_pairs(sport_id)
            logger.info(f"Found {len(pairs)} player-team pairs with recent mentions")

            for pair in pairs:
                try:
                    update = self._calculate_prediction(pair)
                    if update:
                        created = self._store_prediction(update)
                        if created:
                            result.predictions_created += 1
                        else:
                            result.predictions_updated += 1

                except Exception as e:
                    result.errors.append(f"Player {pair['player_id']}: {e}")

            # Update transfer links
            result.links_updated = self._update_transfer_links(sport_id)

            # Clean up old predictions
            self._cleanup_stale_predictions()

        except Exception as e:
            result.errors.append(f"Refresh failed: {e}")
            logger.error(f"Prediction refresh error: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    def _get_mention_pairs(self, sport_id: str | None) -> list[dict]:
        """Get all player-team pairs with sufficient recent mentions."""
        cutoff = datetime.utcnow() - timedelta(hours=self.mention_window_hours)

        query = """
            SELECT
                m.player_id,
                m.team_id,
                m.player_name,
                m.team_name,
                COUNT(*) as mention_count,
                MIN(m.source_tier) as top_tier,
                AVG(m.confidence) as avg_confidence,
                MAX(m.created_at) as last_mention,
                ARRAY_AGG(DISTINCT m.source) as sources
            FROM transfer_mentions m
            WHERE m.created_at > %s
              AND m.player_id IS NOT NULL
              AND m.team_id IS NOT NULL
        """
        params = [cutoff]

        if sport_id:
            query += """
              AND EXISTS (
                  SELECT 1 FROM players p
                  WHERE p.id = m.player_id AND p.sport_id = %s
              )
            """
            params.append(sport_id)

        query += """
            GROUP BY m.player_id, m.team_id, m.player_name, m.team_name
            HAVING COUNT(*) >= %s
            ORDER BY COUNT(*) DESC
        """
        params.append(self.min_mentions)

        return self.db.fetchall(query, tuple(params))

    def _calculate_prediction(self, pair: dict) -> PredictionUpdate | None:
        """Calculate prediction score for a player-team pair."""
        player_id = pair["player_id"]
        team_id = pair["team_id"]

        # Get detailed mentions for this pair
        mentions = self.db.fetchall(
            """
            SELECT source_tier, confidence, created_at
            FROM transfer_mentions
            WHERE player_id = %s AND team_id = %s
              AND created_at > NOW() - INTERVAL '%s hours'
            ORDER BY created_at DESC
            """,
            (player_id, team_id, self.mention_window_hours),
        )

        if not mentions:
            return None

        # Calculate weighted score
        total_weight = 0.0
        weighted_sum = 0.0
        now = datetime.utcnow()

        for mention in mentions:
            # Tier weight (higher tier = higher weight)
            tier_key = f"tier_{mention['source_tier']}"
            tier_weight = TIER_WEIGHTS.get(tier_key, 0.15)

            # Time decay (more recent = higher weight)
            age_days = (now - mention["created_at"]).days
            time_weight = self.decay_factor ** age_days

            # Confidence from extraction
            confidence = mention["confidence"] or 0.5

            # Combined weight
            weight = tier_weight * time_weight * confidence
            total_weight += weight
            weighted_sum += weight * confidence

        if total_weight == 0:
            return None

        # Normalize to 0-1 scale
        base_score = weighted_sum / total_weight

        # Boost for volume (more mentions = higher confidence)
        volume_boost = min(0.2, len(mentions) * 0.02)

        # Boost for high-tier sources
        tier_boost = 0.0
        if pair["top_tier"] == 1:
            tier_boost = 0.15
        elif pair["top_tier"] == 2:
            tier_boost = 0.08

        final_score = min(0.99, base_score + volume_boost + tier_boost)

        # Get previous prediction
        previous = self.db.fetchone(
            """
            SELECT prediction_score FROM transfer_predictions
            WHERE player_id = %s AND team_id = %s AND is_active = true
            """,
            (player_id, team_id),
        )

        return PredictionUpdate(
            player_id=player_id,
            team_id=team_id,
            previous_score=previous["prediction_score"] if previous else None,
            new_score=final_score,
            mention_count=len(mentions),
            top_tier=pair["top_tier"],
            confidence=pair["avg_confidence"] or 0.5,
        )

    def _store_prediction(self, update: PredictionUpdate) -> bool:
        """Store prediction, returns True if created, False if updated."""
        # Check for existing active prediction
        existing = self.db.fetchone(
            """
            SELECT id, prediction_score FROM transfer_predictions
            WHERE player_id = %s AND team_id = %s AND is_active = true
            """,
            (update.player_id, update.team_id),
        )

        if existing:
            # Update existing prediction
            self.db.execute(
                """
                UPDATE transfer_predictions
                SET prediction_score = %s,
                    mention_count = %s,
                    source_tier_best = %s,
                    confidence = %s,
                    updated_at = NOW()
                WHERE id = %s
                """,
                (
                    update.new_score,
                    update.mention_count,
                    update.top_tier,
                    update.confidence,
                    existing["id"],
                ),
            )
            return False
        else:
            # Create new prediction
            self.db.execute(
                """
                INSERT INTO transfer_predictions (
                    player_id, team_id, prediction_score,
                    mention_count, source_tier_best, confidence,
                    is_active, created_at, updated_at
                ) VALUES (%s, %s, %s, %s, %s, %s, true, NOW(), NOW())
                """,
                (
                    update.player_id,
                    update.team_id,
                    update.new_score,
                    update.mention_count,
                    update.top_tier,
                    update.confidence,
                ),
            )
            return True

    def _update_transfer_links(self, sport_id: str | None) -> int:
        """Update transfer_links table based on predictions."""
        # Get high-confidence predictions that need links
        query = """
            SELECT tp.player_id, tp.team_id, tp.prediction_score,
                   p.current_team_id, p.sport_id
            FROM transfer_predictions tp
            JOIN players p ON p.id = tp.player_id
            WHERE tp.is_active = true
              AND tp.prediction_score > 0.3
              AND tp.team_id != p.current_team_id
        """
        params = []

        if sport_id:
            query += " AND p.sport_id = %s"
            params.append(sport_id)

        predictions = self.db.fetchall(query, tuple(params))
        updated = 0

        for pred in predictions:
            # Upsert transfer link
            self.db.execute(
                """
                INSERT INTO transfer_links (
                    player_id, from_team_id, to_team_id,
                    status, likelihood_score, source,
                    created_at, updated_at
                ) VALUES (%s, %s, %s, 'rumored', %s, 'ml_prediction', NOW(), NOW())
                ON CONFLICT (player_id, to_team_id)
                DO UPDATE SET
                    likelihood_score = EXCLUDED.likelihood_score,
                    updated_at = NOW()
                WHERE transfer_links.status NOT IN ('confirmed', 'completed')
                """,
                (
                    pred["player_id"],
                    pred["current_team_id"],
                    pred["team_id"],
                    pred["prediction_score"],
                ),
            )
            updated += 1

        return updated

    def _cleanup_stale_predictions(self) -> int:
        """Mark old predictions as inactive."""
        # Predictions with no mentions in 30 days become inactive
        result = self.db.execute(
            """
            UPDATE transfer_predictions
            SET is_active = false
            WHERE is_active = true
              AND updated_at < NOW() - INTERVAL '30 days'
            RETURNING id
            """
        )
        return result.rowcount if hasattr(result, "rowcount") else 0


class PerformancePredictionRefresh:
    """
    Refreshes performance predictions for upcoming games.

    Runs before game days to pre-compute predictions.
    """

    def __init__(self, db: Any, predictor: Any = None):
        """
        Initialize performance prediction refresh.

        Args:
            db: Database connection
            predictor: Optional PerformancePredictor instance
        """
        self.db = db
        self.predictor = predictor

    def refresh_upcoming(
        self,
        sport_id: str | None = None,
        hours_ahead: int = 48,
    ) -> RefreshResult:
        """
        Refresh predictions for upcoming games.

        Args:
            sport_id: Optional sport filter
            hours_ahead: How far ahead to look for games

        Returns:
            Refresh result
        """
        start_time = time.time()
        result = RefreshResult(
            predictions_updated=0,
            predictions_created=0,
            links_updated=0,
        )

        try:
            # Get upcoming fixtures
            fixtures = self._get_upcoming_fixtures(sport_id, hours_ahead)
            logger.info(f"Found {len(fixtures)} upcoming fixtures")

            for fixture in fixtures:
                try:
                    count = self._predict_for_fixture(fixture)
                    result.predictions_created += count
                except Exception as e:
                    result.errors.append(f"Fixture {fixture['id']}: {e}")

        except Exception as e:
            result.errors.append(f"Performance refresh failed: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    def _get_upcoming_fixtures(
        self, sport_id: str | None, hours_ahead: int
    ) -> list[dict]:
        """Get fixtures in the next N hours."""
        query = """
            SELECT id, sport_id, home_team_id, away_team_id, start_time
            FROM fixtures
            WHERE start_time > NOW()
              AND start_time < NOW() + INTERVAL '%s hours'
              AND status = 'scheduled'
        """
        params = [hours_ahead]

        if sport_id:
            query += " AND sport_id = %s"
            params.append(sport_id)

        query += " ORDER BY start_time"

        return self.db.fetchall(query, tuple(params))

    def _predict_for_fixture(self, fixture: dict) -> int:
        """Generate predictions for all players in a fixture."""
        if not self.predictor:
            return 0

        # Get players for both teams
        players = self.db.fetchall(
            """
            SELECT id, current_team_id, position
            FROM players
            WHERE current_team_id IN (%s, %s)
              AND is_active = true
            """,
            (fixture["home_team_id"], fixture["away_team_id"]),
        )

        count = 0
        for player in players:
            try:
                # Generate prediction
                prediction = self.predictor.predict(
                    player["id"],
                    fixture["sport_id"],
                    context={
                        "opponent_id": (
                            fixture["away_team_id"]
                            if player["current_team_id"] == fixture["home_team_id"]
                            else fixture["home_team_id"]
                        ),
                        "is_home": player["current_team_id"] == fixture["home_team_id"],
                        "game_id": fixture["id"],
                    },
                )

                if prediction:
                    self._store_performance_prediction(
                        player["id"],
                        fixture["id"],
                        prediction,
                    )
                    count += 1

            except Exception as e:
                logger.warning(f"Failed to predict for player {player['id']}: {e}")

        return count

    def _store_performance_prediction(
        self,
        player_id: int,
        fixture_id: int,
        prediction: dict,
    ) -> None:
        """Store a performance prediction."""
        self.db.execute(
            """
            INSERT INTO performance_predictions (
                entity_type, entity_id, game_id,
                predicted_stats, confidence_score,
                model_version, created_at
            ) VALUES ('player', %s, %s, %s, %s, %s, NOW())
            ON CONFLICT (entity_type, entity_id, game_id)
            DO UPDATE SET
                predicted_stats = EXCLUDED.predicted_stats,
                confidence_score = EXCLUDED.confidence_score,
                model_version = EXCLUDED.model_version,
                created_at = NOW()
            """,
            (
                player_id,
                fixture_id,
                prediction.get("stats", {}),
                prediction.get("confidence", 0.5),
                prediction.get("model_version", "v1.0.0"),
            ),
        )
