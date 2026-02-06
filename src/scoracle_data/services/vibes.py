"""
Vibe service — SQL queries for vibe score endpoints.

Encapsulates all raw SQL for vibe-related queries.
Routers call these functions instead of building SQL inline.
"""

from __future__ import annotations

import logging
from datetime import datetime
from typing import TYPE_CHECKING, Any

from ..core.types import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


def get_latest_vibe(
    db: "PostgresDB", entity_type: str, entity_id: int
) -> dict[str, Any] | None:
    """Get the most recent vibe score for an entity.

    Returns:
        A dict with keys: entity_name, sport, overall_score,
        twitter_score, twitter_sample_size, news_score, news_sample_size,
        reddit_score, reddit_sample_size, positive_themes, negative_themes,
        calculated_at.  Returns None if no vibe data exists.
    """
    return db.fetchone(
        """
        SELECT
            vs.entity_name, vs.sport, vs.overall_score,
            vs.twitter_score, vs.twitter_sample_size,
            vs.news_score, vs.news_sample_size,
            vs.reddit_score, vs.reddit_sample_size,
            vs.positive_themes, vs.negative_themes,
            vs.calculated_at
        FROM vibe_scores vs
        WHERE vs.entity_type = %s AND vs.entity_id = %s
        ORDER BY vs.calculated_at DESC
        LIMIT 1
        """,
        (entity_type, entity_id),
    )


def get_previous_vibe(
    db: "PostgresDB",
    entity_type: str,
    entity_id: int,
    before_timestamp: datetime,
) -> dict[str, Any] | None:
    """Get the vibe score immediately before a given timestamp.

    Used to compute trend / change vs. the latest score.

    Returns:
        A dict with key 'overall_score', or None.
    """
    return db.fetchone(
        """
        SELECT overall_score
        FROM vibe_scores
        WHERE entity_type = %s AND entity_id = %s
        AND calculated_at < %s
        ORDER BY calculated_at DESC
        LIMIT 1
        """,
        (entity_type, entity_id, before_timestamp),
    )


def get_trending_vibes(
    db: "PostgresDB", sport: str, limit: int = 10
) -> list[dict[str, Any]]:
    """Get entities with the biggest vibe-score changes over the past 7 days.

    Uses a CTE to pair each entity's latest score against its score from
    7+ days ago, then ranks by absolute change.

    Returns:
        List of dicts with keys: entity_id, entity_name, entity_type,
        overall_score, change.
    """
    return db.fetchall(
        """
        WITH latest_scores AS (
            SELECT DISTINCT ON (entity_type, entity_id)
                entity_id, entity_name, entity_type, overall_score, calculated_at
            FROM vibe_scores
            WHERE LOWER(sport) = LOWER(%s)
            ORDER BY entity_type, entity_id, calculated_at DESC
        ),
        previous_scores AS (
            SELECT DISTINCT ON (entity_type, entity_id)
                entity_id, entity_type, overall_score
            FROM vibe_scores
            WHERE LOWER(sport) = LOWER(%s)
            AND calculated_at < NOW() - INTERVAL '7 days'
            ORDER BY entity_type, entity_id, calculated_at DESC
        )
        SELECT
            l.entity_id, l.entity_name, l.entity_type,
            l.overall_score, (l.overall_score - COALESCE(p.overall_score, l.overall_score)) as change
        FROM latest_scores l
        LEFT JOIN previous_scores p ON l.entity_id = p.entity_id AND l.entity_type = p.entity_type
        ORDER BY ABS(l.overall_score - COALESCE(p.overall_score, l.overall_score)) DESC
        LIMIT %s
        """,
        (sport, sport, limit),
    )


def get_entity_name(
    db: "PostgresDB", entity_type: str, entity_id: int
) -> str:
    """Get entity name from sport-specific profile tables.

    Unlike the original helper that queried non-existent generic 'players' /
    'teams' tables, this iterates over the sport-specific profile tables
    defined in PLAYER_PROFILE_TABLES and TEAM_PROFILE_TABLES.

    Returns:
        The entity name, or ``"Unknown"`` if not found.
    """
    if entity_type == "player":
        for _sport_id, profile_table in PLAYER_PROFILE_TABLES.items():
            row = db.fetchone(
                f"""
                SELECT COALESCE(full_name, first_name || ' ' || last_name) as name
                FROM {profile_table}
                WHERE id = %s
                """,
                (entity_id,),
            )
            if row:
                return row["name"] or "Unknown"
    else:
        for _sport_id, team_table in TEAM_PROFILE_TABLES.items():
            row = db.fetchone(
                f"SELECT name FROM {team_table} WHERE id = %s",
                (entity_id,),
            )
            if row:
                return row["name"] or "Unknown"

    return "Unknown"
