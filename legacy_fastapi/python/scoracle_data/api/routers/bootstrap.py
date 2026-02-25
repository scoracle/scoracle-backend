"""
Autofill databases router — serves entity bootstrap data for frontend autocomplete.

Endpoints:
- GET / - Autofill entity database for a single sport

The frontend calls this endpoint to fetch the full set of players and teams
for a sport, then caches it locally for instant fuzzy-search autocomplete.
These databases are refreshed infrequently (3-4 times per year).

Performance Features:
- Server-side caching with 24h TTL (HybridCache)
- ETag support for conditional requests (304 Not Modified)
- GZip compression via middleware (~650KB -> ~80KB on the wire)
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_ENTITY_INFO
from ..dependencies import DBDependency
from ...core.types import Sport, get_sport_config
from ._utils import (
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
)
from ...services.bootstrap import get_autofill_database

logger = logging.getLogger(__name__)

router = APIRouter()


@router.get("/", response_model=None)
async def get_autofill_db(
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get the autofill entity database for a sport.

    Returns all players and teams in a compact format optimized for
    frontend fuzzy-search autocomplete. The frontend should cache this
    response locally and refresh it periodically.

    Each entity includes:
    - Unique compound ID and database entity_id
    - Display name with normalized/tokenized variants for search
    - Position, team, and league metadata
    - Sport identifier for API callbacks

    Supports conditional requests via If-None-Match header.
    Returns 304 Not Modified if the database hasn't changed.
    """
    cache = get_cache()
    cache_key = ("autofill_databases", sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=True)
        set_etag_headers(response, etag, TTL_ENTITY_INFO)
        return cached

    # Resolve current season for the sport
    cfg = get_sport_config(sport.value)
    season = cfg.current_season

    result = await get_autofill_database(db, sport.value, season)

    cache.set(result, *cache_key, ttl=TTL_ENTITY_INFO)
    etag = compute_etag(result)
    set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=False)
    set_etag_headers(response, etag, TTL_ENTITY_INFO)
    return result
