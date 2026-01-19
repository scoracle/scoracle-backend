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
from enum import Enum
from typing import Annotated, Literal

from fastapi import APIRouter, Path, Query, Response

from ..cache import get_cache
from ..errors import NotFoundError
from ...services.news import get_news_service

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for news (10 minutes - news is time-sensitive)
NEWS_CACHE_TTL = 600


class EntityType(str, Enum):
    """Supported entity types."""
    PLAYER = "player"
    TEAM = "team"


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


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
    entity_type: Annotated[EntityType, Path(description="Entity type (player or team)")],
    entity_id: Annotated[int, Path(description="Entity ID from database")],
    sport: Annotated[Sport, Query(description="Sport context for better filtering")],
    team: Annotated[str | None, Query(description="Team name for player context")] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    source: Annotated[
        Literal["rss", "api", "both"], 
        Query(description="News source preference")
    ] = "rss",
    response: Response = None,
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
    # Get entity name from database
    # For now, we need to look up the entity
    from ...pg_connection import get_postgres_db
    
    db = get_postgres_db()
    
    if entity_type == EntityType.PLAYER:
        table = f"{sport.value.lower()}_players"
        result = db.execute(
            f"SELECT full_name, current_team_id FROM {table} WHERE id = %s",
            (entity_id,)
        )
        if not result:
            raise NotFoundError(
                resource_type="player",
                resource_id=str(entity_id),
                message=f"Player {entity_id} not found in {sport.value}",
            )
        entity_name = result[0]["full_name"]
        # Get team name if not provided
        if not team and result[0].get("current_team_id"):
            team_table = f"{sport.value.lower()}_teams"
            team_result = db.execute(
                f"SELECT name FROM {team_table} WHERE id = %s",
                (result[0]["current_team_id"],)
            )
            if team_result:
                team = team_result[0]["name"]
    else:
        table = f"{sport.value.lower()}_teams"
        result = db.execute(
            f"SELECT name FROM {table} WHERE id = %s",
            (entity_id,)
        )
        if not result:
            raise NotFoundError(
                resource_type="team",
                resource_id=str(entity_id),
                message=f"Team {entity_id} not found in {sport.value}",
            )
        entity_name = result[0]["name"]
    
    # Check cache
    cache = get_cache()
    cache_key = ("news", entity_type.value, entity_id, sport.value, source, limit)
    
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return cached
    
    response.headers["X-Cache"] = "MISS"
    
    # Fetch from news service
    service = get_news_service()
    result = await service.get_entity_news(
        entity_name=entity_name,
        sport=sport.value,
        team=team,
        limit=limit,
        prefer_source=source,
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
    response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
    
    return result
