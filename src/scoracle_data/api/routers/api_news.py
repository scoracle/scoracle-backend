"""
API News router - serves news via NewsAPI.org.

Endpoints:
- GET / - Search for news articles about sports entities
- GET /status - Check NewsAPI configuration status

This router uses NewsAPI.org which requires an API key.
For free news access, use the /rss-news endpoint (Google News RSS) instead.
"""

import logging
from enum import Enum
from typing import Annotated

from fastapi import APIRouter, Query, Response

from ..cache import get_cache
from ..errors import ServiceUnavailableError, ExternalServiceError, RateLimitedError
from ...external import NewsClient, ExternalAPIError, RateLimitError

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for news (5 minutes - external API data)
NEWS_CACHE_TTL = 300


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


# Lazy-initialized client
_news_client: NewsClient | None = None


def get_news_client() -> NewsClient:
    """Get or create NewsAPI client."""
    global _news_client
    if _news_client is None:
        _news_client = NewsClient()
    return _news_client


def _handle_external_error(e: Exception, service: str) -> None:
    """Convert external API errors to consistent API errors (raises)."""
    if isinstance(e, RateLimitError):
        raise RateLimitedError(retry_after=e.retry_after or 60)
    elif isinstance(e, ExternalAPIError):
        raise ExternalServiceError(
            service=service,
            message=e.message,
            status_code=e.status_code,
        )
    else:
        logger.error(f"{service} unexpected error: {e}")
        raise ExternalServiceError(
            service=service,
            message=f"{service} request failed unexpectedly",
        )


def _set_cache_headers(response: Response, cache_hit: bool) -> None:
    """Set cache headers for external API responses."""
    response.headers["X-Cache"] = "HIT" if cache_hit else "MISS"
    response.headers["Cache-Control"] = f"public, max-age={NEWS_CACHE_TTL}"


@router.get("/status")
async def get_api_news_status():
    """
    Check NewsAPI configuration status.

    Returns configuration state and rate limit info for debugging.
    """
    client = get_news_client()

    return {
        "service": "newsapi",
        "configured": client.is_configured(),
        "rate_limit": "100 requests / day (free tier)",
        "note": "Requires NEWS_API_KEY environment variable. For free alternative, use /rss-news endpoint.",
    }


@router.get("")
async def get_news(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport for source filtering")] = None,
    days: Annotated[int, Query(ge=1, le=30, description="Days back to search")] = 7,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    response: Response = None,
):
    """
    Search for news articles about a sports entity.

    Returns articles from major sports news sources, filtered by sport
    for better relevance. Results are cached for 5 minutes.

    **Rate limit:** 100 requests per day (free tier).
    """
    client = get_news_client()

    if not client.is_configured():
        raise ServiceUnavailableError(
            service="NewsAPI",
            message="NewsAPI not configured. Set NEWS_API_KEY environment variable.",
        )

    # Check cache first
    cache = get_cache()
    cache_key = ("api_news", q.lower(), sport.value if sport else None, days, limit)
    cached = cache.get(*cache_key)

    if cached:
        _set_cache_headers(response, cache_hit=True)
        return cached

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            days=days,
            limit=limit,
        )
        result["provider"] = "newsapi"

        # Cache successful results
        cache.set(result, *cache_key, ttl=NEWS_CACHE_TTL)
        _set_cache_headers(response, cache_hit=False)

        return result
    except Exception as e:
        _handle_external_error(e, "NewsAPI")
