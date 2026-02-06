"""
Stats router - serves entity statistics for frontend widgets.

Endpoints:
- GET /{entity_type}/{entity_id} - Stats + percentiles from sport-specific stats tables
- GET /{entity_type}/{entity_id}/seasons - Available seasons for an entity

Data:
- Serves statistics from stats tables (nba_player_stats, nba_team_stats, etc.)
- Percentiles are embedded as JSONB in stats tables (no separate query needed)

Performance Features:
- In-memory caching with TTLs
- ETag support for conditional requests (304 Not Modified)
- Season ID caching to eliminate redundant lookups
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_CURRENT_SEASON
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ..types import (
    EntityType,
    Sport,
    PLAYER_STATS_TABLES,
    TEAM_STATS_TABLES,
)
from ._utils import (
    get_season_id,
    validate_season,
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
    get_stats_ttl,
)
from ...percentiles.config import SMALL_SAMPLE_WARNING_THRESHOLD

logger = logging.getLogger(__name__)

router = APIRouter()


@router.get("/{entity_type}/{entity_id}", response_model=None)
async def get_entity_stats(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int | None, Query(description="Season year (defaults to current)")] = None,
    league_id: Annotated[int | None, Query(description="League ID (for FOOTBALL)")] = None,
    response: Response = None,
    db: DBDependency = None,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get entity statistics for a season.

    Returns all stat fields from the appropriate stats table.
    Season defaults to current season if not specified.

    Supports conditional requests via If-None-Match header.
    """
    # Validate and normalize season
    season = validate_season(season, sport.value)

    cache = get_cache()
    cache_key = ("stats", entity_type.value, entity_id, sport.value, season, league_id)
    ttl = get_stats_ttl(sport.value, season)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, ttl, cache_hit=True)
        set_etag_headers(response, etag, ttl)
        return cached

    # Get season ID (cached lookup)
    season_id = get_season_id(db, sport.value, season)
    if not season_id:
        raise NotFoundError(resource="Season", identifier=season, context=sport.value)

    # Get stats (includes percentiles as JSONB)
    if entity_type == EntityType.player:
        stats_data = _get_stats(db, PLAYER_STATS_TABLES, sport.value, "player_id", entity_id, season_id, league_id)
    else:
        stats_data = _get_stats(db, TEAM_STATS_TABLES, sport.value, "team_id", entity_id, season_id, league_id)

    if not stats_data:
        raise NotFoundError(
            resource=f"{entity_type.value.title()} stats",
            identifier=entity_id,
            context=f"{sport.value} {season}",
        )

    result = {
        "entity_id": entity_id,
        "entity_type": entity_type.value,
        "sport": sport.value,
        "season": season,
        "stats": stats_data["stats"],
        "percentiles": stats_data["percentiles"],
        "percentile_metadata": stats_data["percentile_metadata"],
    }
    if league_id:
        result["league_id"] = league_id

    cache.set(result, *cache_key, ttl=ttl)
    etag = compute_etag(result)
    set_cache_headers(response, ttl, cache_hit=False)
    set_etag_headers(response, etag, ttl)
    return result


def _get_stats(
    db,
    table_map: dict[str, str],
    sport: str,
    id_column: str,
    entity_id: int,
    season_id: int,
    league_id: int | None,
) -> dict[str, Any] | None:
    """
    Fetch stats from sport-specific table.

    Returns a dict with:
    - stats: All non-zero stat values
    - percentiles: JSONB percentile data (if available)
    - percentile_metadata: Position group and sample size
    """
    from psycopg import sql

    table = table_map.get(sport)
    if not table:
        return None

    tbl = sql.Identifier(table)
    col = sql.Identifier(id_column)

    if sport == "FOOTBALL" and league_id:
        query = sql.SQL("SELECT * FROM {} WHERE {} = %s AND season_id = %s AND league_id = %s").format(tbl, col)
        row = db.fetchone(query, (entity_id, season_id, league_id))
    else:
        query = sql.SQL("SELECT * FROM {} WHERE {} = %s AND season_id = %s").format(tbl, col)
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


@router.get("/{entity_type}/{entity_id}/seasons", response_model=None)
async def get_available_seasons(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response = None,
    db: DBDependency = None,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get list of seasons with stats available for an entity.

    Useful for building season dropdown selectors.
    Supports conditional requests via If-None-Match header.
    """
    cache = get_cache()
    cache_key = ("seasons_available", entity_type.value, entity_id, sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, TTL_CURRENT_SEASON, cache_hit=True)
        set_etag_headers(response, etag, TTL_CURRENT_SEASON)
        return cached

    table_map = PLAYER_STATS_TABLES if entity_type == EntityType.player else TEAM_STATS_TABLES
    table = table_map.get(sport.value)
    id_column = "player_id" if entity_type == EntityType.player else "team_id"

    rows = db.fetchall(
        f"""
        SELECT DISTINCT s.season_year
        FROM {table} st
        JOIN seasons s ON s.id = st.season_id
        WHERE st.{id_column} = %s AND s.sport_id = %s
        ORDER BY s.season_year DESC
        """,
        (entity_id, sport.value),
    )

    result = {
        "entity_id": entity_id,
        "entity_type": entity_type.value,
        "sport": sport.value,
        "seasons": [row["season_year"] for row in rows],
    }

    cache.set(result, *cache_key, ttl=TTL_CURRENT_SEASON)
    etag = compute_etag(result)
    set_cache_headers(response, TTL_CURRENT_SEASON, cache_hit=False)
    set_etag_headers(response, etag, TTL_CURRENT_SEASON)
    return result
