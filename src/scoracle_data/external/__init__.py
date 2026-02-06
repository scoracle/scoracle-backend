"""External API clients for Twitter, News, and Google News RSS."""

from ..core.http import BaseApiClient, ExternalAPIError, RateLimitError
from .twitter import TwitterClient
from .news import NewsClient
from .google_news import GoogleNewsClient

__all__ = [
    "BaseApiClient",
    "ExternalAPIError",
    "RateLimitError",
    "TwitterClient",
    "NewsClient",
    "GoogleNewsClient",
]
