"""Base client for external API integrations."""

import asyncio
import logging
import os
from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import Any

import httpx

logger = logging.getLogger(__name__)


class ExternalAPIError(Exception):
    """Base exception for external API errors."""

    def __init__(self, message: str, code: str = "EXTERNAL_API_ERROR", status_code: int = 500):
        super().__init__(message)
        self.message = message
        self.code = code
        self.status_code = status_code


class RateLimitError(ExternalAPIError):
    """Exception raised when rate limit is exceeded."""

    def __init__(self, message: str, retry_after: int = 60):
        super().__init__(message, code="RATE_LIMITED", status_code=429)
        self.retry_after = retry_after


@dataclass
class RateLimiter:
    """Simple token bucket rate limiter."""

    max_requests: int
    window_seconds: int
    _tokens: int = 0
    _last_refill: datetime = None

    def __post_init__(self):
        self._tokens = self.max_requests
        self._last_refill = datetime.utcnow()

    def _refill(self) -> None:
        """Refill tokens based on elapsed time."""
        now = datetime.utcnow()
        elapsed = (now - self._last_refill).total_seconds()
        refill_amount = int(elapsed * self.max_requests / self.window_seconds)
        if refill_amount > 0:
            self._tokens = min(self.max_requests, self._tokens + refill_amount)
            self._last_refill = now

    def acquire(self) -> bool:
        """Try to acquire a token. Returns True if successful."""
        self._refill()
        if self._tokens > 0:
            self._tokens -= 1
            return True
        return False

    def wait_time(self) -> float:
        """Returns seconds to wait before next token is available."""
        if self._tokens > 0:
            return 0
        return self.window_seconds / self.max_requests


class BaseExternalClient(ABC):
    """
    Base class for external API clients.

    Provides:
    - Async HTTP client with timeout
    - Retry logic with exponential backoff
    - Rate limiting
    - Error handling
    """

    def __init__(
        self,
        base_url: str,
        rate_limit: tuple[int, int],  # (max_requests, window_seconds)
        timeout: float = 10.0,
        max_retries: int = 3,
    ):
        self.base_url = base_url
        self.timeout = timeout
        self.max_retries = max_retries
        self.rate_limiter = RateLimiter(max_requests=rate_limit[0], window_seconds=rate_limit[1])
        self._client: httpx.AsyncClient | None = None

    @property
    def client(self) -> httpx.AsyncClient:
        """Lazy-initialize HTTP client."""
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(
                base_url=self.base_url,
                timeout=httpx.Timeout(self.timeout),
                follow_redirects=True,
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client and not self._client.is_closed:
            await self._client.aclose()
            self._client = None

    @abstractmethod
    def _get_auth_headers(self) -> dict[str, str]:
        """Return authentication headers. Override in subclasses."""
        pass

    async def _request(
        self,
        method: str,
        endpoint: str,
        params: dict[str, Any] | None = None,
        json: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """
        Make an HTTP request with retry logic and rate limiting.

        Args:
            method: HTTP method
            endpoint: API endpoint (appended to base_url)
            params: Query parameters
            json: JSON body
            headers: Additional headers

        Returns:
            Parsed JSON response

        Raises:
            RateLimitError: If rate limit exceeded
            ExternalAPIError: If request fails after retries
        """
        # Check rate limit
        if not self.rate_limiter.acquire():
            wait_time = self.rate_limiter.wait_time()
            raise RateLimitError(
                f"Rate limit exceeded. Try again in {int(wait_time)} seconds.",
                retry_after=int(wait_time),
            )

        # Merge headers
        request_headers = self._get_auth_headers()
        if headers:
            request_headers.update(headers)

        last_error = None

        for attempt in range(self.max_retries):
            try:
                response = await self.client.request(
                    method=method,
                    url=endpoint,
                    params=params,
                    json=json,
                    headers=request_headers,
                )

                # Handle rate limiting from the API
                if response.status_code == 429:
                    retry_after = int(response.headers.get("retry-after", 60))
                    if attempt < self.max_retries - 1:
                        logger.warning(f"Rate limited by API, waiting {retry_after}s (attempt {attempt + 1})")
                        await asyncio.sleep(min(retry_after, 30))  # Cap at 30s per retry
                        continue
                    raise RateLimitError(
                        f"API rate limit exceeded. Try again in {retry_after} seconds.",
                        retry_after=retry_after,
                    )

                response.raise_for_status()
                return response.json()

            except httpx.HTTPStatusError as e:
                last_error = ExternalAPIError(
                    f"HTTP {e.response.status_code}: {e.response.text[:200]}",
                    status_code=e.response.status_code,
                )
                if e.response.status_code >= 500:
                    # Server error, retry with backoff
                    if attempt < self.max_retries - 1:
                        wait = 2**attempt
                        logger.warning(f"Server error, retrying in {wait}s (attempt {attempt + 1})")
                        await asyncio.sleep(wait)
                        continue
                raise last_error

            except httpx.RequestError as e:
                last_error = ExternalAPIError(f"Request failed: {str(e)}")
                if attempt < self.max_retries - 1:
                    wait = 2**attempt
                    logger.warning(f"Request error, retrying in {wait}s (attempt {attempt + 1})")
                    await asyncio.sleep(wait)
                    continue
                raise last_error

        raise last_error or ExternalAPIError("Request failed after retries")

    async def get(
        self,
        endpoint: str,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Make a GET request."""
        return await self._request("GET", endpoint, params=params, headers=headers)

    async def post(
        self,
        endpoint: str,
        json: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Make a POST request."""
        return await self._request("POST", endpoint, params=params, json=json, headers=headers)

    @abstractmethod
    def is_configured(self) -> bool:
        """Check if the client has required configuration (API keys, etc.)."""
        pass
