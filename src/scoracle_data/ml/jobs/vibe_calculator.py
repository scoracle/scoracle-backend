"""
Vibe Calculator Job

Calculates and updates vibe scores (sentiment analysis) for players and teams.
Aggregates sentiment from multiple sources.
"""

import logging
import time
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any

from ..config import get_vibe_label
from ...services.vibes import get_entity_name as _get_entity_name_from_db

logger = logging.getLogger(__name__)


@dataclass
class VibeUpdate:
    """A vibe score update for an entity."""

    entity_type: str
    entity_id: int
    entity_name: str
    previous_score: float | None
    new_score: float
    trend: str  # up, down, stable
    sample_count: int
    source_breakdown: dict[str, float]


@dataclass
class VibeResult:
    """Result of a vibe calculation run."""

    entities_updated: int
    entities_created: int
    samples_processed: int
    errors: list[str] = field(default_factory=list)
    duration_seconds: float = 0.0


class VibeCalculatorJob:
    """
    Calculates vibe scores for players and teams.

    Sources:
    - Twitter mentions (sentiment)
    - Reddit posts/comments
    - News article titles

    Algorithm:
    1. Collect recent sentiment samples
    2. Weight by recency and source reliability
    3. Calculate aggregated vibe score (0-100)
    4. Store with trend analysis
    """

    def __init__(self, db: Any, analyzer: Any = None, config: dict | None = None):
        """
        Initialize vibe calculator.

        Args:
            db: Database connection
            analyzer: Optional SentimentAnalyzer instance
            config: Optional configuration overrides
        """
        self.db = db
        self.analyzer = analyzer
        self.config = config or {}

        # Calculation settings
        self.window_hours = self.config.get("window_hours", 168)  # 7 days
        self.min_samples = self.config.get("min_samples", 5)
        self.decay_per_day = self.config.get("decay_per_day", 0.9)

        # Source weights
        # Note: Reddit weight set to 0 (soft deprecated - removed from API)
        self.source_weights = self.config.get("source_weights", {
            "twitter": 1.0,
            "reddit": 0.0,  # Deprecated - Reddit removed from API
            "news": 0.9,
        })

    def run(
        self,
        entity_type: str | None = None,
        sport_id: str | None = None,
    ) -> VibeResult:
        """
        Run vibe calculation for entities.

        Args:
            entity_type: Optional filter (player, team)
            sport_id: Optional sport filter

        Returns:
            Vibe calculation result
        """
        start_time = time.time()
        result = VibeResult(
            entities_updated=0,
            entities_created=0,
            samples_processed=0,
        )

        try:
            # Get entities that need vibe calculation
            entities = self._get_entities_with_samples(entity_type, sport_id)
            logger.info(f"Found {len(entities)} entities with recent sentiment samples")

            for entity in entities:
                try:
                    update = self._calculate_vibe(entity)
                    if update:
                        created = self._store_vibe(update)
                        result.samples_processed += update.sample_count
                        if created:
                            result.entities_created += 1
                        else:
                            result.entities_updated += 1

                except Exception as e:
                    result.errors.append(
                        f"{entity['entity_type']} {entity['entity_id']}: {e}"
                    )

        except Exception as e:
            result.errors.append(f"Vibe calculation failed: {e}")
            logger.error(f"Vibe calculation error: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    def calculate_for_entity(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        force_refresh: bool = False,
    ) -> VibeUpdate | None:
        """
        Calculate vibe score for a single entity.

        Args:
            entity_type: player or team
            entity_id: Entity ID
            sport_id: Sport identifier (NBA, NFL, FOOTBALL) - required to avoid cross-sport contamination
            force_refresh: Recalculate even if recent score exists

        Returns:
            Vibe update or None if insufficient data
        """
        if not force_refresh:
            # Check if we have a recent score
            recent = self.db.fetchone(
                """
                SELECT score, updated_at FROM vibe_scores
                WHERE entity_type = %s AND entity_id = %s
                  AND updated_at > NOW() - INTERVAL '1 hour'
                """,
                (entity_type, entity_id),
            )
            if recent:
                return None

        entity = {
            "entity_type": entity_type,
            "entity_id": entity_id,
            "entity_name": self._get_entity_name(entity_type, entity_id, sport_id),
        }

        return self._calculate_vibe(entity)

    def _get_entities_with_samples(
        self,
        entity_type: str | None,
        sport_id: str | None,
    ) -> list[dict]:
        """Get entities with recent sentiment samples."""
        cutoff = datetime.now(tz=timezone.utc) - timedelta(hours=self.window_hours)

        # Build query for entities with samples
        query = """
            SELECT
                ss.entity_type,
                ss.entity_id,
                COALESCE(p.full_name, t.name) as entity_name,
                COUNT(*) as sample_count
            FROM sentiment_samples ss
            LEFT JOIN players p ON ss.entity_type = 'player' AND ss.entity_id = p.id
            LEFT JOIN teams t ON ss.entity_type = 'team' AND ss.entity_id = t.id
            WHERE ss.created_at > %s
        """
        params: list[Any] = [cutoff]

        if entity_type:
            query += " AND ss.entity_type = %s"
            params.append(entity_type)

        if sport_id:
            query += """
                AND (
                    (ss.entity_type = 'player' AND p.sport_id = %s) OR
                    (ss.entity_type = 'team' AND t.sport_id = %s)
                )
            """
            params.extend([sport_id, sport_id])

        query += """
            GROUP BY ss.entity_type, ss.entity_id, p.full_name, t.name
            HAVING COUNT(*) >= %s
            ORDER BY COUNT(*) DESC
        """
        params.append(self.min_samples)

        return self.db.fetchall(query, tuple(params))

    def _calculate_vibe(self, entity: dict) -> VibeUpdate | None:
        """Calculate vibe score for an entity."""
        entity_type = entity["entity_type"]
        entity_id = entity["entity_id"]

        # Get sentiment samples
        samples = self.db.fetchall(
            """
            SELECT source, sentiment_score, confidence, created_at
            FROM sentiment_samples
            WHERE entity_type = %s AND entity_id = %s
              AND created_at > NOW() - INTERVAL '%s hours'
            ORDER BY created_at DESC
            """,
            (entity_type, entity_id, self.window_hours),
        )

        if len(samples) < self.min_samples:
            return None

        # Calculate weighted average
        now = datetime.now(tz=timezone.utc)
        total_weight = 0.0
        weighted_sum = 0.0
        source_sums = {}
        source_counts = {}

        for sample in samples:
            source = sample["source"]
            score = sample["sentiment_score"]
            confidence = sample["confidence"] or 0.5

            # Source weight
            source_weight = self.source_weights.get(source, 0.5)

            # Time decay
            age_days = (now - sample["created_at"]).total_seconds() / 86400
            time_weight = self.decay_per_day ** age_days

            # Combined weight
            weight = source_weight * time_weight * confidence
            total_weight += weight
            weighted_sum += weight * score

            # Track per-source
            if source not in source_sums:
                source_sums[source] = 0.0
                source_counts[source] = 0
            source_sums[source] += score
            source_counts[source] += 1

        if total_weight == 0:
            return None

        # Calculate final score (0-100 scale)
        raw_score = weighted_sum / total_weight
        # Sentiment scores are typically -1 to 1, convert to 0-100
        vibe_score = (raw_score + 1) * 50  # Map [-1,1] to [0,100]
        vibe_score = max(0, min(100, vibe_score))

        # Calculate per-source breakdown
        source_breakdown = {}
        for source, total in source_sums.items():
            avg = total / source_counts[source]
            source_breakdown[source] = (avg + 1) * 50

        # Get previous score for trend
        previous = self.db.fetchone(
            """
            SELECT score FROM vibe_scores
            WHERE entity_type = %s AND entity_id = %s
            ORDER BY updated_at DESC
            LIMIT 1
            """,
            (entity_type, entity_id),
        )

        previous_score = previous["score"] if previous else None

        # Calculate trend
        if previous_score is None:
            trend = "stable"
        elif vibe_score > previous_score + 5:
            trend = "up"
        elif vibe_score < previous_score - 5:
            trend = "down"
        else:
            trend = "stable"

        return VibeUpdate(
            entity_type=entity_type,
            entity_id=entity_id,
            entity_name=entity.get("entity_name") or f"{entity_type}_{entity_id}",
            previous_score=previous_score,
            new_score=vibe_score,
            trend=trend,
            sample_count=len(samples),
            source_breakdown=source_breakdown,
        )

    def _store_vibe(self, update: VibeUpdate) -> bool:
        """Store vibe score, returns True if created, False if updated."""
        # Check for existing score
        existing = self.db.fetchone(
            """
            SELECT id FROM vibe_scores
            WHERE entity_type = %s AND entity_id = %s
            """,
            (update.entity_type, update.entity_id),
        )

        label = get_vibe_label(update.new_score)

        if existing:
            # Update existing
            self.db.execute(
                """
                UPDATE vibe_scores
                SET score = %s,
                    label = %s,
                    trend = %s,
                    sample_count = %s,
                    source_breakdown = %s,
                    updated_at = NOW()
                WHERE id = %s
                """,
                (
                    update.new_score,
                    label,
                    update.trend,
                    update.sample_count,
                    update.source_breakdown,
                    existing["id"],
                ),
            )
            return False
        else:
            # Create new
            self.db.execute(
                """
                INSERT INTO vibe_scores (
                    entity_type, entity_id, score, label,
                    trend, sample_count, source_breakdown,
                    created_at, updated_at
                ) VALUES (%s, %s, %s, %s, %s, %s, %s, NOW(), NOW())
                """,
                (
                    update.entity_type,
                    update.entity_id,
                    update.new_score,
                    label,
                    update.trend,
                    update.sample_count,
                    update.source_breakdown,
                ),
            )
            return True

    def _get_entity_name(self, entity_type: str, entity_id: int, sport_id: str) -> str:
        """Get entity name from database using sport-specific tables."""
        name = _get_entity_name_from_db(self.db, entity_type, entity_id, sport=sport_id)
        return name if name != "Unknown" else f"{entity_type}_{entity_id}"


