"""
Data provider HTTP client base.

Re-exports BaseApiClient and RateLimiter from core.http for
backward compatibility. All shared HTTP infrastructure now lives
in core/http.py.
"""

from ..core.http import BaseApiClient, RateLimiter

__all__ = ["BaseApiClient", "RateLimiter"]
