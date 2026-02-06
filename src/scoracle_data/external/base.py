"""
External API client base.

Re-exports BaseApiClient, ExternalAPIError, and RateLimitError from
core.http for backward compatibility. All shared HTTP infrastructure
now lives in core/http.py.

BaseExternalClient is kept as a thin alias of BaseApiClient for
existing subclasses (TwitterClient, NewsClient).
"""

from ..core.http import BaseApiClient as BaseExternalClient, ExternalAPIError, RateLimitError

__all__ = ["BaseExternalClient", "ExternalAPIError", "RateLimitError"]
