"""
Profile router - serves entity profiles for frontend widgets.

Endpoints:
- GET /{entity_type}/{entity_id} - Entity profile from players/teams tables

Data:
- Serves entity info from profile tables (nba_players, nba_teams, etc.)
- Includes related team/league info via JOINs

Performance Features:
- In-memory caching with TTLs
- ETag support for conditional requests (304 Not Modified)
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_ENTITY_INFO
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ..types import EntityType, Sport
from ._utils import (
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
)
from ...services.profiles import get_player_profile, get_team_profile

logger = logging.getLogger(__name__)

router = APIRouter()


@router.get("/{entity_type}/{entity_id}", response_model=None)
async def get_entity_profile(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get entity profile for widget rendering.

    Returns all fields from the players or teams profile table, with related
    team/league info included via JOINs.

    This endpoint serves entity info ONLY (name, photo, team, position, etc.).
    For statistics, use the /stats endpoint.

    Supports conditional requests via If-None-Match header for ETag validation.
    Returns 304 Not Modified if content hasn't changed.
    """
    cache = get_cache()
    cache_key = ("profile", entity_type.value, entity_id, sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=True)
        set_etag_headers(response, etag, TTL_ENTITY_INFO)
        return cached

    if entity_type == EntityType.player:
        result = get_player_profile(db, entity_id, sport.value)
    else:
        result = get_team_profile(db, entity_id, sport.value)

    if not result:
        raise NotFoundError(
            resource=entity_type.value.title(),
            identifier=entity_id,
            context=sport.value,
        )

    cache.set(result, *cache_key, ttl=TTL_ENTITY_INFO)
    etag = compute_etag(result)
    set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=False)
    set_etag_headers(response, etag, TTL_ENTITY_INFO)
    return result
