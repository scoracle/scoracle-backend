"""
Similarity calculator for entity comparison based on percentile vectors.

Computes cosine similarity between entities using their percentile profiles,
then stores the top N most similar entities for fast API lookups.

This is designed to run as a batch process, chained after percentile calculation.
"""

from __future__ import annotations

import logging
import math
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any

from ..percentiles.config import get_stat_label

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


# Table mappings for fetching percentiles
STATS_TABLE_MAP = {
    ("NBA", "player"): "nba_player_stats",
    ("NBA", "team"): "nba_team_stats",
    ("NFL", "player"): "nfl_player_stats",
    ("NFL", "team"): "nfl_team_stats",
    ("FOOTBALL", "player"): "football_player_stats",
    ("FOOTBALL", "team"): "football_team_stats",
}

PROFILE_TABLE_MAP = {
    ("NBA", "player"): "nba_player_profiles",
    ("NBA", "team"): "nba_team_profiles",
    ("NFL", "player"): "nfl_player_profiles",
    ("NFL", "team"): "nfl_team_profiles",
    ("FOOTBALL", "player"): "football_player_profiles",
    ("FOOTBALL", "team"): "football_team_profiles",
}


@dataclass
class SimilarEntity:
    """A similar entity with comparison details."""

    entity_id: int
    entity_name: str
    similarity_score: float
    shared_traits: list[str]
    key_differences: list[str]


