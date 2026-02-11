"""
Twitter router - serves curated journalist feed for sports news.

Endpoints:
- GET /journalist-feed - Search trusted journalist tweets for team/player mentions
- GET /status - Check Twitter API configuration status

Delegates to TwitterService for feed fetching, caching, and filtering.
"""

import logging
from typing import Annotated

from fastapi import APIRouter, Query, Response

from ..errors import ServiceUnavailableError, ExternalServiceError, RateLimitedError
from ...core.http import ExternalAPIError, RateLimitError
from ...core.types import Sport
from ...services.twitter import get_twitter_service
from ._utils import set_cache_headers

logger = logging.getLogger(__name__)

router = APIRouter()


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


# Cache headers use shared set_cache_headers from _utils


@router.get("/journalist-feed")
async def get_journalist_feed(
    q: Annotated[
        str,
        Query(
            min_length=1, max_length=200, description="Search query (player/team name)"
        ),
    ],
    response: Response,
    sport: Annotated[
        Sport | None,
        Query(description="Sport for context (not used for filtering, metadata only)"),
    ] = None,
    limit: Annotated[int, Query(ge=1, le=50, description="Max results to return")] = 10,
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
    service = get_twitter_service()

    # Check configuration — raise HTTP errors for backward compatibility.
    # The service exposes is_configured and list_id so we can give specific messages.
    if not service.is_configured:
        if not service.list_id:
            raise ServiceUnavailableError(
                service="Twitter",
                message="Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.",
            )
        raise ServiceUnavailableError(
            service="Twitter",
            message="Twitter API not configured. Set TWITTER_BEARER_TOKEN.",
        )

    sport_value = sport.value if sport else None

    try:
        result = await service.get_journalist_feed(
            query=q,
            sport=sport_value,
            limit=limit,
        )
    except Exception as e:
        _handle_external_error(e, "Twitter")
        return  # unreachable; _handle_external_error always raises

    # Set cache headers from result metadata
    meta = result.get("meta", {})
    set_cache_headers(
        response,
        ttl=service.cache_ttl,
        cache_hit=meta.get("feed_cached", False),
    )

    return result


@router.get("/status")
async def get_twitter_status():
    """
    Check Twitter API configuration status.

    Returns configuration state and rate limit info for debugging.
    """
    service = get_twitter_service()
    status = service.get_status()
    status["note"] = (
        "Only journalist-feed endpoint available. "
        "Generic search removed to ensure content quality."
    )
    return status
