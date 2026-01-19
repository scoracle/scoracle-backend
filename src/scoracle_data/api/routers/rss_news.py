"""
RSS News router - serves news via Google News RSS feed.

Endpoints:
- GET / - Get aggregated news for a sport
- GET /{entity_name} - Get news about a specific entity (player/team)
- GET /status - Check RSS news service status

This router uses Google News RSS which is free and requires no API key.
For NewsAPI access, use the /api-news endpoint instead.
"""

import logging
from enum import Enum
from typing import Annotated

from fastapi import APIRouter, Query, Response
from fastapi.responses import JSONResponse

from ..cache import get_cache
from ...external import GoogleNewsClient

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for news (10 minutes - news is time-sensitive)
NEWS_CACHE_TTL = 600


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


# Lazy-initialized client
_google_news_client: GoogleNewsClient | None = None


def get_google_news_client() -> GoogleNewsClient:
    """Get or create Google News client."""
    global _google_news_client
    if _google_news_client is None:
        _google_news_client = GoogleNewsClient()
    return _google_news_client


@router.get("/status")
async def get_rss_news_status():
    """
    Check RSS News service status.

    Google News RSS is always available (no API key required).
    """
    return {
        "service": "google_news_rss",
        "configured": True,
        "rate_limit": "Self-limited (respectful crawling)",
        "note": "No API key required. Free service via Google News RSS feeds.",
    }


@router.get("/{entity_name}")
async def get_entity_news(
    entity_name: str,
    sport: Annotated[Sport | None, Query(description="Sport for context filtering")] = None,
    team: Annotated[str | None, Query(description="Team name for additional context")] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    response: Response = None,
) -> dict:
    """
    Get news articles about a specific entity (player or team).

    Uses Google News RSS feed (free, no API key required).

    Args:
        entity_name: Player or team name
        sport: Optional sport context for better filtering
        team: Optional team name for additional context
        limit: Maximum articles to return

    Returns:
        Dictionary with articles and metadata
    """
    cache = get_cache()
    cache_key = ("rss_news", "entity", entity_name, sport.value if sport else None, limit)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return cached

    response.headers["X-Cache"] = "MISS"

    google_client = get_google_news_client()
    try:
        result = await google_client.search(
            query=entity_name,
            sport=sport.value if sport else None,
            team=team,
            limit=limit,
        )
        result["provider"] = "google_news_rss"

        # Cache and return
        cache.set(result, *cache_key, ttl=NEWS_CACHE_TTL)
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return result

    except Exception as e:
        logger.error(f"Google News RSS failed: {e}")
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "NEWS_UNAVAILABLE",
                    "message": "Google News RSS temporarily unavailable",
                }
            },
        )


@router.get("")
async def get_sport_news(
    sport: Annotated[Sport, Query(description="Sport to get news for")],
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 20,
    response: Response = None,
) -> dict:
    """
    Get aggregated news for a sport.

    Returns recent news about the sport in general via Google News RSS.

    Args:
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        limit: Maximum articles to return

    Returns:
        Dictionary with articles and metadata
    """
    cache = get_cache()
    cache_key = ("rss_news", "sport", sport.value, limit)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return cached

    response.headers["X-Cache"] = "MISS"

    google_client = get_google_news_client()
    try:
        result = await google_client.search_sport(
            sport=sport.value,
            limit=limit,
        )
        result["provider"] = "google_news_rss"

        # Cache and return
        cache.set(result, *cache_key, ttl=NEWS_CACHE_TTL)
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return result

    except Exception as e:
        logger.error(f"Sport news fetch failed: {e}")
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "NEWS_UNAVAILABLE",
                    "message": "Google News RSS temporarily unavailable",
                }
            },
        )
