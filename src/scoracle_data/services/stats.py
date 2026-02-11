"""
Stats service — sport-aware entity statistics lookups.

Queries the unified player_stats/team_stats tables with JSONB stats.
Routers and CLI call this instead of building SQL.

Also provides stat_leaders and standings queries (previously in queries/ module).
"""

import logging
from typing import Any

from ..core.types import (
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
)
from ..percentiles.config import SMALL_SAMPLE_WARNING_THRESHOLD

logger = logging.getLogger(__name__)


def _resolve_table(entity_type: str) -> tuple[str, str]:
    """Return (stats_table, id_column) for an entity type."""
    if entity_type == "player":
        return PLAYER_STATS_TABLE, "player_id"
    return TEAM_STATS_TABLE, "team_id"


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
    player_stats / team_stats tables.  Percentile metadata (_position_group,
    _sample_size) is extracted in SQL so Python never mutates the dict.

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
    table, id_column = _resolve_table(entity_type)

    # Extract percentile metadata in SQL rather than popping keys in Python
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
        row = db.fetchone(query, (entity_id, sport, season, league_id))
    else:
        row = db.fetchone(base_query, (entity_id, sport, season))

    if not row:
        return None

    # Stats come directly from JSONB; filter out null/zero for frontend
    stats = row["stats"] or {}
    stats = {k: v for k, v in stats.items() if v is not None and v != 0}

    percentiles = row["percentiles"] or {}
    position_group = row["position_group"]
    sample_size = row["sample_size"]

    # Build percentile metadata (only if percentiles exist)
    percentile_metadata = None
    if percentiles:
        sz = sample_size or 0
        percentile_metadata = {
            "position_group": position_group,
            "sample_size": sz,
            "small_sample_warning": sz < SMALL_SAMPLE_WARNING_THRESHOLD,
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
    table, id_column = _resolve_table(entity_type)

    query = (
        f"SELECT DISTINCT season FROM {table} "
        f"WHERE {id_column} = %s AND sport = %s "
        f"ORDER BY season DESC"
    )
    rows = db.fetchall(query, (entity_id, sport))
    return [row["season"] for row in rows]


# =========================================================================
# Stat Leaders (moved from queries/players.py, with SQL injection fix)
# =========================================================================


def get_stat_leaders(
    db,
    sport: str,
    season: int,
    stat_name: str,
    limit: int = 25,
    position: str | None = None,
    league_id: int = 0,
) -> list[dict[str, Any]]:
    """Get top players for a specific stat (from JSONB).

    Uses parameterized queries for the stat_name JSONB key to prevent
    SQL injection (the previous queries/ module used f-string interpolation).

    Args:
        db: Database connection.
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        season: Season year.
        stat_name: Stat key within the JSONB stats column.
        limit: Number of results.
        position: Optional position filter.
        league_id: League filter (0 for NBA/NFL, >0 for football).

    Returns:
        List of player stats ranked by the stat, with rank added.
    """
    conditions = [
        "s.sport = %s",
        "s.season = %s",
        "s.league_id = %s",
        "(s.stats->>%s) IS NOT NULL",
    ]
    params: list[Any] = [sport, season, league_id, stat_name]

    if position:
        conditions.append("p.position = %s")
        params.append(position)

    # stat_name is passed as a parameter — safe from injection
    query = f"""
        SELECT
            p.id AS player_id,
            p.name,
            p.position,
            t.name AS team_name,
            (s.stats->>%s)::NUMERIC AS stat_value
        FROM {PLAYER_STATS_TABLE} s
        JOIN {PLAYERS_TABLE} p ON s.player_id = p.id AND s.sport = p.sport
        LEFT JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
        WHERE {" AND ".join(conditions)}
        ORDER BY (s.stats->>%s)::NUMERIC DESC
        LIMIT %s
    """
    params.extend([stat_name, stat_name, limit])

    rows = db.fetchall(query, tuple(params))
    return [{**row, "rank": i + 1} for i, row in enumerate(rows)]


# =========================================================================
# Standings (moved from queries/teams.py)
# =========================================================================


def get_standings(
    db,
    sport: str,
    season: int,
    league_id: int = 0,
    conference: str | None = None,
) -> list[dict[str, Any]]:
    """Get team standings from JSONB stats.

    Now uses the typed conference column instead of extracting from meta JSONB.

    Args:
        db: Database connection.
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        season: Season year.
        league_id: League filter (0 for NBA/NFL, >0 for football).
        conference: Optional conference filter (NBA/NFL only).

    Returns:
        List of teams with standings info, ranked.
    """
    if sport == "FOOTBALL":
        query = f"""
            SELECT
                t.id,
                t.name,
                t.logo_url,
                l.name AS league_name,
                s.stats
            FROM {TEAM_STATS_TABLE} s
            JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
            LEFT JOIN leagues l ON s.league_id = l.id
            WHERE s.sport = %s AND s.season = %s AND s.league_id = %s
            ORDER BY
                (s.stats->>'points')::INTEGER DESC NULLS LAST,
                (s.stats->>'goal_difference')::INTEGER DESC NULLS LAST,
                (s.stats->>'goals_for')::INTEGER DESC NULLS LAST
        """
        params: tuple = (sport, season, league_id)
    else:
        # NBA/NFL — use typed conference column for filtering
        conditions = ["s.sport = %s", "s.season = %s", "s.league_id = %s"]
        param_list: list[Any] = [sport, season, league_id]

        if conference:
            conditions.append("t.conference = %s")
            param_list.append(conference)

        query = f"""
            SELECT
                t.id,
                t.name,
                t.logo_url,
                t.conference,
                t.division,
                s.stats
            FROM {TEAM_STATS_TABLE} s
            JOIN {TEAMS_TABLE} t ON s.team_id = t.id AND s.sport = t.sport
            WHERE {" AND ".join(conditions)}
            ORDER BY
                (s.stats->>'wins')::INTEGER DESC NULLS LAST
        """
        params = tuple(param_list)

    rows = db.fetchall(query, params)
    return [{**row, "rank": i + 1} for i, row in enumerate(rows)]
