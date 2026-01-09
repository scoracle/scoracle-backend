"""
Intel router for external API endpoints (Twitter, News, Reddit).

These endpoints fetch real-time social/news data about players and teams.
Frontend uses lazy-loading: stats load immediately, these load on tab click.
"""

import logging
from enum import Enum
from typing import Annotated

from fastapi import APIRouter, Query, HTTPException
from fastapi.responses import JSONResponse

from ...external import TwitterClient, NewsClient, RedditClient, ExternalAPIError, RateLimitError

logger = logging.getLogger(__name__)

router = APIRouter()


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


def _handle_external_error(e: Exception, service: str) -> JSONResponse:
    """Convert external API errors to consistent error responses."""
    if isinstance(e, RateLimitError):
        return JSONResponse(
            status_code=429,
            content={
                "error": {
                    "code": "RATE_LIMITED",
                    "message": f"{service} API rate limit exceeded. Try again later.",
                    "retry_after": e.retry_after,
                }
            },
        )
    elif isinstance(e, ExternalAPIError):
        return JSONResponse(
            status_code=e.status_code,
            content={
                "error": {
                    "code": e.code,
                    "message": e.message,
                }
            },
        )
    else:
        logger.error(f"{service} unexpected error: {e}")
        return JSONResponse(
            status_code=500,
            content={
                "error": {
                    "code": "EXTERNAL_API_ERROR",
                    "message": f"{service} request failed unexpectedly.",
                }
            },
        )


@router.get("/twitter")
async def get_twitter_intel(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport for context filtering")] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
):
    """
    Search for recent tweets about a sports entity.

    Returns tweets from the last 7 days matching the query, with author info
    and engagement metrics.

    **Rate limit:** 450 requests per 15 minutes (shared across all users).
    """
    client = get_twitter_client()

    if not client.is_configured():
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "Twitter API not configured.",
                }
            },
        )

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            limit=limit,
        )
        return result
    except Exception as e:
        return _handle_external_error(e, "Twitter")


@router.get("/news")
async def get_news_intel(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport for source filtering")] = None,
    days: Annotated[int, Query(ge=1, le=30, description="Days back to search")] = 7,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
):
    """
    Search for news articles about a sports entity.

    Returns articles from major sports news sources, filtered by sport
    for better relevance.

    **Rate limit:** 100 requests per day (free tier).
    """
    client = get_news_client()

    if not client.is_configured():
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "News API not configured.",
                }
            },
        )

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            days=days,
            limit=limit,
        )
        return result
    except Exception as e:
        return _handle_external_error(e, "News")


@router.get("/reddit")
async def get_reddit_intel(
    q: Annotated[str, Query(min_length=1, max_length=200, description="Search query (player/team name)")],
    sport: Annotated[Sport | None, Query(description="Sport determines subreddit (NBA→r/nba, etc.)")] = None,
    sort: Annotated[RedditSort, Query(description="Sort order")] = RedditSort.relevance,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results")] = 10,
):
    """
    Search for Reddit posts about a sports entity.

    Searches sport-specific subreddits (r/nba, r/nfl, r/soccer) for discussions
    about the queried player or team.

    **Rate limit:** 100 requests per minute.
    """
    client = get_reddit_client()

    if not client.is_configured():
        return JSONResponse(
            status_code=503,
            content={
                "error": {
                    "code": "SERVICE_UNAVAILABLE",
                    "message": "Reddit API not configured.",
                }
            },
        )

    try:
        result = await client.search(
            query=q,
            sport=sport.value if sport else None,
            sort=sort.value,
            limit=limit,
        )
        return result
    except Exception as e:
        return _handle_external_error(e, "Reddit")


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
