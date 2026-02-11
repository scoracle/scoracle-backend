"""
Unified News router - serves entity-specific news from multiple sources.

Endpoints:
- GET /news/{entity_type}/{entity_id} - Get news about a specific entity
- GET /news/status - Check news services status

This router uses the NewsService which combines:
- Google News RSS (primary, free)
- NewsAPI (fallback, requires key)

Note: No general sport news endpoint - only entity-specific news.
Twitter is served separately via /twitter for lazy loading.
"""

import logging
from typing import Annotated, Literal

from fastapi import APIRouter, Path, Query, Response

from ..cache import get_cache
from ..dependencies import DBDependency
from ..errors import NotFoundError, ExternalServiceError
from ...core.http import ExternalAPIError
from ...core.types import EntityType, PLAYERS_TABLE, Sport, TEAMS_TABLE
from ...services.news import get_news_service
from ._utils import set_cache_headers

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for news (10 minutes - news is time-sensitive)
NEWS_CACHE_TTL = 600


@router.get("/status")
async def get_news_status():
    """
    Check news services status.

    Returns configuration state for both RSS and NewsAPI.
    """
    service = get_news_service()
    return service.get_status()


@router.get("/{entity_type}/{entity_id}")
async def get_entity_news(
    entity_type: Annotated[
        EntityType, Path(description="Entity type (player or team)")
    ],
    entity_id: Annotated[int, Path(description="Entity ID from database")],
    sport: Annotated[Sport, Query(description="Sport context for better filtering")],
    response: Response,
    db: DBDependency,
    team: Annotated[
        str | None, Query(description="Team name for player context")
    ] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    source: Annotated[
        Literal["rss", "api", "both"], Query(description="News source preference")
    ] = "rss",
) -> dict:
    """
    Get news articles about a specific entity (player or team).

    Uses the unified NewsService which combines Google News RSS and NewsAPI.

    Args:
        entity_type: "player" or "team"
        entity_id: Entity ID from database
        sport: Sport context for better filtering (NBA, NFL, FOOTBALL)
        team: Optional team name for player context
        limit: Maximum articles to return
        source: News source preference:
            - "rss": Google News RSS only (default, free)
            - "api": NewsAPI only (requires key)
            - "both": Try both, merge and dedupe

    Returns:
        Dictionary with articles and metadata

    Example:
        GET /news/player/123?sport=NBA&limit=10
        GET /news/team/456?sport=NFL&source=both
    """
    # Track name components for stricter filtering
    first_name: str | None = None
    last_name: str | None = None

    if entity_type == EntityType.player:
        result = db.fetchone(
            f"SELECT name, first_name, last_name, team_id FROM {PLAYERS_TABLE} "
            f"WHERE id = %s AND sport = %s",
            (entity_id, sport.value),
        )
        if not result:
            raise NotFoundError(
                resource="player",
                identifier=str(entity_id),
            )
        entity_name = result["name"]
        first_name = result.get("first_name")
        last_name = result.get("last_name")
        # Get team name if not provided
        if not team and result.get("team_id"):
            team_result = db.fetchone(
                f"SELECT name FROM {TEAMS_TABLE} WHERE id = %s AND sport = %s",
                (result["team_id"], sport.value),
            )
            if team_result:
                team = team_result["name"]
    else:
        result = db.fetchone(
            f"SELECT name FROM {TEAMS_TABLE} WHERE id = %s AND sport = %s",
            (entity_id, sport.value),
        )
        if not result:
            raise NotFoundError(
                resource="team",
                identifier=str(entity_id),
            )
        entity_name = result["name"]

    # Check cache
    cache = get_cache()
    cache_key = ("news", entity_type.value, entity_id, sport.value, source, limit)

    cached = cache.get(*cache_key)
    if cached:
        set_cache_headers(response, ttl=NEWS_CACHE_TTL, cache_hit=True)
        return cached

    # Fetch from news service
    service = get_news_service()
    try:
        result = await service.get_entity_news(
            entity_name=entity_name,
            sport=sport.value,
            team=team,
            limit=limit,
            prefer_source=source,
            first_name=first_name,
            last_name=last_name,
        )
    except ExternalAPIError as e:
        raise ExternalServiceError(
            service="News",
            message=e.message,
            status_code=e.status_code,
        )

    # Add entity info to response
    result["entity"] = {
        "type": entity_type.value,
        "id": entity_id,
        "name": entity_name,
        "sport": sport.value,
    }

    # Cache and return
    cache.set(result, *cache_key, ttl=NEWS_CACHE_TTL)
    set_cache_headers(response, ttl=NEWS_CACHE_TTL, cache_hit=False)

    return result
