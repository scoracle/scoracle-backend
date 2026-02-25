"""
Shared HTTP client infrastructure for all API integrations.

Provides BaseApiClient with rate limiting, retries, and error handling.
Used by both data handlers (BallDontLie, SportMonks) and
external API clients (Twitter, NewsAPI).

Usage:
    class MyClient(BaseApiClient):
        BASE_URL = "https://api.example.com"

        def __init__(self, api_key: str):
            super().__init__(
                headers={"Authorization": f"Bearer {api_key}"},
                requests_per_minute=60,
            )

        async def get_data(self) -> dict:
            return await self._get("/data")
"""

import asyncio
import logging
import time
from typing import Any

import httpx

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------

class ExternalAPIError(Exception):
    """Base exception for external API errors."""

    def __init__(
        self,
        message: str,
        code: str = "EXTERNAL_API_ERROR",
        status_code: int = 500,
    ):
        super().__init__(message)
        self.message = message
        self.code = code
        self.status_code = status_code


class RateLimitError(ExternalAPIError):
    """Exception raised when rate limit is exceeded."""

    def __init__(self, message: str, retry_after: int = 60):
        super().__init__(message, code="RATE_LIMITED", status_code=429)
        self.retry_after = retry_after


# ---------------------------------------------------------------------------
# Rate limiter
# ---------------------------------------------------------------------------

class RateLimiter:
    """Simple token bucket rate limiter for async API calls."""

    def __init__(self, requests_per_minute: int = 600):
        self.delay = 60.0 / requests_per_minute
        self._last_request = 0.0
        self._lock = asyncio.Lock()

    async def acquire(self) -> None:
        """Wait until we can make a request."""
        async with self._lock:
            now = time.monotonic()
            elapsed = now - self._last_request
            if elapsed < self.delay:
                await asyncio.sleep(self.delay - elapsed)
            self._last_request = time.monotonic()


# ---------------------------------------------------------------------------
# Base client
# ---------------------------------------------------------------------------

class BaseApiClient:
    """
    Async HTTP client base with rate limiting and retries.

    Subclasses set BASE_URL, configure auth, and add domain-specific methods.
    Use as an async context manager:

        async with MyClient(api_key="...") as client:
            data = await client._get("/endpoint")

    Or with lazy initialisation (for long-lived services):

        client = MyClient(api_key="...")
        data = await client._get("/endpoint")  # client auto-creates on first use
        await client.close()
    """

    BASE_URL: str = ""

    def __init__(
        self,
        *,
        base_url: str | None = None,
        headers: dict[str, str] | None = None,
        params: dict[str, str] | None = None,
        requests_per_minute: int = 600,
        timeout: float = 30.0,
        max_retries: int = 3,
        follow_redirects: bool = False,
    ):
        self._base_url = (base_url or self.BASE_URL).rstrip("/")
        self._default_headers = headers or {}
        self._default_params = params or {}
        self._rate_limiter = RateLimiter(requests_per_minute)
        self._timeout = timeout
        self._max_retries = max_retries
        self._follow_redirects = follow_redirects
        self._client: httpx.AsyncClient | None = None

    # -- Lifecycle -----------------------------------------------------------

    async def __aenter__(self) -> "BaseApiClient":
        self._client = httpx.AsyncClient(
            base_url=self._base_url,
            headers=self._default_headers,
            timeout=httpx.Timeout(self._timeout),
            follow_redirects=self._follow_redirects,
        )
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        await self.close()

    @property
    def client(self) -> httpx.AsyncClient:
        """Return the HTTP client, lazily creating it if needed."""
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(
                base_url=self._base_url,
                headers=self._default_headers,
                timeout=httpx.Timeout(self._timeout),
                follow_redirects=self._follow_redirects,
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client and not self._client.is_closed:
            await self._client.aclose()
            self._client = None

    def is_configured(self) -> bool:
        """
        Check if the client has required configuration (API keys, etc.).

        Override in subclasses that need configuration validation.
        """
        return True

    # -- HTTP methods --------------------------------------------------------

    async def _get(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Make a GET request with retries and rate limiting."""
        return await self._request("GET", path, params=params, headers=headers)

    async def _post(
        self,
        path: str,
        json: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Make a POST request with retries and rate limiting."""
        return await self._request("POST", path, params=params, json=json, headers=headers)

    async def _request(
        self,
        method: str,
        path: str,
        params: dict[str, Any] | None = None,
        json: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """
        Make an HTTP request with retry logic and rate limiting.

        Raises:
            RateLimitError: If API returns 429 and retries are exhausted
            ExternalAPIError: If request fails after retries
        """
        merged_params = {**self._default_params, **(params or {})}
        request_headers = {**self._default_headers, **(headers or {})}
        last_error: Exception | None = None

        for attempt in range(self._max_retries):
            try:
                await self._rate_limiter.acquire()
                response = await self.client.request(
                    method=method,
                    url=path,
                    params=merged_params,
                    json=json,
                    headers=request_headers,
                )

                # Handle API rate limiting
                if response.status_code == 429:
                    retry_after = int(response.headers.get("retry-after", 60))
                    if attempt < self._max_retries - 1:
                        wait = min(retry_after, 30)
                        logger.warning(
                            f"Rate limited by API, waiting {wait}s (attempt {attempt + 1})"
                        )
                        await asyncio.sleep(wait)
                        continue
                    raise RateLimitError(
                        f"API rate limit exceeded. Try again in {retry_after} seconds.",
                        retry_after=retry_after,
                    )

                response.raise_for_status()
                return response.json()

            except httpx.HTTPStatusError as e:
                status = e.response.status_code
                last_error = ExternalAPIError(
                    f"HTTP {status}: {e.response.text[:200]}",
                    status_code=status,
                )
                # Client errors (except 429) are not retryable
                if 400 <= status < 500 and status != 429:
                    raise last_error
                # Server errors: retry with backoff
                if attempt < self._max_retries - 1:
                    wait = 2 ** attempt
                    logger.warning(f"Request failed, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)

            except httpx.RequestError as e:
                last_error = ExternalAPIError(f"Request failed: {str(e)}")
                if attempt < self._max_retries - 1:
                    wait = 2 ** attempt
                    logger.warning(f"Request error, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)

        raise last_error or ExternalAPIError("Request failed after retries")
