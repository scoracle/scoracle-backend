"""
Stats service — sport-aware entity statistics lookups.

Queries Postgres views and functions for stats, stat leaders, and standings.
Python is a thin pass-through — Postgres owns all data shaping and ranking.

All functions are async — they accept an AsyncPostgresDB and await queries.

Postgres objects used:
- player_stats / team_stats tables (entity stats lookup)
- fn_stat_leaders() (ranked stat leaders with LATERAL + ROW_NUMBER)
- fn_standings() (unified standings with win_pct computation)
"""

import logging
from typing import Any

from ..core.types import PLAYER_STATS_TABLE, TEAM_STATS_TABLE

logger = logging.getLogger(__name__)


def _resolve_table(entity_type: str) -> tuple[str, str]:
    """Return (stats_table, id_column) for an entity type."""
    if entity_type == "player":
        return PLAYER_STATS_TABLE, "player_id"
    return TEAM_STATS_TABLE, "team_id"


async def get_entity_stats(
    db,
    sport: str,
    entity_type: str,
    entity_id: int,
    season: int,
    league_id: int | None = None,
) -> dict[str, Any] | None:
    """Fetch stats + percentiles from the unified stats table.

    Stats and percentiles are stored as JSONB columns. Percentile metadata
    (_position_group, _sample_size) is extracted in SQL. Null values pass
    through to the frontend — filtering is a presentation concern.

    Args:
        db: Async database connection (AsyncPostgresDB).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        entity_type: "player" or "team".
        entity_id: The entity's ID.
        season: Season year (e.g. 2025).
        league_id: Optional league ID (used for FOOTBALL).

    Returns:
        Dict with keys ``stats``, ``percentiles``, ``percentile_metadata``,
        or None if no row found.
    """
    table, id_column = _resolve_table(entity_type)

    base_query = f"""
        SELECT
            stats,
            percentiles - '_position_group' - '_sample_size' AS percentiles,
            percentiles->>'_position_group' AS position_group,
            (percentiles->>'_sample_size')::int AS sample_size
        FROM {table}
        WHERE {id_column} = %s AND sport = %s AND season = %s
    """

    if sport == "FOOTBALL" and league_id:
        query = base_query + " AND league_id = %s"
        row = await db.fetchone(query, (entity_id, sport, season, league_id))
    else:
        row = await db.fetchone(base_query, (entity_id, sport, season))

    if not row:
        return None

    percentiles = row["percentiles"] or {}
    percentile_metadata = None
    if percentiles:
        percentile_metadata = {
            "position_group": row["position_group"],
            "sample_size": row["sample_size"] or 0,
        }

    return {
        "stats": row["stats"] or {},
        "percentiles": percentiles,
        "percentile_metadata": percentile_metadata,
    }


async def get_available_seasons(
    db,
    sport: str,
    entity_type: str,
    entity_id: int,
) -> list[int]:
    """Return season years with stats for an entity, newest first.

    Args:
        db: Async database connection (AsyncPostgresDB).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        entity_type: "player" or "team".
        entity_id: The entity's ID.

    Returns:
        List of season years in descending order.
    """
    table, id_column = _resolve_table(entity_type)

    query = (
        f"SELECT DISTINCT season FROM {table} "
        f"WHERE {id_column} = %s AND sport = %s "
        f"ORDER BY season DESC"
    )
    rows = await db.fetchall(query, (entity_id, sport))
    return [row["season"] for row in rows]


# =========================================================================
# Stat Leaders — delegates to fn_stat_leaders() Postgres function
# =========================================================================


async def get_stat_leaders(
    db,
    sport: str,
    season: int,
    stat_name: str,
    limit: int = 25,
    position: str | None = None,
    league_id: int = 0,
) -> list[dict[str, Any]]:
    """Get top players for a specific stat.

    Delegates entirely to the fn_stat_leaders() Postgres function which
    uses LATERAL join for efficient JSONB extraction and ROW_NUMBER()
    for ranking.

    Args:
        db: Async database connection (AsyncPostgresDB).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        season: Season year.
        stat_name: Stat key within the JSONB stats column.
        limit: Number of results.
        position: Optional position filter.
        league_id: League filter (0 for NBA/NFL, >0 for football).

    Returns:
        List of player stats ranked by the stat.
    """
    rows = await db.fetchall(
        "SELECT * FROM fn_stat_leaders(%s, %s, %s, %s, %s, %s)",
        (sport, season, stat_name, limit, position, league_id),
    )
    return [dict(row) for row in rows]


# =========================================================================
# Standings — delegates to fn_standings() Postgres function
# =========================================================================


async def get_standings(
    db,
    sport: str,
    season: int,
    league_id: int = 0,
    conference: str | None = None,
) -> list[dict[str, Any]]:
    """Get team standings.

    Delegates entirely to the fn_standings() Postgres function which
    handles sport-conditional ordering, ROW_NUMBER() for ranking,
    and win_pct computation for NBA/NFL.

    Args:
        db: Async database connection (AsyncPostgresDB).
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        season: Season year.
        league_id: League filter (0 for NBA/NFL, >0 for football).
        conference: Optional conference filter (NBA/NFL only).

    Returns:
        List of teams with standings info, ranked.
    """
    rows = await db.fetchall(
        "SELECT * FROM fn_standings(%s, %s, %s, %s)",
        (sport, season, league_id, conference),
    )
    return [dict(row) for row in rows]


# =========================================================================
# Stat Definitions — queries stat_definitions table
# =========================================================================


async def get_stat_definitions(
    db,
    sport: str,
) -> list[dict[str, Any]]:
    """Get canonical stat definitions for a sport.

    Returns display names, categories, sort orders, and flags (is_inverse,
    is_derived, is_percentile_eligible) from the stat_definitions table.
    Useful for the frontend to label and order stat displays.

    Args:
        db: Async database connection (AsyncPostgresDB).
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        List of stat definition dicts, ordered by sort_order.
    """
    rows = await db.fetchall(
        "SELECT * FROM stat_definitions WHERE sport = %s ORDER BY sort_order",
        (sport,),
    )
    return [dict(row) for row in rows]