class SimilarityCalculator:
    """
    Computes entity similarity based on percentile vectors.

    Uses cosine similarity to find the most statistically similar
    players/teams within the same sport. Compares across ALL players
    in the sport (not restricted by position).

    Designed to run as a batch process after percentile calculation.
    """

    def __init__(self, db: Any):
        """
        Initialize the calculator.

        Args:
            db: Database connection (PostgresDB instance)
        """
        self.db = db

    # =========================================================================
    # Core Similarity Computation
    # =========================================================================

    # Minimum number of common stats required for meaningful similarity
    MIN_COMMON_STATS = 5

    def compute_cosine_similarity(
        self,
        vec1: dict[str, float],
        vec2: dict[str, float],
        min_common_stats: int | None = None,
    ) -> float:
        """
        Compute cosine similarity between two percentile dicts.

        Only uses stats that both entities have (intersection of keys).
        Returns value between 0.0 (completely different) and 1.0 (identical).

        IMPORTANT: Requires a minimum number of common stats to avoid
        misleading high scores from low-dimensional comparisons.
        With only 1-2 common stats, cosine similarity is unreliable.

        Args:
            vec1: First entity's percentiles {stat_name: percentile_value}
            vec2: Second entity's percentiles {stat_name: percentile_value}
            min_common_stats: Minimum common stats required (default: MIN_COMMON_STATS)

        Returns:
            Cosine similarity score (0.0 to 1.0), or 0.0 if insufficient common stats
        """
        min_stats = (
            min_common_stats if min_common_stats is not None else self.MIN_COMMON_STATS
        )

        # Find common stats
        common_keys = set(vec1.keys()) & set(vec2.keys())

        # Require minimum number of common stats for meaningful comparison
        if len(common_keys) < min_stats:
            return 0.0

        # Extract values for common stats
        values1 = [vec1[k] for k in common_keys]
        values2 = [vec2[k] for k in common_keys]

        # Compute dot product
        dot_product = sum(v1 * v2 for v1, v2 in zip(values1, values2))

        # Compute magnitudes
        magnitude1 = math.sqrt(sum(v * v for v in values1))
        magnitude2 = math.sqrt(sum(v * v for v in values2))

        # Avoid division by zero
        if magnitude1 == 0 or magnitude2 == 0:
            return 0.0

        # Cosine similarity
        similarity = dot_product / (magnitude1 * magnitude2)

        # Clamp to [0, 1] (floating point errors can cause slight overflow)
        return max(0.0, min(1.0, round(similarity, 4)))

    def generate_shared_traits(
        self,
        p1: dict[str, float],
        p2: dict[str, float],
        sport_id: str,
        entity_type: str,
        threshold: float = 10.0,
        min_percentile: float = 70.0,
    ) -> list[str]:
        """
        Find stats where both entities are similar AND high performers.

        A shared trait requires:
        - Both entities have the stat
        - Percentile difference is within threshold
        - Both are above min_percentile (high performers)

        Args:
            p1: First entity's percentiles
            p2: Second entity's percentiles
            sport_id: Sport for label lookup
            entity_type: 'player' or 'team'
            threshold: Max percentile difference to be "similar"
            min_percentile: Minimum percentile to be a "high performer"

        Returns:
            List of human-readable trait descriptions
        """
        common_keys = set(p1.keys()) & set(p2.keys())
        traits = []

        for stat in common_keys:
            val1, val2 = p1[stat], p2[stat]
            diff = abs(val1 - val2)
            min_val = min(val1, val2)

            # Both similar AND high performers
            if diff <= threshold and min_val >= min_percentile:
                label = get_stat_label(sport_id, entity_type, stat)

                # Describe the percentile range
                if min_val >= 90:
                    traits.append(f"Elite {label} (90th+ percentile)")
                elif min_val >= 80:
                    traits.append(f"Excellent {label} (80th+ percentile)")
                else:
                    traits.append(f"Strong {label} (70th+ percentile)")

        # Sort by impressiveness (elite first) and limit to top 5
        traits.sort(key=lambda x: (0 if "Elite" in x else 1 if "Excellent" in x else 2))

        return traits[:5]

    def generate_key_differences(
        self,
        p1: dict[str, float],
        p2: dict[str, float],
        name1: str,
        name2: str,
        sport_id: str,
        entity_type: str,
        threshold: float = 20.0,
    ) -> list[str]:
        """
        Find stats with the largest percentile gaps between entities.

        Args:
            p1: First entity's percentiles
            p2: Second entity's percentiles
            name1: First entity's name (for descriptions)
            name2: Second entity's name (for descriptions)
            sport_id: Sport for label lookup
            entity_type: 'player' or 'team'
            threshold: Minimum percentile gap to be a "difference"

        Returns:
            List of human-readable difference descriptions
        """
        common_keys = set(p1.keys()) & set(p2.keys())
        differences = []

        for stat in common_keys:
            val1, val2 = p1[stat], p2[stat]
            diff = val1 - val2  # Positive means entity1 is higher

            if abs(diff) >= threshold:
                label = get_stat_label(sport_id, entity_type, stat)

                if diff > 0:
                    # Entity 1 is higher
                    differences.append((abs(diff), f"{name1}: higher {label}"))
                else:
                    # Entity 2 is higher
                    differences.append((abs(diff), f"{name2}: higher {label}"))

        # Sort by magnitude and take top 3
        differences.sort(key=lambda x: x[0], reverse=True)

        return [d[1] for d in differences[:3]]

    # =========================================================================
    # Data Fetching
    # =========================================================================

    def _fetch_all_percentiles(
        self,
        sport_id: str,
        entity_type: str,
        season_id: int,
    ) -> list[dict[str, Any]]:
        """
        Fetch all entities with their percentiles for a sport/season.

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            entity_type: 'player' or 'team'
            season_id: Season ID

        Returns:
            List of dicts with entity_id, entity_name, percentiles
        """
        stats_table = STATS_TABLE_MAP.get((sport_id, entity_type))
        profile_table = PROFILE_TABLE_MAP.get((sport_id, entity_type))

        if not stats_table or not profile_table:
            logger.warning("No tables configured for %s %s", sport_id, entity_type)
            return []

        id_column = "player_id" if entity_type == "player" else "team_id"
        name_column = "full_name" if entity_type == "player" else "name"

        query = f"""
            SELECT 
                s.{id_column} as entity_id,
                p.{name_column} as entity_name,
                s.percentiles
            FROM {stats_table} s
            JOIN {profile_table} p ON s.{id_column} = p.id
            WHERE s.season_id = %s
              AND s.percentiles IS NOT NULL
              AND s.percentiles != '{{}}'::jsonb
        """

        results = self.db.fetchall(query, (season_id,))

        logger.info(
            "Fetched %d %ss with percentiles for %s (season_id=%d)",
            len(results),
            entity_type,
            sport_id,
            season_id,
        )

        return results

    def _get_season_id(self, sport_id: str, season_year: int) -> int | None:
        """Get season ID from sport and year."""
        result = self.db.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year),
        )
        return result["id"] if result else None

    def _get_season_year(self, season_id: int) -> int | None:
        """Get season year from season ID."""
        result = self.db.fetchone(
            "SELECT season_year FROM seasons WHERE id = %s", (season_id,)
        )
        return result["season_year"] if result else None

    # =========================================================================
    # Batch Similarity Calculation
    # =========================================================================

    def calculate_all_similarities(
        self,
        sport_id: str,
        season_id: int,
        entity_type: str = "player",
        limit: int = 5,
    ) -> int:
        """
        Batch compute similarities for all entities in a sport/season.

        For each entity:
        1. Compute cosine similarity against all other entities
        2. Identify top N most similar
        3. Generate shared_traits and key_differences
        4. Store in entity_similarities table

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_id: Season ID
            entity_type: 'player' or 'team'
            limit: Number of similar entities to store per entity

        Returns:
            Number of entities processed
        """
        # Fetch all entities with percentiles
        entities = self._fetch_all_percentiles(sport_id, entity_type, season_id)

        if len(entities) < 2:
            logger.warning(
                "Not enough entities with percentiles for %s %s (found %d)",
                sport_id,
                entity_type,
                len(entities),
            )
            return 0

        # Get season year for storage
        season_year = self._get_season_year(season_id)
        season_str = str(season_year) if season_year else str(season_id)

        # Build entity lookup for quick access
        entity_lookup = {
            e["entity_id"]: {
                "name": e["entity_name"],
                "percentiles": e["percentiles"] or {},
            }
            for e in entities
        }

        # Clear existing similarities for this sport/season/entity_type
        self.db.execute(
            """
            DELETE FROM entity_similarities 
            WHERE sport = %s AND season = %s AND entity_type = %s
            """,
            (sport_id, season_str, entity_type),
        )

        # Process each entity
        processed_count = 0
        batch_inserts = []

        for entity in entities:
            entity_id = entity["entity_id"]
            entity_name = entity["entity_name"]
            entity_percentiles = entity["percentiles"] or {}

            if not entity_percentiles:
                continue

            # Compute similarity against all other entities
            similarities: list[tuple[int, str, float, dict]] = []

            for other_id, other_data in entity_lookup.items():
                if other_id == entity_id:
                    continue

                other_percentiles = other_data["percentiles"]
                if not other_percentiles:
                    continue

                score = self.compute_cosine_similarity(
                    entity_percentiles, other_percentiles
                )

                similarities.append(
                    (
                        other_id,
                        other_data["name"],
                        score,
                        other_percentiles,
                    )
                )

            # Sort by similarity (highest first) and take top N
            similarities.sort(key=lambda x: x[2], reverse=True)
            top_similar = similarities[:limit]

            # Generate traits and differences for each similar entity
            for rank, (
                similar_id,
                similar_name,
                score,
                similar_percentiles,
            ) in enumerate(top_similar, 1):
                shared_traits = self.generate_shared_traits(
                    entity_percentiles,
                    similar_percentiles,
                    sport_id,
                    entity_type,
                )

                key_differences = self.generate_key_differences(
                    entity_percentiles,
                    similar_percentiles,
                    entity_name,
                    similar_name,
                    sport_id,
                    entity_type,
                )

                batch_inserts.append(
                    (
                        entity_type,
                        entity_id,
                        entity_name,
                        similar_id,
                        similar_name,
                        sport_id,
                        season_str,
                        score,
                        shared_traits,
                        key_differences,
                        rank,
                    )
                )

            processed_count += 1

            # Batch insert every 100 entities to manage memory
            if len(batch_inserts) >= 500:
                self._batch_insert_similarities(batch_inserts)
                batch_inserts = []

        # Insert remaining
        if batch_inserts:
            self._batch_insert_similarities(batch_inserts)

        logger.info(
            "Computed similarities for %d %ss in %s (season=%s, limit=%d)",
            processed_count,
            entity_type,
            sport_id,
            season_str,
            limit,
        )

        return processed_count

    def _batch_insert_similarities(self, batch: list[tuple]) -> None:
        """Insert a batch of similarity records."""
        if not batch:
            return

        self.db.executemany(
            """
            INSERT INTO entity_similarities (
                entity_type, entity_id, entity_name,
                similar_entity_id, similar_entity_name,
                sport, season, similarity_score,
                shared_traits, key_differences, rank,
                computed_at
            ) VALUES (
                %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, NOW()
            )
            ON CONFLICT (entity_type, entity_id, similar_entity_id, sport, season)
            DO UPDATE SET
                entity_name = EXCLUDED.entity_name,
                similar_entity_name = EXCLUDED.similar_entity_name,
                similarity_score = EXCLUDED.similarity_score,
                shared_traits = EXCLUDED.shared_traits,
                key_differences = EXCLUDED.key_differences,
                rank = EXCLUDED.rank,
                computed_at = NOW()
            """,
            batch,
        )

        logger.debug("Inserted %d similarity records", len(batch))

    # =========================================================================
    # Single Entity Lookup (for API)
    # =========================================================================

    def get_similar_entities(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        season_year: int | None = None,
        limit: int = 5,
    ) -> list[SimilarEntity]:
        """
        Get pre-computed similar entities for a single entity.

        This is the fast lookup used by the API endpoint.

        Args:
            entity_type: 'player' or 'team'
            entity_id: Entity ID
            sport_id: Sport identifier
            season_year: Season year (uses current if not provided)
            limit: Max number of similar entities to return

        Returns:
            List of SimilarEntity objects
        """
        # Build season filter
        season_filter = ""
        params: list[Any] = [entity_type, entity_id, sport_id]

        if season_year:
            season_filter = "AND season = %s"
            params.append(str(season_year))

        params.append(limit)

        query = f"""
            SELECT 
                similar_entity_id,
                similar_entity_name,
                similarity_score,
                shared_traits,
                key_differences
            FROM entity_similarities
            WHERE entity_type = %s 
              AND entity_id = %s 
              AND sport = %s
              {season_filter}
            ORDER BY rank
            LIMIT %s
        """

        rows = self.db.fetchall(query, tuple(params))

        return [
            SimilarEntity(
                entity_id=row["similar_entity_id"],
                entity_name=row["similar_entity_name"],
                similarity_score=row["similarity_score"],
                shared_traits=row["shared_traits"] or [],
                key_differences=row["key_differences"] or [],
            )
            for row in rows
        ]

    # =========================================================================
    # Main Entry Point for Batch Processing
    # =========================================================================

    def calculate_all_for_sport(
        self,
        sport_id: str,
        season_year: int,
        limit: int = 5,
    ) -> dict[str, int]:
        """
        Calculate all similarities for a sport/season.

        This is the main entry point for batch processing.
        Should be called after percentile calculation completes.

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Season year
            limit: Number of similar entities to store per entity

        Returns:
            Dict with player and team counts
        """
        season_id = self._get_season_id(sport_id, season_year)
        if not season_id:
            logger.warning("No season found for %s %d", sport_id, season_year)
            return {"players": 0, "teams": 0}

        logger.info("Starting similarity calculation for %s %d", sport_id, season_year)

        # Calculate player similarities
        player_count = self.calculate_all_similarities(
            sport_id, season_id, "player", limit
        )

        # Calculate team similarities
        team_count = self.calculate_all_similarities(sport_id, season_id, "team", limit)

        logger.info(
            "Completed similarity calculation for %s %d: %d players, %d teams",
            sport_id,
            season_year,
            player_count,
            team_count,
        )

        return {"players": player_count, "teams": team_count}
