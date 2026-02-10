"""
Stats router - serves entity statistics for frontend widgets.

Endpoints:
- GET /{entity_type}/{entity_id} - Stats + percentiles from unified stats tables
- GET /{entity_type}/{entity_id}/seasons - Available seasons for an entity

Data:
- Serves statistics from unified tables (player_stats, team_stats) with sport filter
- Percentiles are embedded as JSONB in stats tables (no separate query needed)

Performance Features:
- In-memory caching with TTLs
- ETag support for conditional requests (304 Not Modified)
- Season validation to eliminate redundant lookups
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_CURRENT_SEASON
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ...core.types import EntityType, Sport
from ._utils import (
    get_season_id,
    validate_season,
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
    get_stats_ttl,
)
from ...services.stats import get_entity_stats, get_available_seasons

logger = logging.getLogger(__name__)

router = APIRouter()


@router.get("/{entity_type}/{entity_id}", response_model=None)
async def get_entity_stats_endpoint(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    season: Annotated[
        int | None, Query(description="Season year (defaults to current)")
    ] = None,
    league_id: Annotated[
        int | None, Query(description="League ID (for FOOTBALL)")
    ] = None,
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

    # Get stats via service layer
    stats_data = get_entity_stats(
        db,
        sport.value,
        entity_type.value,
        entity_id,
        season_id,
        league_id,
    )

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


@router.get("/{entity_type}/{entity_id}/seasons", response_model=None)
async def get_available_seasons_endpoint(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
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

    seasons = get_available_seasons(db, sport.value, entity_type.value, entity_id)

    result = {
        "entity_id": entity_id,
        "entity_type": entity_type.value,
        "sport": sport.value,
        "seasons": seasons,
    }

    cache.set(result, *cache_key, ttl=TTL_CURRENT_SEASON)
    etag = compute_etag(result)
    set_cache_headers(response, TTL_CURRENT_SEASON, cache_hit=False)
    set_etag_headers(response, etag, TTL_CURRENT_SEASON)
    return result
