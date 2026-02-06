"""External API clients for Twitter, News, and Google News RSS."""

from .base import BaseExternalClient, ExternalAPIError, RateLimitError
from .twitter import TwitterClient
from .news import NewsClient
from .google_news import GoogleNewsClient

__all__ = [
    "BaseExternalClient",
    "ExternalAPIError",
    "RateLimitError",
    "TwitterClient",
    "NewsClient",
    "GoogleNewsClient",
]
