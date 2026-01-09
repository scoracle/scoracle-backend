"""Reddit API client for fetching posts about sports entities."""

import os
import logging
import time
from datetime import datetime
from typing import Any

import httpx

from .base import BaseExternalClient, ExternalAPIError

logger = logging.getLogger(__name__)


# Sport to subreddit mapping
SPORT_SUBREDDITS = {
    "NBA": "nba",
    "NFL": "nfl",
    "FOOTBALL": "soccer",
}

# Default subreddits to search if no sport specified
DEFAULT_SUBREDDITS = ["nba", "nfl", "soccer"]


class RedditClient(BaseExternalClient):
    """
    Reddit API client using OAuth2.

    Rate limit: 100 requests per minute with OAuth.
    Uses client credentials flow (application-only).
    """

    def __init__(
        self,
        client_id: str | None = None,
        client_secret: str | None = None,
    ):
        """
        Initialize Reddit client.

        Args:
            client_id: Reddit app client ID. If None, reads from
                       REDDIT_CLIENT_ID environment variable.
            client_secret: Reddit app client secret. If None, reads from
                          REDDIT_CLIENT_SECRET environment variable.
        """
        super().__init__(
            base_url="https://oauth.reddit.com",
            rate_limit=(100, 60),  # 100 requests per minute
            timeout=15.0,
        )
        self.client_id = client_id or os.getenv("REDDIT_CLIENT_ID", "")
        self.client_secret = client_secret or os.getenv("REDDIT_CLIENT_SECRET", "")

        # OAuth token management
        self._access_token: str | None = None
        self._token_expires: float = 0

    def _get_auth_headers(self) -> dict[str, str]:
        """Return OAuth bearer token header."""
        return {
            "Authorization": f"Bearer {self._access_token}",
            "User-Agent": "Scoracle/1.0 (sports stats aggregator)",
        }

    def is_configured(self) -> bool:
        """Check if client credentials are configured."""
        return bool(self.client_id and self.client_secret)

    async def _ensure_token(self) -> None:
        """Ensure we have a valid OAuth access token."""
        if self._access_token and time.time() < self._token_expires - 60:
            return  # Token still valid (with 60s buffer)

        if not self.is_configured():
            raise ExternalAPIError(
                "Reddit API not configured. Set REDDIT_CLIENT_ID and REDDIT_CLIENT_SECRET environment variables.",
                code="SERVICE_UNAVAILABLE",
                status_code=503,
            )

        # Request new token using client credentials
        auth = (self.client_id, self.client_secret)
        headers = {"User-Agent": "Scoracle/1.0 (sports stats aggregator)"}
        data = {"grant_type": "client_credentials"}

        async with httpx.AsyncClient() as client:
            try:
                response = await client.post(
                    "https://www.reddit.com/api/v1/access_token",
                    auth=auth,
                    headers=headers,
                    data=data,
                    timeout=10.0,
                )
                response.raise_for_status()
                token_data = response.json()

                self._access_token = token_data["access_token"]
                # Token expires_in is in seconds, typically 86400 (24 hours)
                self._token_expires = time.time() + token_data.get("expires_in", 3600)

                logger.info("Reddit OAuth token refreshed")

            except httpx.HTTPStatusError as e:
                raise ExternalAPIError(
                    f"Reddit OAuth failed: {e.response.status_code}",
                    status_code=e.response.status_code,
                )
            except Exception as e:
                raise ExternalAPIError(f"Reddit OAuth failed: {str(e)}")

    async def search(
        self,
        query: str,
        sport: str | None = None,
        sort: str = "relevance",
        limit: int = 10,
    ) -> dict[str, Any]:
        """
        Search for Reddit posts about a sports entity.

        Args:
            query: Search query (player name, team name)
            sport: Optional sport to determine subreddit (NBA, NFL, FOOTBALL)
            sort: Sort order: relevance, hot, new, top (default: relevance)
            limit: Maximum number of results (1-100)

        Returns:
            Dictionary with posts and metadata
        """
        await self._ensure_token()

        # Determine subreddit
        if sport and sport.upper() in SPORT_SUBREDDITS:
            subreddit = SPORT_SUBREDDITS[sport.upper()]
        else:
            subreddit = "+".join(DEFAULT_SUBREDDITS)  # Multi-subreddit search

        # Validate sort
        valid_sorts = ["relevance", "hot", "new", "top"]
        if sort not in valid_sorts:
            sort = "relevance"

        limit = min(max(1, limit), 100)

        params = {
            "q": query,
            "sort": sort,
            "limit": limit,
            "restrict_sr": "on" if sport else "off",
            "t": "week",  # Time filter: hour, day, week, month, year, all
        }

        endpoint = f"/r/{subreddit}/search"

        try:
            response = await self.get(endpoint, params=params)
        except ExternalAPIError:
            raise
        except Exception as e:
            logger.error(f"Reddit search failed: {e}")
            raise ExternalAPIError(f"Reddit search failed: {str(e)}")

        # Parse posts
        posts = []
        children = response.get("data", {}).get("children", [])

        for child in children:
            try:
                post_data = child.get("data", {})

                # Skip removed/deleted posts
                if post_data.get("removed_by_category") or post_data.get("selftext") == "[removed]":
                    continue

                post_id = post_data.get("id", "")
                subreddit_name = post_data.get("subreddit", "")
                permalink = post_data.get("permalink", "")

                posts.append({
                    "id": post_id,
                    "title": post_data.get("title", ""),
                    "selftext": post_data.get("selftext", "")[:500],  # Truncate long text
                    "author": post_data.get("author", "[deleted]"),
                    "subreddit": subreddit_name,
                    "score": post_data.get("score", 0),
                    "num_comments": post_data.get("num_comments", 0),
                    "created_utc": post_data.get("created_utc", 0),
                    "url": post_data.get("url", ""),
                    "permalink": permalink,
                    "full_url": f"https://reddit.com{permalink}" if permalink else "",
                    "is_self": post_data.get("is_self", True),
                    "thumbnail": post_data.get("thumbnail") if post_data.get("thumbnail", "").startswith("http") else None,
                    "link_flair_text": post_data.get("link_flair_text"),
                })
            except Exception as e:
                logger.warning(f"Failed to parse Reddit post: {e}")
                continue

        return {
            "query": query,
            "sport": sport,
            "subreddit": subreddit,
            "posts": posts,
            "meta": {
                "result_count": len(posts),
            },
        }
