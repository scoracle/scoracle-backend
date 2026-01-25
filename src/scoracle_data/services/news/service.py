"""
Unified News Service implementation.

Combines Google News RSS and NewsAPI into a single interface with:
- Primary: Google News RSS (free, no rate limits)
- Fallback: NewsAPI (requires API key, rate limited)
- Smart caching to reduce API calls
"""

import logging
from dataclasses import dataclass
from typing import Literal

from ...external import GoogleNewsClient, NewsClient

logger = logging.getLogger(__name__)

# Cache TTL in seconds
DEFAULT_CACHE_TTL = 600  # 10 minutes


@dataclass
class NewsArticle:
    """Normalized news article from any provider."""
    title: str
    url: str
    source: str
    published_at: str | None
    description: str | None = None
    image_url: str | None = None
    provider: str = "unknown"


@dataclass
class NewsResult:
    """Result from news search."""
    articles: list[NewsArticle]
    query: str
    sport: str | None
    provider: str
    meta: dict


class NewsService:
    """
    Unified news service that combines multiple news sources.
    
    Features:
    - Primary source: Google News RSS (free, no API key)
    - Fallback: NewsAPI (if configured and RSS fails)
    - Deduplication across sources
    - Normalized article format
    """
    
    def __init__(
        self,
        google_client: GoogleNewsClient | None = None,
        newsapi_client: NewsClient | None = None,
    ):
        """
        Initialize news service.
        
        Args:
            google_client: Google News RSS client (created if not provided)
            newsapi_client: NewsAPI client (created if not provided)
        """
        self._google_client = google_client or GoogleNewsClient()
        self._newsapi_client = newsapi_client or NewsClient()
    
    @property
    def has_newsapi(self) -> bool:
        """Check if NewsAPI is configured."""
        return self._newsapi_client.is_configured()
    
    async def get_entity_news(
        self,
        entity_name: str,
        sport: str | None = None,
        team: str | None = None,
        limit: int = 10,
        prefer_source: Literal["rss", "api", "both"] = "rss",
        first_name: str | None = None,
        last_name: str | None = None,
    ) -> dict:
        """
        Get news about a specific entity (player or team).

        Args:
            entity_name: Player or team name
            sport: Optional sport context (NBA, NFL, FOOTBALL)
            team: Optional team name for player context
            limit: Maximum articles to return
            prefer_source: Which source to prefer
                - "rss": Use Google News RSS only (default)
                - "api": Use NewsAPI only (requires config)
                - "both": Try both, merge and dedupe
            first_name: Entity's first name (for stricter filtering)
            last_name: Entity's last name (for stricter filtering)

        Returns:
            Dictionary with articles and metadata
        """
        if prefer_source == "api":
            if not self.has_newsapi:
                return self._error_result(
                    query=entity_name,
                    sport=sport,
                    message="NewsAPI not configured",
                )
            return await self._fetch_from_newsapi(entity_name, sport, limit)

        if prefer_source == "both":
            return await self._fetch_from_both(
                entity_name, sport, team, limit, first_name, last_name
            )

        # Default: RSS only
        return await self._fetch_from_rss(
            entity_name, sport, team, limit, first_name, last_name
        )
    
    async def _fetch_from_rss(
        self,
        query: str,
        sport: str | None,
        team: str | None,
        limit: int,
        first_name: str | None = None,
        last_name: str | None = None,
    ) -> dict:
        """Fetch news from Google News RSS."""
        try:
            result = await self._google_client.search(
                query=query,
                sport=sport,
                team=team,
                limit=limit,
                first_name=first_name,
                last_name=last_name,
            )
            result["provider"] = "google_news_rss"
            return result
        except Exception as e:
            logger.error(f"Google News RSS failed: {e}")
            # Try fallback to NewsAPI if available
            if self.has_newsapi:
                logger.info("Falling back to NewsAPI")
                return await self._fetch_from_newsapi(query, sport, limit)
            return self._error_result(query, sport, f"News fetch failed: {e}")
    
    async def _fetch_from_newsapi(
        self,
        query: str,
        sport: str | None,
        limit: int,
    ) -> dict:
        """Fetch news from NewsAPI."""
        try:
            result = await self._newsapi_client.search(
                query=query,
                sport=sport,
                days=7,
                limit=limit,
            )
            result["provider"] = "newsapi"
            return result
        except Exception as e:
            logger.error(f"NewsAPI failed: {e}")
            return self._error_result(query, sport, f"NewsAPI fetch failed: {e}")
    
    async def _fetch_from_both(
        self,
        query: str,
        sport: str | None,
        team: str | None,
        limit: int,
        first_name: str | None = None,
        last_name: str | None = None,
    ) -> dict:
        """Fetch from both sources, merge and dedupe."""
        rss_result = await self._fetch_from_rss(
            query, sport, team, limit * 2, first_name, last_name
        )
        
        if not self.has_newsapi:
            # Just return RSS if NewsAPI not available
            return rss_result
        
        api_result = await self._fetch_from_newsapi(query, sport, limit * 2)
        
        # Merge articles, dedupe by URL
        seen_urls: set[str] = set()
        merged_articles = []
        
        for article in rss_result.get("articles", []):
            url = article.get("url", "")
            if url and url not in seen_urls:
                seen_urls.add(url)
                merged_articles.append(article)
        
        for article in api_result.get("articles", []):
            url = article.get("url", "")
            if url and url not in seen_urls:
                seen_urls.add(url)
                merged_articles.append(article)
        
        # Limit results
        merged_articles = merged_articles[:limit]
        
        return {
            "articles": merged_articles,
            "query": query,
            "sport": sport,
            "provider": "combined",
            "meta": {
                "rss_count": len(rss_result.get("articles", [])),
                "api_count": len(api_result.get("articles", [])),
                "merged_count": len(merged_articles),
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
            "articles": [],
            "query": query,
            "sport": sport,
            "provider": "none",
            "error": message,
            "meta": {
                "result_count": 0,
            },
        }
    
    def get_status(self) -> dict:
        """Get service status."""
        return {
            "rss_available": True,  # Always available (no auth)
            "newsapi_configured": self.has_newsapi,
            "primary_source": "google_news_rss",
            "fallback_source": "newsapi" if self.has_newsapi else None,
        }


# Singleton instance
_news_service: NewsService | None = None


def get_news_service() -> NewsService:
    """Get or create the singleton news service."""
    global _news_service
    if _news_service is None:
        _news_service = NewsService()
    return _news_service
