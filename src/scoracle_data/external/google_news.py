"""Google News RSS client for fetching news articles about sports entities.

This client uses Google News RSS feeds which are free and don't require an API key.
It's used as the primary news source for the Scoracle frontend.
"""

import asyncio
import logging
import re
import xml.etree.ElementTree as ET
from datetime import datetime, timedelta
from typing import Any
from urllib.parse import quote_plus

import httpx

logger = logging.getLogger(__name__)


# Default number of articles to return
DEFAULT_LIMIT = 10
MAX_LIMIT = 50
MIN_ARTICLES = 3

# Time windows for escalation (hours)
TIME_WINDOWS = [24, 48, 168]  # 1 day, 2 days, 1 week


def _normalize(text: str) -> str:
    """Lowercase and strip whitespace."""
    return text.lower().strip() if text else ""


def _name_in_text(name: str, text: str) -> bool:
    """
    Check if entity name appears in text using word boundary matching.

    Handles cases like "LeBron James" in "LeBron James leads Lakers".
    """
    if not name or not text:
        return False

    name_lower = _normalize(name)
    text_lower = _normalize(text)

    # Try exact match first
    if name_lower in text_lower:
        return True

    # Try individual name parts (for "LeBron James" -> "LeBron" or "James")
    name_parts = name_lower.split()
    for part in name_parts:
        if len(part) > 2:  # Skip very short parts
            pattern = rf'\b{re.escape(part)}\b'
            if re.search(pattern, text_lower):
                return True

    return False


def _parse_pub_date(date_str: str) -> datetime | None:
    """Parse RSS pubDate to datetime."""
    if not date_str:
        return None

    # RSS date formats
    formats = [
        "%a, %d %b %Y %H:%M:%S %Z",
        "%a, %d %b %Y %H:%M:%S %z",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S%z",
    ]

    for fmt in formats:
        try:
            return datetime.strptime(date_str.strip(), fmt)
        except ValueError:
            continue

    return None


def _deduplicate(articles: list[dict]) -> list[dict]:
    """Remove duplicate articles by URL."""
    seen_urls = set()
    unique = []

    for article in articles:
        url = article.get("url", "")
        if url and url not in seen_urls:
            seen_urls.add(url)
            unique.append(article)

    return unique


def _sort_by_date(articles: list[dict]) -> list[dict]:
    """Sort articles by publish date, newest first."""
    def get_date(article):
        date_str = article.get("published_at", "")
        parsed = _parse_pub_date(date_str)
        return parsed or datetime.min

    return sorted(articles, key=get_date, reverse=True)


