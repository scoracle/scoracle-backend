"""
Twitter/X Service implementation.

Provides access to curated journalist feeds with smart caching.
"""

import logging
import time

from ...core.config import get_settings
from ...core.http import ExternalAPIError
from ...external import TwitterClient

logger = logging.getLogger(__name__)


class TwitterService:
    """
    Twitter/X service for journalist feed access.

    Features:
    - Fetches from curated journalist X List
    - Caches full feed to minimize API calls (important for Free tier)
    - Filters cached feed client-side for queries
    """

    def __init__(
        self,
        client: TwitterClient | None = None,
        list_id: str | None = None,
        cache_ttl: int | None = None,
    ):
        """
        Initialize Twitter service.

        Args:
            client: Twitter client (created if not provided)
            list_id: X List ID for journalist feed (from config if not provided)
            cache_ttl: Cache TTL in seconds (from config if not provided)
        """
        self._client = client or TwitterClient()
        settings = get_settings()
        self._list_id = list_id or settings.twitter_journalist_list_id
        self._cache_ttl = cache_ttl or settings.twitter_feed_cache_ttl

        # In-memory cache for the feed
        self._cached_feed: dict | None = None
        self._cache_timestamp: float = 0

    @property
    def is_configured(self) -> bool:
        """Check if Twitter service is properly configured."""
        return self._client.is_configured() and bool(self._list_id)

    @property
    def list_id(self) -> str | None:
        """Get configured list ID."""
        return self._list_id

    @property
    def cache_ttl(self) -> int:
        """Get cache TTL in seconds."""
        return self._cache_ttl

    async def get_journalist_feed(
        self,
        query: str,
        sport: str | None = None,
        limit: int = 10,
    ) -> dict:
        """
        Search journalist feed for entity mentions.

        The full feed is fetched and cached; searches filter the cache.
        This optimizes API usage for Free tier limits.

        Args:
            query: Search query (player/team name)
            sport: Optional sport context (metadata only)
            limit: Maximum tweets to return

        Returns:
            Dictionary with matching tweets and metadata

        Raises:
            ExternalAPIError: If the service is not configured or the API call fails.
        """
        if not self._client.is_configured():
            raise ExternalAPIError(
                "Twitter API not configured. Set TWITTER_BEARER_TOKEN.",
                code="SERVICE_UNAVAILABLE",
                status_code=503,
            )

        if not self._list_id:
            raise ExternalAPIError(
                "Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.",
                code="SERVICE_UNAVAILABLE",
                status_code=503,
            )

        # Fetch or use cached feed
        current_time = time.time()
        feed_from_cache = False

        if (
            self._cached_feed is not None
            and (current_time - self._cache_timestamp) < self._cache_ttl
        ):
            feed_from_cache = True
            feed = self._cached_feed
        else:
            feed = await self._client.get_list_tweets(self._list_id, limit=100)
            self._cached_feed = feed
            self._cache_timestamp = current_time

        # Filter feed for query matches
        query_lower = query.lower()
        all_tweets = feed.get("tweets", [])
        filtered_tweets = [
            tweet
            for tweet in all_tweets
            if query_lower in tweet.get("text", "").lower()
        ]

        # Apply limit
        filtered_tweets = filtered_tweets[:limit]

        return {
            "query": query,
            "sport": sport,
            "tweets": filtered_tweets,
            "meta": {
                "result_count": len(filtered_tweets),
                "feed_cached": feed_from_cache,
                "feed_size": len(all_tweets),
                "cache_ttl_seconds": self._cache_ttl,
            },
        }

    def get_status(self) -> dict:
        """Get service status."""
        return {
            "service": "twitter",
            "configured": self._client.is_configured(),
            "journalist_list_configured": bool(self._list_id),
            "journalist_list_id": self._list_id,
            "feed_cache_ttl_seconds": self._cache_ttl,
            "rate_limit": "900 requests / 15 min (List endpoint)",
        }

    def clear_cache(self) -> None:
        """Clear the cached feed."""
        self._cached_feed = None
        self._cache_timestamp = 0


# Singleton instance
_twitter_service: TwitterService | None = None


def get_twitter_service() -> TwitterService:
    """Get or create the singleton Twitter service."""
    global _twitter_service
    if _twitter_service is None:
        _twitter_service = TwitterService()
    return _twitter_service