class SentimentSampler:
    """
    Collects sentiment samples from various sources.

    Used by MentionScanner to extract sentiment from collected content.
    """

    def __init__(self, db: Any, analyzer: Any = None):
        """
        Initialize sentiment sampler.

        Args:
            db: Database connection
            analyzer: SentimentAnalyzer instance
        """
        self.db = db
        self.analyzer = analyzer

    def sample_from_mentions(self, hours_back: int = 24) -> int:
        """
        Extract sentiment samples from recent transfer mentions.

        Args:
            hours_back: How many hours back to process

        Returns:
            Number of samples created
        """
        # Get unprocessed mentions
        mentions = self.db.fetchall(
            """
            SELECT id, player_id, team_id, content, source, created_at
            FROM transfer_mentions
            WHERE sentiment_processed = false
              AND created_at > NOW() - INTERVAL '%s hours'
            ORDER BY created_at DESC
            LIMIT 500
            """,
            (hours_back,),
        )

        count = 0
        for mention in mentions:
            try:
                # Analyze sentiment
                if self.analyzer:
                    sentiment = self.analyzer.analyze_text(mention["content"])
                else:
                    # Basic heuristic fallback
                    sentiment = self._basic_sentiment(mention["content"])

                # Store samples for player and team
                if mention["player_id"]:
                    self._store_sample(
                        "player",
                        mention["player_id"],
                        mention["source"],
                        sentiment["score"],
                        sentiment["confidence"],
                        mention["content"],
                    )
                    count += 1

                if mention["team_id"]:
                    self._store_sample(
                        "team",
                        mention["team_id"],
                        mention["source"],
                        sentiment["score"],
                        sentiment["confidence"],
                        mention["content"],
                    )
                    count += 1

                # Mark mention as processed
                self.db.execute(
                    "UPDATE transfer_mentions SET sentiment_processed = true WHERE id = %s",
                    (mention["id"],),
                )

            except Exception as e:
                logger.warning(f"Failed to sample mention {mention['id']}: {e}")

        return count

    def _store_sample(
        self,
        entity_type: str,
        entity_id: int,
        source: str,
        score: float,
        confidence: float,
        content: str,
    ) -> None:
        """Store a sentiment sample."""
        self.db.execute(
            """
            INSERT INTO sentiment_samples (
                entity_type, entity_id, source,
                sentiment_score, confidence, content_preview,
                created_at
            ) VALUES (%s, %s, %s, %s, %s, %s, NOW())
            """,
            (
                entity_type,
                entity_id,
                source,
                score,
                confidence,
                content[:200] if content else None,
            ),
        )

    def _basic_sentiment(self, text: str) -> dict:
        """Basic sentiment analysis without ML model."""
        if not text:
            return {"score": 0.0, "confidence": 0.3}

        text_lower = text.lower()

        # Positive indicators
        positive_words = [
            "great", "amazing", "excellent", "fantastic", "brilliant",
            "exciting", "happy", "love", "best", "win", "success",
            "incredible", "outstanding", "superb", "perfect",
        ]

        # Negative indicators
        negative_words = [
            "bad", "terrible", "awful", "horrible", "poor", "worst",
            "disappointing", "sad", "hate", "fail", "loss", "disaster",
            "pathetic", "shameful", "embarrassing",
        ]

        positive_count = sum(1 for w in positive_words if w in text_lower)
        negative_count = sum(1 for w in negative_words if w in text_lower)

        if positive_count == 0 and negative_count == 0:
            return {"score": 0.0, "confidence": 0.3}

        total = positive_count + negative_count
        score = (positive_count - negative_count) / total

        return {"score": score, "confidence": min(0.6, 0.3 + total * 0.1)}
