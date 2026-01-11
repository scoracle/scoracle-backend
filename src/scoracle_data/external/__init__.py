"""External API clients for Twitter, News, Reddit, and Google News RSS."""

from .base import BaseExternalClient, ExternalAPIError, RateLimitError
from .twitter import TwitterClient
from .news import NewsClient
from .reddit import RedditClient
from .google_news import GoogleNewsClient

__all__ = [
    "BaseExternalClient",
    "ExternalAPIError",
    "RateLimitError",
    "TwitterClient",
    "NewsClient",
    "RedditClient",
    "GoogleNewsClient",
]