class GoogleNewsClient:
    """
    Google News RSS client for fetching sports news.

    Uses free RSS feeds - no API key required.
    Rate limit: Self-imposed 60 requests per minute to be respectful.
    """

    def __init__(self, timeout: float = 15.0):
        """
        Initialize Google News client.

        Args:
            timeout: Request timeout in seconds
        """
        self.timeout = timeout
        self._client: httpx.AsyncClient | None = None

    @property
    def client(self) -> httpx.AsyncClient:
        """Lazy-initialize HTTP client."""
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(
                timeout=httpx.Timeout(self.timeout),
                follow_redirects=True,
                headers={
                    "User-Agent": "Mozilla/5.0 (compatible; ScoracleBot/1.0)",
                    "Accept": "application/rss+xml, application/xml, text/xml",
                },
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client and not self._client.is_closed:
            await self._client.aclose()
            self._client = None

    def is_configured(self) -> bool:
        """Always returns True - no API key needed."""
        return True

    async def _fetch_rss(self, query: str, hours_back: int = 24) -> list[dict]:
        """
        Fetch articles from Google News RSS.

        Args:
            query: Search query
            hours_back: How many hours back to search

        Returns:
            List of article dictionaries
        """
        # Build Google News RSS URL
        # Format: https://news.google.com/rss/search?q=query&hl=en-US&gl=US&ceid=US:en
        encoded_query = quote_plus(query)

        # Add time filter if needed
        if hours_back <= 24:
            when = "1d"
        elif hours_back <= 168:
            when = "7d"
        else:
            when = "30d"

        url = f"https://news.google.com/rss/search?q={encoded_query}+when:{when}&hl=en-US&gl=US&ceid=US:en"

        try:
            response = await self.client.get(url)
            response.raise_for_status()

            # Parse RSS XML
            root = ET.fromstring(response.text)

            articles = []
            for item in root.findall(".//item"):
                try:
                    title = item.findtext("title", "")
                    link = item.findtext("link", "")
                    pub_date = item.findtext("pubDate", "")
                    description = item.findtext("description", "")

                    # Extract source from title (Google News format: "Title - Source")
                    source = "Google News"
                    if " - " in title:
                        parts = title.rsplit(" - ", 1)
                        if len(parts) == 2:
                            title = parts[0].strip()
                            source = parts[1].strip()

                    # Clean up description (remove HTML)
                    if description:
                        description = re.sub(r'<[^>]+>', '', description)
                        description = description[:300] + "..." if len(description) > 300 else description

                    articles.append({
                        "title": title,
                        "description": description,
                        "url": link,
                        "source": source,
                        "published_at": pub_date,
                        "image_url": None,  # RSS doesn't include images
                    })

                except Exception as e:
                    logger.warning(f"Failed to parse RSS item: {e}")
                    continue

            return articles

        except httpx.HTTPStatusError as e:
            logger.error(f"Google News RSS HTTP error: {e}")
            return []
        except ET.ParseError as e:
            logger.error(f"Google News RSS parse error: {e}")
            return []
        except Exception as e:
            logger.error(f"Google News RSS unexpected error: {e}")
            return []

    async def search(
        self,
        query: str,
        sport: str | None = None,
        team: str | None = None,
        limit: int = DEFAULT_LIMIT,
    ) -> dict[str, Any]:
        """
        Search for news articles about a sports entity.

        Uses time-range escalation if insufficient results are found.

        Args:
            query: Entity name (player/team name)
            sport: Optional sport context (NBA, NFL, FOOTBALL)
            team: Optional team name for additional context
            limit: Maximum number of results (1-50)

        Returns:
            Dictionary with articles and metadata
        """
        limit = min(max(1, limit), MAX_LIMIT)

        # Build search query with sport context
        search_query = query
        if sport:
            sport_terms = {
                "NBA": "NBA basketball",
                "NFL": "NFL football",
                "FOOTBALL": "soccer football",
            }
            search_query = f"{query} {sport_terms.get(sport.upper(), sport)}"

        # Try escalating time windows until we get enough results
        all_articles = []

        for hours in TIME_WINDOWS:
            articles = await self._fetch_rss(search_query, hours_back=hours)

            # Filter to articles that actually mention the entity
            filtered = [
                a for a in articles
                if _name_in_text(query, a.get("title", ""))
            ]

            all_articles.extend(filtered)
            all_articles = _deduplicate(all_articles)

            if len(all_articles) >= MIN_ARTICLES:
                break

            # Small delay before next request
            await asyncio.sleep(0.1)

        # Sort by date and limit
        all_articles = _sort_by_date(all_articles)[:limit]

        return {
            "query": query,
            "sport": sport,
            "articles": all_articles,
            "meta": {
                "total_results": len(all_articles),
                "returned": len(all_articles),
                "source": "google_news_rss",
            },
        }

    async def search_sport(
        self,
        sport: str,
        limit: int = DEFAULT_LIMIT,
    ) -> dict[str, Any]:
        """
        Get general news for a sport.

        Args:
            sport: Sport identifier (NBA, NFL, FOOTBALL)
            limit: Maximum number of results

        Returns:
            Dictionary with articles and metadata
        """
        sport_queries = {
            "NBA": "NBA basketball news",
            "NFL": "NFL football news",
            "FOOTBALL": "soccer football Premier League news",
        }

        query = sport_queries.get(sport.upper(), f"{sport} news")
        articles = await self._fetch_rss(query, hours_back=48)

        # Sort and limit
        articles = _sort_by_date(articles)[:limit]

        return {
            "sport": sport,
            "articles": articles,
            "meta": {
                "total_results": len(articles),
                "returned": len(articles),
                "source": "google_news_rss",
            },
        }
