"""
News router with Google News RSS as free fallback.

This router provides news endpoints that use:
1. NewsAPI (if configured) - better quality, requires API key
2. Google News RSS (fallback) - free, no API key required

The /intel/news endpoint in intel.py remains unchanged for direct NewsAPI access.
This router adds entity-specific and sport-wide news endpoints.
"""

import logging
from enum import Enum
from typing import Annotated

from fastapi import APIRouter, Query, Response
from fastapi.responses import JSONResponse

from ..cache import get_cache
from ...external import NewsClient, GoogleNewsClient

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for news (10 minutes - news is time-sensitive)
NEWS_CACHE_TTL = 600


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


# Lazy-initialized clients
_news_client: NewsClient | None = None
_google_news_client: GoogleNewsClient | None = None


def get_news_client() -> NewsClient:
    """Get or create NewsAPI client."""
    global _news_client
    if _news_client is None:
        _news_client = NewsClient()
    return _news_client


def get_google_news_client() -> GoogleNewsClient:
    """Get or create Google News client."""
    global _google_news_client
    if _google_news_client is None:
        _google_news_client = GoogleNewsClient()
    return _google_news_client


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

    Uses NewsAPI if configured, falls back to Google News RSS (free).

    Args:
        entity_name: Player or team name
        sport: Optional sport context for better filtering
        team: Optional team name for additional context
        limit: Maximum articles to return

    Returns:
        Dictionary with articles and metadata including source info
    """
    cache = get_cache()
    cache_key = ("news", entity_name, sport.value if sport else None, limit)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return cached

    response.headers["X-Cache"] = "MISS"

    # Try NewsAPI first (better quality)
    news_client = get_news_client()
    if news_client.is_configured():
        try:
            result = await news_client.search(
                query=entity_name,
                sport=sport.value if sport else None,
                days=7,
                limit=limit,
            )
            result["provider"] = "newsapi"

            # Cache and return
            cache.set(result, *cache_key, ttl=NEWS_CACHE_TTL)
            response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
            return result

        except Exception as e:
            logger.warning(f"NewsAPI failed, falling back to Google News: {e}")

    # Fallback to Google News RSS (free)
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
        logger.error(f"Google News RSS also failed: {e}")
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "NEWS_UNAVAILABLE",
                    "message": "News services temporarily unavailable",
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

    Returns recent news about the sport in general.

    Args:
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        limit: Maximum articles to return

    Returns:
        Dictionary with articles and metadata
    """
    cache = get_cache()
    cache_key = ("sport_news", sport.value, limit)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"
        return cached

    response.headers["X-Cache"] = "MISS"

    # Use Google News for sport-wide news (free, no API limit concerns)
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
                    "message": "News services temporarily unavailable",
                }
            },
        )
