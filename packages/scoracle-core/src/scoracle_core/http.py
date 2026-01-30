"""HTTP client with rate limiting and retry logic."""

import asyncio
import logging
import time
from typing import Any, AsyncIterator

import httpx

logger = logging.getLogger(__name__)


class RateLimiter:
    """Simple token bucket rate limiter."""
    
    def __init__(self, requests_per_minute: int):
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


class HTTPClient:
    """Async HTTP client with rate limiting and retries."""
    
    def __init__(
        self,
        base_url: str,
        headers: dict[str, str] | None = None,
        rate_limiter: RateLimiter | None = None,
        timeout: float = 30.0,
        max_retries: int = 3,
    ):
        self.base_url = base_url.rstrip("/")
        self.headers = headers or {}
        self.rate_limiter = rate_limiter
        self.timeout = timeout
        self.max_retries = max_retries
        self._client: httpx.AsyncClient | None = None
    
    async def __aenter__(self) -> "HTTPClient":
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            headers=self.headers,
            timeout=self.timeout,
        )
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None
    
    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise RuntimeError("HTTPClient not initialized. Use 'async with' or call __aenter__")
        return self._client
    
    async def get(
        self,
        path: str,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        Make a GET request with retries.
        
        Args:
            path: URL path (will be joined with base_url)
            params: Query parameters
        
        Returns:
            JSON response as dict
        
        Raises:
            httpx.HTTPStatusError: On non-2xx response after retries
        """
        last_error: Exception | None = None
        
        for attempt in range(self.max_retries):
            try:
                if self.rate_limiter:
                    await self.rate_limiter.acquire()
                
                response = await self.client.get(path, params=params)
                response.raise_for_status()
                return response.json()
                
            except httpx.HTTPStatusError as e:
                last_error = e
                status = e.response.status_code
                
                # Don't retry client errors (except rate limit)
                if 400 <= status < 500 and status != 429:
                    logger.error(f"Client error {status} for {path}: {e}")
                    raise
                
                # Retry on rate limit or server error
                if attempt < self.max_retries - 1:
                    wait_time = (attempt + 1) * 2  # Exponential backoff
                    logger.warning(
                        f"Request failed (attempt {attempt + 1}/{self.max_retries}), "
                        f"retrying in {wait_time}s: {e}"
                    )
                    await asyncio.sleep(wait_time)
                    
            except httpx.RequestError as e:
                last_error = e
                if attempt < self.max_retries - 1:
                    wait_time = (attempt + 1) * 2
                    logger.warning(
                        f"Request error (attempt {attempt + 1}/{self.max_retries}), "
                        f"retrying in {wait_time}s: {e}"
                    )
                    await asyncio.sleep(wait_time)
        
        # All retries failed
        raise last_error or RuntimeError("Request failed with unknown error")
    
    async def get_paginated(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        cursor_key: str = "cursor",
        meta_cursor_key: str = "next_cursor",
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Iterate through paginated responses.
        
        Yields each page's full response. Caller should extract data.
        """
        params = dict(params) if params else {}
        
        while True:
            response = await self.get(path, params)
            yield response
            
            # Check for next page
            meta = response.get("meta", {})
            next_cursor = meta.get(meta_cursor_key)
            
            if not next_cursor:
                break
            
            params[cursor_key] = next_cursor
