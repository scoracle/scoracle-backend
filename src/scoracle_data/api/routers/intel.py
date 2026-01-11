"""
Intel router for external API endpoints (Twitter, News, Reddit).

These endpoints fetch real-time social/news data about players and teams.
Frontend uses lazy-loading: stats load immediately, these load on tab click.

Caching Strategy:
- External API responses are cached for 5 minutes to reduce API calls
- This is a balance between freshness and API rate limits
- Cache keys include query + sport + limit to avoid serving wrong results
"""

import logging
from enum import Enum
from typing import Annotated, Any

from fastapi import APIRouter, Query, Response

from ..cache import get_cache
from ..errors import ServiceUnavailableError, ExternalServiceError, RateLimitedError
from ...external import TwitterClient, NewsClient, RedditClient, ExternalAPIError, RateLimitError

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTL for external API responses (5 minutes)
# This reduces API calls while keeping data reasonably fresh
TTL_EXTERNAL_API = 300  # 5 minutes


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


class RedditSort(str, Enum):
    """Reddit sort options."""
    relevance = "relevance"
    hot = "hot"
    new = "new"
    top = "top"


# Initialize clients (lazy - only make requests when called)
_twitter_client: TwitterClient | None = None
_news_client: NewsClient | None = None
_reddit_client: RedditClient | None = None


def get_twitter_client() -> TwitterClient:
    """Get or create Twitter client."""
    global _twitter_client
    if _twitter_client is None:
        _twitter_client = TwitterClient()
    return _twitter_client


def get_news_client() -> NewsClient:
    """Get or create News client."""
    global _news_client
    if _news_client is None:
        _news_client = NewsClient()
    return _news_client


def get_reddit_client() -> RedditClient:
    """Get or create Reddit client."""
    global _reddit_client
    if _reddit_client is None:
        _reddit_client = RedditClient()
    return _reddit_client


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
    response.headers["Cache-Control"] = f"public, max-age={TTL_EXTERNAL_API}"


def _get_cached_or_none(cache_key: tuple) -> tuple[Any | None, bool]:
    """
    Check cache for a key.

    Returns:
        Tuple of (cached_value_or_none, was_cache_hit)
    """
    cache = get_cache()
    cached = cache.get(*cache_key)
    return cached, cached is not None


def _cache_result(cache_key: tuple, result: Any) -> None:
    """Store result in cache with external API TTL."""
    cache = get_cache()
    cache.set(result, *cache_key, ttl=TTL_EXTERNAL_API)


@router.get("/twitter")
async def get_twitter_intel(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport for context filtering")] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    response: Response = None,
):
    """
    Search for recent tweets about a sports entity.

    Returns tweets from the last 7 days matching the query, with author info
    and engagement metrics. Results are cached for 5 minutes.

    **Rate limit:** 450 requests per 15 minutes (shared across all users).
    """
    client = get_twitter_client()

    if not client.is_configured():
        raise ServiceUnavailableError(service="Twitter", message="Twitter API not configured")

    # Check cache first
    cache_key = ("intel", "twitter", q.lower(), sport.value if sport else None, limit)
    cached, cache_hit = _get_cached_or_none(cache_key)

    if cached:
        _set_cache_headers(response, cache_hit=True)
        return cached

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            limit=limit,
        )

        # Cache successful results
        _cache_result(cache_key, result)
        _set_cache_headers(response, cache_hit=False)

        return result
    except Exception as e:
        _handle_external_error(e, "Twitter")


@router.get("/news")
async def get_news_intel(
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
        raise ServiceUnavailableError(service="News", message="News API not configured")

    # Check cache first
    cache_key = ("intel", "news", q.lower(), sport.value if sport else None, days, limit)
    cached, cache_hit = _get_cached_or_none(cache_key)

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

        # Cache successful results
        _cache_result(cache_key, result)
        _set_cache_headers(response, cache_hit=False)

        return result
    except Exception as e:
        _handle_external_error(e, "News")


@router.get("/reddit")
async def get_reddit_intel(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport determines subreddit (NBA→r/nba, etc.)")] = None,
    sort: Annotated[RedditSort, Query(description="Sort order")] = RedditSort.relevance,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
    response: Response = None,
):
    """
    Search for Reddit posts about a sports entity.

    Searches sport-specific subreddits (r/nba, r/nfl, r/soccer) for discussions
    about the queried player or team. Results are cached for 5 minutes.

    **Rate limit:** 100 requests per minute.
    """
    client = get_reddit_client()

    if not client.is_configured():
        raise ServiceUnavailableError(service="Reddit", message="Reddit API not configured")

    # Check cache first
    cache_key = ("intel", "reddit", q.lower(), sport.value if sport else None, sort.value, limit)
    cached, cache_hit = _get_cached_or_none(cache_key)

    if cached:
        _set_cache_headers(response, cache_hit=True)
        return cached

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            sort=sort.value,
            limit=limit,
        )

        # Cache successful results
        _cache_result(cache_key, result)
        _set_cache_headers(response, cache_hit=False)

        return result
    except Exception as e:
        _handle_external_error(e, "Reddit")


@router.get("/status")
async def get_intel_status():
    """
    Check configuration status of all external API services.

    Returns which services are available (have API keys configured).
    """
    twitter = get_twitter_client()
    news = get_news_client()
    reddit = get_reddit_client()

    return {
        "services": {
            "twitter": {
                "configured": twitter.is_configured(),
                "rate_limit": "450 requests / 15 min",
            },
            "news": {
                "configured": news.is_configured(),
                "rate_limit": "100 requests / day (free tier)",
            },
            "reddit": {
                "configured": reddit.is_configured(),
                "rate_limit": "100 requests / min",
            },
        }
    }
