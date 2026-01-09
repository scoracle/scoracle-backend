"""Twitter/X API v2 client for fetching tweets about sports entities."""

import os
import logging
from dataclasses import dataclass
from datetime import datetime
from typing import Any

from .base import BaseExternalClient, ExternalAPIError

logger = logging.getLogger(__name__)


@dataclass
class TweetAuthor:
    """Tweet author information."""
    id: str
    username: str
    name: str
    verified: bool
    profile_image_url: str | None


@dataclass
class TweetMetrics:
    """Tweet engagement metrics."""
    likes: int
    retweets: int
    replies: int
    quotes: int


@dataclass
class Tweet:
    """Normalized tweet data."""
    id: str
    text: str
    author: TweetAuthor
    created_at: str
    metrics: TweetMetrics
    url: str


class TwitterClient(BaseExternalClient):
    """
    Twitter/X API v2 client.

    Uses the recent search endpoint to find tweets about players/teams.
    Rate limit: 450 requests per 15 minutes (Essential tier).
    """

    def __init__(self, bearer_token: str | None = None):
        """
        Initialize Twitter client.

        Args:
            bearer_token: Twitter API v2 bearer token. If None, reads from
                          TWITTER_BEARER_TOKEN environment variable.
        """
        super().__init__(
            base_url="https://api.twitter.com/2",
            rate_limit=(450, 900),  # 450 requests per 15 minutes
            timeout=15.0,
        )
        self.bearer_token = bearer_token or os.getenv("TWITTER_BEARER_TOKEN", "")

    def _get_auth_headers(self) -> dict[str, str]:
        """Return Bearer token auth header."""
        return {"Authorization": f"Bearer {self.bearer_token}"}

    def is_configured(self) -> bool:
        """Check if bearer token is configured."""
        return bool(self.bearer_token)

    def _build_search_query(self, query: str, sport: str | None = None) -> str:
        """
        Build Twitter search query with sport context.

        Args:
            query: Base search query (player/team name)
            sport: Optional sport for context (NBA, NFL, FOOTBALL)

        Returns:
            Enhanced search query
        """
        # Clean the query
        clean_query = query.strip()

        # Add sport-specific context
        sport_context = {
            "NBA": "basketball OR NBA",
            "NFL": "football OR NFL",
            "FOOTBALL": "soccer OR football OR Premier League OR Champions League",
        }

        if sport and sport.upper() in sport_context:
            return f'"{clean_query}" ({sport_context[sport.upper()]}) -is:retweet lang:en'

        return f'"{clean_query}" -is:retweet lang:en'

    def _parse_tweet(self, tweet_data: dict, users_map: dict[str, dict]) -> Tweet:
        """Parse raw API response into Tweet dataclass."""
        author_id = tweet_data.get("author_id", "")
        author_data = users_map.get(author_id, {})

        author = TweetAuthor(
            id=author_id,
            username=author_data.get("username", "unknown"),
            name=author_data.get("name", "Unknown"),
            verified=author_data.get("verified", False),
            profile_image_url=author_data.get("profile_image_url"),
        )

        public_metrics = tweet_data.get("public_metrics", {})
        metrics = TweetMetrics(
            likes=public_metrics.get("like_count", 0),
            retweets=public_metrics.get("retweet_count", 0),
            replies=public_metrics.get("reply_count", 0),
            quotes=public_metrics.get("quote_count", 0),
        )

        tweet_id = tweet_data.get("id", "")
        return Tweet(
            id=tweet_id,
            text=tweet_data.get("text", ""),
            author=author,
            created_at=tweet_data.get("created_at", ""),
            metrics=metrics,
            url=f"https://twitter.com/{author.username}/status/{tweet_id}",
        )

    async def search(
        self,
        query: str,
        sport: str | None = None,
        limit: int = 10,
    ) -> dict[str, Any]:
        """
        Search for recent tweets about a sports entity.

        Args:
            query: Search query (player name, team name)
            sport: Optional sport context (NBA, NFL, FOOTBALL)
            limit: Maximum number of results (1-100)

        Returns:
            Dictionary with tweets and metadata
        """
        if not self.is_configured():
            raise ExternalAPIError(
                "Twitter API not configured. Set TWITTER_BEARER_TOKEN environment variable.",
                code="SERVICE_UNAVAILABLE",
                status_code=503,
            )

        search_query = self._build_search_query(query, sport)
        limit = min(max(1, limit), 100)  # Clamp to 1-100

        params = {
            "query": search_query,
            "max_results": limit,
            "tweet.fields": "created_at,public_metrics,author_id",
            "user.fields": "username,name,verified,profile_image_url",
            "expansions": "author_id",
        }

        try:
            response = await self.get("/tweets/search/recent", params=params)
        except ExternalAPIError:
            raise
        except Exception as e:
            logger.error(f"Twitter search failed: {e}")
            raise ExternalAPIError(f"Twitter search failed: {str(e)}")

        # Build users map for author lookup
        users_map = {}
        includes = response.get("includes", {})
        for user in includes.get("users", []):
            users_map[user["id"]] = user

        # Parse tweets
        tweets = []
        for tweet_data in response.get("data", []):
            try:
                tweet = self._parse_tweet(tweet_data, users_map)
                tweets.append({
                    "id": tweet.id,
                    "text": tweet.text,
                    "author": {
                        "username": tweet.author.username,
                        "name": tweet.author.name,
                        "verified": tweet.author.verified,
                        "profile_image_url": tweet.author.profile_image_url,
                    },
                    "created_at": tweet.created_at,
                    "metrics": {
                        "likes": tweet.metrics.likes,
                        "retweets": tweet.metrics.retweets,
                        "replies": tweet.metrics.replies,
                    },
                    "url": tweet.url,
                })
            except Exception as e:
                logger.warning(f"Failed to parse tweet: {e}")
                continue

        meta = response.get("meta", {})

        return {
            "query": query,
            "sport": sport,
            "tweets": tweets,
            "meta": {
                "result_count": meta.get("result_count", len(tweets)),
                "newest_id": meta.get("newest_id"),
                "oldest_id": meta.get("oldest_id"),
            },
        }
