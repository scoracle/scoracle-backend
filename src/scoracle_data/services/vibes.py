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
    db: "PostgresDB",
    entity_type: str,
    entity_id: int,
    sport: str | None = None,
) -> str:
    """Get entity name from sport-specific profile tables.

    If ``sport`` is provided, queries only that sport's table (faster).
    Otherwise iterates all sport tables until a match is found.

    Returns:
        The entity name, or ``"Unknown"`` if not found.
    """
    if entity_type == "player":
        tables = PLAYER_PROFILE_TABLES
        name_col = "full_name"
    else:
        tables = TEAM_PROFILE_TABLES
        name_col = "name"

    # If sport is known, query only that table
    if sport:
        table = tables.get(sport)
        if not table:
            return "Unknown"
        row = db.fetchone(
            f"SELECT {name_col} FROM {table} WHERE id = %s",
            (entity_id,),
        )
        return row[name_col] if row and row[name_col] else "Unknown"

    # Otherwise iterate all sport tables
    for _sport_id, table in tables.items():
        row = db.fetchone(
            f"SELECT {name_col} FROM {table} WHERE id = %s",
            (entity_id,),
        )
        if row and row[name_col]:
            return row[name_col]

    return "Unknown"
