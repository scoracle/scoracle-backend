"""
Stats service — sport-aware entity statistics lookups.

Encapsulates all raw SQL for stats queries, using psycopg.sql for safe
dynamic table/column injection. Routers call this instead of building SQL.
"""

import logging
from typing import Any

from psycopg import sql

from ..core.types import (
    PLAYER_STATS_TABLES,
    TEAM_STATS_TABLES,
)
from ..percentiles.config import SMALL_SAMPLE_WARNING_THRESHOLD

logger = logging.getLogger(__name__)


def get_entity_stats(
    db,
    sport: str,
    entity_type: str,
    entity_id: int,
    season_id: int,
    league_id: int | None = None,
) -> dict[str, Any] | None:
    """Fetch stats from a sport-specific table with embedded percentiles.

    Args:
        db: Database connection (psycopg-style with fetchone).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        entity_type: "player" or "team".
        entity_id: The entity's ID.
        season_id: The season row ID (not year).
        league_id: Optional league ID (used for FOOTBALL).

    Returns:
        Dict with keys ``stats``, ``percentiles``, ``percentile_metadata``,
        or None if no row found.
    """
    table_map = PLAYER_STATS_TABLES if entity_type == "player" else TEAM_STATS_TABLES
    table = table_map.get(sport)
    if not table:
        return None

    id_column = "player_id" if entity_type == "player" else "team_id"

    tbl = sql.Identifier(table)
    col = sql.Identifier(id_column)

    if sport == "FOOTBALL" and league_id:
        query = sql.SQL(
            "SELECT * FROM {} WHERE {} = %s AND season_id = %s AND league_id = %s"
        ).format(tbl, col)
        row = db.fetchone(query, (entity_id, season_id, league_id))
    else:
        query = sql.SQL(
            "SELECT * FROM {} WHERE {} = %s AND season_id = %s"
        ).format(tbl, col)
        row = db.fetchone(query, (entity_id, season_id))

    if not row:
        return None

    data = dict(row)

    # Extract percentile data (stored as JSONB in stats table)
    percentiles = data.pop("percentiles", None) or {}
    percentile_position_group = data.pop("percentile_position_group", None)
    percentile_sample_size = data.pop("percentile_sample_size", None)

    # Remove internal IDs and metadata
    for key in ["id", "player_id", "team_id", "season_id", "league_id", "updated_at"]:
        data.pop(key, None)

    # Filter out null and zero values (frontend only needs non-zero stats)
    stats = {k: v for k, v in data.items() if v is not None and v != 0}

    # Build percentile metadata (only if percentiles exist)
    percentile_metadata = None
    if percentiles:
        sample_size = percentile_sample_size or 0
        percentile_metadata = {
            "position_group": percentile_position_group,
            "sample_size": sample_size,
            "small_sample_warning": sample_size < SMALL_SAMPLE_WARNING_THRESHOLD,
        }

    return {
        "stats": stats,
        "percentiles": percentiles,
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
    table_map = PLAYER_STATS_TABLES if entity_type == "player" else TEAM_STATS_TABLES
    table = table_map.get(sport)
    if not table:
        return []

    id_column = "player_id" if entity_type == "player" else "team_id"

    tbl = sql.Identifier(table)
    col = sql.Identifier(id_column)

    query = sql.SQL("""
        SELECT DISTINCT s.season_year
        FROM {} st
        JOIN seasons s ON s.id = st.season_id
        WHERE st.{} = %s AND s.sport_id = %s
        ORDER BY s.season_year DESC
    """).format(tbl, col)

    rows = db.fetchall(query, (entity_id, sport))
    return [row["season_year"] for row in rows]
