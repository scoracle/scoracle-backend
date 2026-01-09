"""NewsAPI client for fetching news articles about sports entities."""

import os
import logging
from datetime import datetime, timedelta
from typing import Any

from .base import BaseExternalClient, ExternalAPIError

logger = logging.getLogger(__name__)


# Sport-specific news domains for better relevance
SPORT_DOMAINS = {
    "NBA": "espn.com,bleacherreport.com,nba.com,theathletic.com,cbssports.com",
    "NFL": "espn.com,bleacherreport.com,nfl.com,theathletic.com,cbssports.com",
    "FOOTBALL": "espn.com,skysports.com,bbc.com,goal.com,theathletic.com,theguardian.com",
}


class NewsClient(BaseExternalClient):
    """
    NewsAPI.org client for fetching news articles.

    Rate limit: 100 requests per day (free tier), 1000/day (paid).
    """

    def __init__(self, api_key: str | None = None):
        """
        Initialize News API client.

        Args:
            api_key: NewsAPI.org API key. If None, reads from
                     NEWS_API_KEY environment variable.
        """
        super().__init__(
            base_url="https://newsapi.org/v2",
            rate_limit=(100, 86400),  # 100 requests per day (free tier)
            timeout=15.0,
        )
        self.api_key = api_key or os.getenv("NEWS_API_KEY", "")

    def _get_auth_headers(self) -> dict[str, str]:
        """Return API key header."""
        return {"X-Api-Key": self.api_key}

    def is_configured(self) -> bool:
        """Check if API key is configured."""
        return bool(self.api_key)

    async def search(
        self,
        query: str,
        sport: str | None = None,
        days: int = 7,
        limit: int = 10,
    ) -> dict[str, Any]:
        """
        Search for news articles about a sports entity.

        Args:
            query: Search query (player name, team name)
            sport: Optional sport for domain filtering (NBA, NFL, FOOTBALL)
            days: How many days back to search (max 30 for free tier)
            limit: Maximum number of results (1-100)

        Returns:
            Dictionary with articles and metadata
        """
        if not self.is_configured():
            raise ExternalAPIError(
                "News API not configured. Set NEWS_API_KEY environment variable.",
                code="SERVICE_UNAVAILABLE",
                status_code=503,
            )

        # Calculate date range
        days = min(max(1, days), 30)  # Free tier limited to 30 days
        from_date = (datetime.utcnow() - timedelta(days=days)).strftime("%Y-%m-%d")

        params = {
            "q": query,
            "from": from_date,
            "sortBy": "relevancy",
            "pageSize": min(max(1, limit), 100),
            "language": "en",
        }

        # Add sport-specific domains for better relevance
        if sport and sport.upper() in SPORT_DOMAINS:
            params["domains"] = SPORT_DOMAINS[sport.upper()]

        try:
            response = await self.get("/everything", params=params)
        except ExternalAPIError:
            raise
        except Exception as e:
            logger.error(f"News API search failed: {e}")
            raise ExternalAPIError(f"News API search failed: {str(e)}")

        # Check for API errors
        if response.get("status") != "ok":
            error_msg = response.get("message", "Unknown error")
            raise ExternalAPIError(f"News API error: {error_msg}")

        # Parse articles
        articles = []
        for article in response.get("articles", []):
            try:
                # Extract source info
                source = article.get("source", {})
                source_name = source.get("name", "Unknown")

                articles.append({
                    "title": article.get("title", ""),
                    "description": article.get("description", ""),
                    "url": article.get("url", ""),
                    "source": source_name,
                    "author": article.get("author"),
                    "published_at": article.get("publishedAt", ""),
                    "image_url": article.get("urlToImage"),
                })
            except Exception as e:
                logger.warning(f"Failed to parse article: {e}")
                continue

        return {
            "query": query,
            "sport": sport,
            "articles": articles,
            "meta": {
                "total_results": response.get("totalResults", 0),
                "returned": len(articles),
            },
        }
