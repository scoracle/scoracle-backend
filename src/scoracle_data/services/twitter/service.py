"""
Twitter/X Service implementation.

Provides access to curated journalist feeds with smart caching.
"""

import logging
from dataclasses import dataclass

from ...config import get_settings
from ...external import TwitterClient

logger = logging.getLogger(__name__)


@dataclass
class Tweet:
    """Normalized tweet from journalist feed."""
    id: str
    text: str
    author_id: str
    author_username: str | None
    created_at: str | None
    url: str | None = None


@dataclass
class JournalistFeedResult:
    """Result from journalist feed search."""
    tweets: list[dict]
    query: str
    sport: str | None
    meta: dict


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
        """
        if not self._client.is_configured():
            return self._error_result(
                query=query,
                sport=sport,
                message="Twitter API not configured. Set TWITTER_BEARER_TOKEN.",
            )
        
        if not self._list_id:
            return self._error_result(
                query=query,
                sport=sport,
                message="Twitter journalist list not configured. Set TWITTER_JOURNALIST_LIST_ID.",
            )
        
        # Fetch or use cached feed
        import time
        current_time = time.time()
        feed_from_cache = False
        
        if (
            self._cached_feed is not None
            and (current_time - self._cache_timestamp) < self._cache_ttl
        ):
            feed_from_cache = True
            feed = self._cached_feed
        else:
            try:
                feed = await self._client.get_list_tweets(self._list_id, limit=100)
                self._cached_feed = feed
                self._cache_timestamp = current_time
            except Exception as e:
                logger.error(f"Twitter API failed: {e}")
                return self._error_result(query, sport, f"Twitter API failed: {e}")
        
        # Filter feed for query matches
        query_lower = query.lower()
        all_tweets = feed.get("tweets", [])
        filtered_tweets = [
            tweet for tweet in all_tweets
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
    
    def _error_result(
        self,
        query: str,
        sport: str | None,
        message: str,
    ) -> dict:
        """Create error result."""
        return {
            "query": query,
            "sport": sport,
            "tweets": [],
            "error": message,
            "meta": {
                "result_count": 0,
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
