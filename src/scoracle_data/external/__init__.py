"""External API clients for Twitter, News, and Google News RSS."""

from .base import BaseExternalClient, ExternalAPIError, RateLimitError
from .twitter import TwitterClient
from .news import NewsClient
from .google_news import GoogleNewsClient

# Note: RedditClient removed from exports (soft deprecated)
# The file reddit.py is kept for potential future ML use

__all__ = [
    "BaseExternalClient",
    "ExternalAPIError",
    "RateLimitError",
    "TwitterClient",
    "NewsClient",
    "GoogleNewsClient",
]
