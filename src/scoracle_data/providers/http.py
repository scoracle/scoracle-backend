"""
Shared HTTP client with rate limiting and retry logic.

Extracted from common patterns across BallDontLie and SportMonks clients.
All sport-specific provider clients inherit from BaseApiClient.
"""

import asyncio
import logging
import time
from typing import Any

import httpx

logger = logging.getLogger(__name__)


class RateLimiter:
    """Simple token bucket rate limiter for API calls."""

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


class BaseApiClient:
    """
    Async HTTP client base with rate limiting and retries.

    Subclasses set BASE_URL, configure auth, and add sport-specific methods.
    Use as an async context manager:

        async with MyClient(api_key="...") as client:
            data = await client._get("/endpoint")
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
    ):
        self._base_url = (base_url or self.BASE_URL).rstrip("/")
        self._default_headers = headers or {}
        self._default_params = params or {}
        self._rate_limiter = RateLimiter(requests_per_minute)
        self._timeout = timeout
        self._max_retries = max_retries
        self._client: httpx.AsyncClient | None = None

    async def __aenter__(self) -> "BaseApiClient":
        self._client = httpx.AsyncClient(
            base_url=self._base_url,
            headers=self._default_headers,
            timeout=self._timeout,
        )
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None

    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise RuntimeError("Client not initialized. Use 'async with'.")
        return self._client

    async def _get(
        self,
        path: str,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Make a GET request with retries and rate limiting."""
        merged_params = {**self._default_params, **(params or {})}
        last_error: Exception | None = None

        for attempt in range(self._max_retries):
            try:
                await self._rate_limiter.acquire()
                response = await self.client.get(path, params=merged_params)
                response.raise_for_status()
                return response.json()
            except httpx.HTTPStatusError as e:
                last_error = e
                status = e.response.status_code
                if 400 <= status < 500 and status != 429:
                    logger.error(f"Client error {status} for {path}")
                    raise
                if attempt < self._max_retries - 1:
                    wait = (attempt + 1) * 2
                    logger.warning(f"Request failed, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)
            except httpx.RequestError as e:
                last_error = e
                if attempt < self._max_retries - 1:
                    wait = (attempt + 1) * 2
                    logger.warning(f"Request error, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)

        raise last_error or RuntimeError("Request failed")
