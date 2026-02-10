"""
Stats service — sport-aware entity statistics lookups.

Queries the unified player_stats/team_stats tables with JSONB stats.
Routers call this instead of building SQL.
"""

import logging
from typing import Any

from ..core.types import PLAYER_STATS_TABLE, TEAM_STATS_TABLE
from ..percentiles.config import SMALL_SAMPLE_WARNING_THRESHOLD

logger = logging.getLogger(__name__)


def get_entity_stats(
    db,
    sport: str,
    entity_type: str,
    entity_id: int,
    season: int,
    league_id: int | None = None,
) -> dict[str, Any] | None:
    """Fetch stats + percentiles from the unified stats table.

    Stats and percentiles are stored as JSONB columns in the unified
    player_stats / team_stats tables.

    Args:
        db: Database connection (psycopg-style with fetchone).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        entity_type: "player" or "team".
        entity_id: The entity's ID.
        season: Season year (e.g. 2025).
        league_id: Optional league ID (used for FOOTBALL).

    Returns:
        Dict with keys ``stats``, ``percentiles``, ``percentile_metadata``,
        or None if no row found.
    """
    table = PLAYER_STATS_TABLE if entity_type == "player" else TEAM_STATS_TABLE
    id_column = "player_id" if entity_type == "player" else "team_id"

    if sport == "FOOTBALL" and league_id:
        query = (
            f"SELECT stats, percentiles FROM {table} "
            f"WHERE {id_column} = %s AND sport = %s AND season = %s AND league_id = %s"
        )
        row = db.fetchone(query, (entity_id, sport, season, league_id))
    else:
        query = (
            f"SELECT stats, percentiles FROM {table} "
            f"WHERE {id_column} = %s AND sport = %s AND season = %s"
        )
        row = db.fetchone(query, (entity_id, sport, season))

    if not row:
        return None

    # Stats and percentiles come directly from JSONB columns
    stats = row["stats"] or {}
    percentiles_raw = row["percentiles"] or {}

    # Separate embedded metadata from percentile values
    position_group = (
        percentiles_raw.pop("_position_group", None)
        if isinstance(percentiles_raw, dict)
        else None
    )
    sample_size = (
        percentiles_raw.pop("_sample_size", None)
        if isinstance(percentiles_raw, dict)
        else None
    )

    # Filter out null and zero stat values (frontend only needs non-zero stats)
    stats = {k: v for k, v in stats.items() if v is not None and v != 0}

    # Build percentile metadata (only if percentiles exist)
    percentile_metadata = None
    if percentiles_raw:
        sz = sample_size or 0
        percentile_metadata = {
            "position_group": position_group,
            "sample_size": sz,
            "small_sample_warning": sz < SMALL_SAMPLE_WARNING_THRESHOLD,
        }

    return {
        "stats": stats,
        "percentiles": percentiles_raw,
        "percentile_metadata": percentile_metadata,
    }


def get_available_seasons(
    db,
    sport: str,
    entity_type: str,
    entity_id: int,
) -> list[int]:
    """Return season years with stats for an entity, newest first.

    Args:
        db: Database connection (psycopg-style with fetchall).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        entity_type: "player" or "team".
        entity_id: The entity's ID.

    Returns:
        List of season years in descending order.
    """
    table = PLAYER_STATS_TABLE if entity_type == "player" else TEAM_STATS_TABLE
    id_column = "player_id" if entity_type == "player" else "team_id"

    query = (
        f"SELECT DISTINCT season FROM {table} "
        f"WHERE {id_column} = %s AND sport = %s "
        f"ORDER BY season DESC"
    )
    rows = db.fetchall(query, (entity_id, sport))
    return [row["season"] for row in rows]
