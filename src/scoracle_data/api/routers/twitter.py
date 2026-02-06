"""
Twitter router - serves curated journalist feed for sports news.

Endpoints:
- GET /journalist-feed - Search trusted journalist tweets for team/player mentions
- GET /status - Check Twitter API configuration status

Strategy:
- Fetches tweets from a curated X List of trusted sports journalists
- Caches the full feed (1 hour TTL) to minimize API calls
- Filters cached feed client-side for each search query
- This approach is optimal for X API Free tier (limited read quota)
"""

import logging
from enum import Enum
from typing import Annotated

from fastapi import APIRouter, Query, Response

from ..cache import get_cache
from ..errors import ServiceUnavailableError, ExternalServiceError, RateLimitedError
from ...core.config import get_settings
from ...external import TwitterClient, ExternalAPIError, RateLimitError

logger = logging.getLogger(__name__)

router = APIRouter()


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


# Lazy-initialized client
_twitter_client: TwitterClient | None = None


def get_twitter_client() -> TwitterClient:
    """Get or create Twitter client."""
    global _twitter_client
    if _twitter_client is None:
        _twitter_client = TwitterClient()
    return _twitter_client


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


def _set_cache_headers(response: Response, cache_hit: bool, ttl: int) -> None:
    """Set cache headers for external API responses."""
    response.headers["X-Cache"] = "HIT" if cache_hit else "MISS"
    response.headers["Cache-Control"] = f"public, max-age={ttl}"


@router.get("/journalist-feed")
async def get_journalist_feed(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport for context (not used for filtering, metadata only)")] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results to return")] = 10,
    response: Response = None,
):
    """
    Search trusted journalist feed for mentions of a player or team.

    Fetches tweets from a curated X List of trusted sports journalists,
    then filters for mentions of the search query. The full feed is cached
    for 1 hour to optimize API usage (important for Free tier limits).

    **Caching Strategy:**
    - Full journalist feed is fetched once and cached for 1 hour
    - All search queries filter the same cached feed
    - This means searches for "Lakers" and "LeBron" use the same API call

    **Rate limit:** Shared feed fetched at most once per hour.
    """
    settings = get_settings()
    client = get_twitter_client()

    # Check configuration
    if not client.is_configured():
        raise ServiceUnavailableError(
            service="Twitter",
            message="Twitter API not configured. Set TWITTER_BEARER_TOKEN.",
        )

    list_id = settings.twitter_journalist_list_id
    if not list_id:
        raise ServiceUnavailableError(
            service="Twitter",
            message="Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.",
        )

    cache = get_cache()
    cache_ttl = settings.twitter_feed_cache_ttl

    # Cache key for the FULL journalist feed (not per-query)
    feed_cache_key = ("twitter", "journalist-feed", list_id)

    # Check if full feed is cached
    cached_feed = cache.get(*feed_cache_key)
    feed_from_cache = cached_feed is not None

    if not cached_feed:
        # Fetch from X API
        try:
            cached_feed = await client.get_list_tweets(list_id, limit=100)
            cache.set(cached_feed, *feed_cache_key, ttl=cache_ttl)
        except Exception as e:
            _handle_external_error(e, "Twitter")

    # Filter cached feed for query matches (case-insensitive)
    query_lower = q.lower()
    all_tweets = cached_feed.get("tweets", [])
    filtered_tweets = [
        tweet for tweet in all_tweets
        if query_lower in tweet.get("text", "").lower()
    ]

    # Apply limit to filtered results
    filtered_tweets = filtered_tweets[:limit]

    _set_cache_headers(response, cache_hit=feed_from_cache, ttl=cache_ttl)

    return {
        "query": q,
        "sport": sport.value if sport else None,
        "tweets": filtered_tweets,
        "meta": {
            "result_count": len(filtered_tweets),
            "feed_cached": feed_from_cache,
            "feed_size": len(all_tweets),
            "cache_ttl_seconds": cache_ttl,
        },
    }


@router.get("/status")
async def get_twitter_status():
    """
    Check Twitter API configuration status.

    Returns configuration state and rate limit info for debugging.
    """
    settings = get_settings()
    client = get_twitter_client()

    return {
        "service": "twitter",
        "configured": client.is_configured(),
        "journalist_list_configured": bool(settings.twitter_journalist_list_id),
        "journalist_list_id": settings.twitter_journalist_list_id,
        "feed_cache_ttl_seconds": settings.twitter_feed_cache_ttl,
        "rate_limit": "900 requests / 15 min (List endpoint)",
        "note": "Only journalist-feed endpoint available. Generic search removed to ensure content quality.",
    }
