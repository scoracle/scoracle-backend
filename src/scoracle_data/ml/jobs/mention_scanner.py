"""
Mention Scanner Job

Scans news sources, Twitter, and Reddit for transfer/trade mentions.
Extracts player-team links and stores them for ML prediction.
"""

import asyncio
import logging
import re
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Any

from ..config import TRANSFER_KEYWORDS, get_tier_for_source
from ..pipelines.text_processing import TextProcessor

logger = logging.getLogger(__name__)


@dataclass
class MentionResult:
    """Result of a mention scan."""

    source: str
    player_id: int | None
    player_name: str
    team_id: int | None
    team_name: str
    content: str
    url: str | None
    published_at: datetime | None
    confidence: float
    tier: int
    keywords_found: list[str] = field(default_factory=list)


@dataclass
class ScanResult:
    """Result of a full scan run."""

    source_type: str  # news, twitter, reddit
    mentions_found: int
    mentions_stored: int
    errors: list[str] = field(default_factory=list)
    duration_seconds: float = 0.0


class MentionScanner:
    """
    Scans external sources for transfer/trade mentions.

    Supports:
    - Google News RSS (free, no API key)
    - Twitter API v2 (requires bearer token)
    - Reddit API (requires client credentials)
    """

    def __init__(self, db: Any, config: dict | None = None):
        """
        Initialize mention scanner.

        Args:
            db: Database connection
            config: Optional configuration overrides
        """
        self.db = db
        self.config = config or {}
        self.text_processor = TextProcessor()

        # Scan settings
        self.max_mentions_per_scan = self.config.get("max_mentions_per_scan", 100)
        self.lookback_hours = self.config.get("lookback_hours", 24)

    async def scan_all_sources(
        self,
        sport_id: str | None = None,
    ) -> dict[str, ScanResult]:
        """
        Scan all available sources for mentions.

        Args:
            sport_id: Optional filter by sport

        Returns:
            Dict mapping source type to scan result
        """
        results = {}

        # Always scan Google News (free)
        results["news"] = await self.scan_google_news(sport_id)

        # Scan Twitter if configured
        if self._has_twitter_config():
            results["twitter"] = await self.scan_twitter(sport_id)

        # Scan Reddit if configured
        if self._has_reddit_config():
            results["reddit"] = await self.scan_reddit(sport_id)

        return results

    async def scan_google_news(self, sport_id: str | None = None) -> ScanResult:
        """
        Scan Google News RSS for transfer mentions.

        Args:
            sport_id: Optional filter by sport

        Returns:
            Scan result with mentions found/stored
        """
        import time

        start_time = time.time()
        result = ScanResult(source_type="news", mentions_found=0, mentions_stored=0)

        try:
            # Build search queries based on sport
            queries = self._build_news_queries(sport_id)

            for query in queries:
                try:
                    articles = await self._fetch_google_news(query)
                    mentions = self._extract_mentions_from_articles(articles)

                    result.mentions_found += len(mentions)

                    for mention in mentions:
                        if self._store_mention(mention):
                            result.mentions_stored += 1

                except Exception as e:
                    result.errors.append(f"Query '{query}': {e}")

        except Exception as e:
            result.errors.append(f"News scan failed: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    async def scan_twitter(self, sport_id: str | None = None) -> ScanResult:
        """
        Scan Twitter for transfer mentions using API v2.

        Args:
            sport_id: Optional filter by sport

        Returns:
            Scan result with mentions found/stored
        """
        import os
        import time

        start_time = time.time()
        result = ScanResult(source_type="twitter", mentions_found=0, mentions_stored=0)

        bearer_token = os.getenv("TWITTER_BEARER_TOKEN")
        if not bearer_token:
            result.errors.append("TWITTER_BEARER_TOKEN not configured")
            return result

        try:
            # Build search queries
            queries = self._build_twitter_queries(sport_id)

            for query in queries:
                try:
                    tweets = await self._fetch_twitter_recent(query, bearer_token)
                    mentions = self._extract_mentions_from_tweets(tweets)

                    result.mentions_found += len(mentions)

                    for mention in mentions:
                        if self._store_mention(mention):
                            result.mentions_stored += 1

                except Exception as e:
                    result.errors.append(f"Twitter query '{query}': {e}")

        except Exception as e:
            result.errors.append(f"Twitter scan failed: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    async def scan_reddit(self, sport_id: str | None = None) -> ScanResult:
        """
        Scan Reddit for transfer mentions.

        Args:
            sport_id: Optional filter by sport

        Returns:
            Scan result with mentions found/stored
        """
        import os
        import time

        start_time = time.time()
        result = ScanResult(source_type="reddit", mentions_found=0, mentions_stored=0)

        client_id = os.getenv("REDDIT_CLIENT_ID")
        client_secret = os.getenv("REDDIT_CLIENT_SECRET")

        if not client_id or not client_secret:
            result.errors.append("Reddit API credentials not configured")
            return result

        try:
            # Get relevant subreddits
            subreddits = self._get_subreddits_for_sport(sport_id)

            for subreddit in subreddits:
                try:
                    posts = await self._fetch_reddit_posts(
                        subreddit, client_id, client_secret
                    )
                    mentions = self._extract_mentions_from_reddit(posts, subreddit)

                    result.mentions_found += len(mentions)

                    for mention in mentions:
                        if self._store_mention(mention):
                            result.mentions_stored += 1

                except Exception as e:
                    result.errors.append(f"Subreddit r/{subreddit}: {e}")

        except Exception as e:
            result.errors.append(f"Reddit scan failed: {e}")

        result.duration_seconds = time.time() - start_time
        return result

    def _build_news_queries(self, sport_id: str | None) -> list[str]:
        """Build Google News search queries."""
        base_terms = ["transfer", "signing", "deal", "trade"]

        if sport_id == "FOOTBALL":
            return [
                "football transfer news",
                "premier league transfer",
                "la liga transfer",
                "bundesliga transfer",
                "serie a transfer",
            ]
        elif sport_id == "NBA":
            return [
                "NBA trade news",
                "NBA free agent signing",
                "NBA trade rumors",
            ]
        elif sport_id == "NFL":
            return [
                "NFL trade news",
                "NFL free agent signing",
                "NFL trade rumors",
            ]
        else:
            # Scan all sports
            return [
                "football transfer news",
                "NBA trade news",
                "NFL trade news",
            ]

    def _build_twitter_queries(self, sport_id: str | None) -> list[str]:
        """Build Twitter search queries with operators."""
        if sport_id == "FOOTBALL":
            return [
                "(transfer OR signing) (football OR soccer) -filter:retweets",
                "from:FabrizioRomano",
                "from:David_Ornstein",
            ]
        elif sport_id == "NBA":
            return [
                "(trade OR signing) NBA -filter:retweets",
                "from:wojespn",
                "from:ShamsCharania",
            ]
        elif sport_id == "NFL":
            return [
                "(trade OR signing) NFL -filter:retweets",
                "from:AdamSchefter",
                "from:RapSheet",
            ]
        else:
            return [
                "(transfer OR trade) (football OR NBA OR NFL) -filter:retweets",
            ]

    def _get_subreddits_for_sport(self, sport_id: str | None) -> list[str]:
        """Get relevant subreddits for a sport."""
        subreddit_map = {
            "FOOTBALL": ["soccer", "PremierLeague", "Bundesliga", "LaLiga"],
            "NBA": ["nba", "nbadiscussion"],
            "NFL": ["nfl", "nfldraft"],
        }

        if sport_id and sport_id in subreddit_map:
            return subreddit_map[sport_id]

        # Return all if no sport filter
        all_subs = []
        for subs in subreddit_map.values():
            all_subs.extend(subs)
        return all_subs

    async def _fetch_google_news(self, query: str) -> list[dict]:
        """Fetch articles from Google News RSS."""
        import urllib.parse

        try:
            import httpx
        except ImportError:
            logger.warning("httpx not installed, skipping Google News fetch")
            return []

        encoded_query = urllib.parse.quote(query)
        url = f"https://news.google.com/rss/search?q={encoded_query}&hl=en-US&gl=US&ceid=US:en"

        async with httpx.AsyncClient() as client:
            response = await client.get(url, timeout=30.0)
            response.raise_for_status()

        # Parse RSS XML
        articles = self._parse_rss(response.text)
        return articles[:self.max_mentions_per_scan]

    def _parse_rss(self, xml_content: str) -> list[dict]:
        """Parse RSS XML into article dicts."""
        import xml.etree.ElementTree as ET

        articles = []
        try:
            root = ET.fromstring(xml_content)
            channel = root.find("channel")
            if channel is None:
                return []

            for item in channel.findall("item"):
                title = item.find("title")
                link = item.find("link")
                pub_date = item.find("pubDate")
                source = item.find("source")

                article = {
                    "title": title.text if title is not None else "",
                    "url": link.text if link is not None else "",
                    "published_at": pub_date.text if pub_date is not None else None,
                    "source": source.text if source is not None else "unknown",
                }
                articles.append(article)

        except ET.ParseError as e:
            logger.warning(f"Failed to parse RSS: {e}")

        return articles

    async def _fetch_twitter_recent(
        self, query: str, bearer_token: str
    ) -> list[dict]:
        """Fetch recent tweets using Twitter API v2."""
        try:
            import httpx
        except ImportError:
            logger.warning("httpx not installed, skipping Twitter fetch")
            return []

        url = "https://api.twitter.com/2/tweets/search/recent"
        headers = {"Authorization": f"Bearer {bearer_token}"}
        params = {
            "query": query,
            "max_results": min(100, self.max_mentions_per_scan),
            "tweet.fields": "created_at,author_id,text",
        }

        async with httpx.AsyncClient() as client:
            response = await client.get(url, headers=headers, params=params, timeout=30.0)
            response.raise_for_status()
            data = response.json()

        return data.get("data", [])

    async def _fetch_reddit_posts(
        self, subreddit: str, client_id: str, client_secret: str
    ) -> list[dict]:
        """Fetch recent posts from a subreddit."""
        try:
            import httpx
        except ImportError:
            logger.warning("httpx not installed, skipping Reddit fetch")
            return []

        # Get OAuth token
        auth_url = "https://www.reddit.com/api/v1/access_token"
        auth = (client_id, client_secret)
        data = {"grant_type": "client_credentials"}
        headers = {"User-Agent": "Scoracle/1.0"}

        async with httpx.AsyncClient() as client:
            auth_response = await client.post(
                auth_url, auth=auth, data=data, headers=headers, timeout=30.0
            )
            auth_response.raise_for_status()
            token = auth_response.json().get("access_token")

            if not token:
                return []

            # Fetch posts
            posts_url = f"https://oauth.reddit.com/r/{subreddit}/new"
            headers = {
                "Authorization": f"Bearer {token}",
                "User-Agent": "Scoracle/1.0",
            }
            params = {"limit": min(100, self.max_mentions_per_scan)}

            response = await client.get(
                posts_url, headers=headers, params=params, timeout=30.0
            )
            response.raise_for_status()
            data = response.json()

        posts = []
        for child in data.get("data", {}).get("children", []):
            post = child.get("data", {})
            posts.append(post)

        return posts

    def _extract_mentions_from_articles(self, articles: list[dict]) -> list[MentionResult]:
        """Extract transfer mentions from news articles."""
        mentions = []

        for article in articles:
            title = article.get("title", "")
            source = article.get("source", "unknown")

            # Check if title contains transfer keywords
            if not self._contains_transfer_keywords(title):
                continue

            # Extract entities from title
            extracted = self.text_processor.extract_transfer_mentions(title)

            for player_name, team_name, confidence in extracted:
                tier, _ = get_tier_for_source(source)

                # Try to resolve player/team IDs from database
                player_id = self._resolve_player_id(player_name)
                team_id = self._resolve_team_id(team_name)

                mention = MentionResult(
                    source=source,
                    player_id=player_id,
                    player_name=player_name,
                    team_id=team_id,
                    team_name=team_name,
                    content=title,
                    url=article.get("url"),
                    published_at=self._parse_date(article.get("published_at")),
                    confidence=confidence,
                    tier=tier,
                    keywords_found=self._find_keywords(title),
                )
                mentions.append(mention)

        return mentions

    def _extract_mentions_from_tweets(self, tweets: list[dict]) -> list[MentionResult]:
        """Extract transfer mentions from tweets."""
        mentions = []

        for tweet in tweets:
            text = tweet.get("text", "")

            if not self._contains_transfer_keywords(text):
                continue

            extracted = self.text_processor.extract_transfer_mentions(text)

            for player_name, team_name, confidence in extracted:
                # Twitter is tier 4 by default, unless from known journalist
                tier = 4

                mention = MentionResult(
                    source="twitter",
                    player_id=self._resolve_player_id(player_name),
                    player_name=player_name,
                    team_id=self._resolve_team_id(team_name),
                    team_name=team_name,
                    content=text,
                    url=None,
                    published_at=self._parse_date(tweet.get("created_at")),
                    confidence=confidence,
                    tier=tier,
                    keywords_found=self._find_keywords(text),
                )
                mentions.append(mention)

        return mentions

    def _extract_mentions_from_reddit(
        self, posts: list[dict], subreddit: str
    ) -> list[MentionResult]:
        """Extract transfer mentions from Reddit posts."""
        mentions = []

        for post in posts:
            title = post.get("title", "")
            selftext = post.get("selftext", "")
            content = f"{title} {selftext}".strip()

            if not self._contains_transfer_keywords(content):
                continue

            extracted = self.text_processor.extract_transfer_mentions(content)

            for player_name, team_name, confidence in extracted:
                mention = MentionResult(
                    source=f"reddit/r/{subreddit}",
                    player_id=self._resolve_player_id(player_name),
                    player_name=player_name,
                    team_id=self._resolve_team_id(team_name),
                    team_name=team_name,
                    content=title,  # Store just title, not full selftext
                    url=f"https://reddit.com{post.get('permalink', '')}",
                    published_at=datetime.fromtimestamp(post.get("created_utc", 0)),
                    confidence=confidence,
                    tier=4,  # Reddit is always tier 4
                    keywords_found=self._find_keywords(content),
                )
                mentions.append(mention)

        return mentions

    def _contains_transfer_keywords(self, text: str) -> bool:
        """Check if text contains any transfer-related keywords."""
        text_lower = text.lower()
        return any(kw in text_lower for kw in TRANSFER_KEYWORDS)

    def _find_keywords(self, text: str) -> list[str]:
        """Find which transfer keywords are in the text."""
        text_lower = text.lower()
        return [kw for kw in TRANSFER_KEYWORDS if kw in text_lower]

    def _resolve_player_id(self, player_name: str) -> int | None:
        """Try to resolve player name to database ID."""
        if not player_name:
            return None

        try:
            result = self.db.fetchone(
                """
                SELECT id FROM players
                WHERE LOWER(full_name) = LOWER(%s)
                   OR LOWER(full_name) LIKE LOWER(%s)
                LIMIT 1
                """,
                (player_name, f"%{player_name}%"),
            )
            return result["id"] if result else None
        except Exception:
            return None

    def _resolve_team_id(self, team_name: str) -> int | None:
        """Try to resolve team name to database ID."""
        if not team_name:
            return None

        try:
            result = self.db.fetchone(
                """
                SELECT id FROM teams
                WHERE LOWER(name) = LOWER(%s)
                   OR LOWER(name) LIKE LOWER(%s)
                   OR LOWER(short_name) = LOWER(%s)
                LIMIT 1
                """,
                (team_name, f"%{team_name}%", team_name),
            )
            return result["id"] if result else None
        except Exception:
            return None

    def _parse_date(self, date_str: str | None) -> datetime | None:
        """Parse date string to datetime."""
        if not date_str:
            return None

        # Try common formats
        formats = [
            "%a, %d %b %Y %H:%M:%S %Z",  # RSS format
            "%Y-%m-%dT%H:%M:%S.%fZ",  # ISO format
            "%Y-%m-%dT%H:%M:%SZ",  # ISO format without microseconds
        ]

        for fmt in formats:
            try:
                return datetime.strptime(date_str, fmt)
            except ValueError:
                continue

        return None

    def _store_mention(self, mention: MentionResult) -> bool:
        """Store a mention in the database."""
        try:
            # Check for duplicate (same content + source within 24h)
            existing = self.db.fetchone(
                """
                SELECT id FROM transfer_mentions
                WHERE source = %s
                  AND content_hash = MD5(%s)
                  AND created_at > NOW() - INTERVAL '24 hours'
                """,
                (mention.source, mention.content),
            )

            if existing:
                return False  # Skip duplicate

            self.db.execute(
                """
                INSERT INTO transfer_mentions (
                    player_id, player_name, team_id, team_name,
                    source, source_tier, content, content_hash,
                    url, published_at, confidence, keywords
                ) VALUES (
                    %s, %s, %s, %s, %s, %s, %s, MD5(%s), %s, %s, %s, %s
                )
                """,
                (
                    mention.player_id,
                    mention.player_name,
                    mention.team_id,
                    mention.team_name,
                    mention.source,
                    mention.tier,
                    mention.content,
                    mention.content,  # For MD5 hash
                    mention.url,
                    mention.published_at,
                    mention.confidence,
                    mention.keywords_found,
                ),
            )
            return True

        except Exception as e:
            logger.warning(f"Failed to store mention: {e}")
            return False

    def _has_twitter_config(self) -> bool:
        """Check if Twitter API is configured."""
        import os
        return bool(os.getenv("TWITTER_BEARER_TOKEN"))

    def _has_reddit_config(self) -> bool:
        """Check if Reddit API is configured."""
        import os
        return bool(os.getenv("REDDIT_CLIENT_ID") and os.getenv("REDDIT_CLIENT_SECRET"))
