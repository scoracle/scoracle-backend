"""
Rate limiting middleware for API protection.

Uses a sliding window algorithm with in-memory storage.
Configurable via settings: rate_limit_enabled, rate_limit_requests, rate_limit_window.
"""

import time
from collections import defaultdict
from dataclasses import dataclass, field
from threading import Lock
from typing import Callable

from fastapi import Request, Response
from fastapi.responses import JSONResponse
from starlette.middleware.base import BaseHTTPMiddleware

from ..config import get_settings


@dataclass
class RateLimitEntry:
    """Tracks request timestamps for a single client."""
    timestamps: list[float] = field(default_factory=list)


class RateLimiter:
    """
    Sliding window rate limiter.

    Tracks requests per IP within a configurable time window.
    Thread-safe for concurrent access.
    """

    def __init__(self, max_requests: int = 100, window_seconds: int = 60):
        self.max_requests = max_requests
        self.window_seconds = window_seconds
        self._storage: dict[str, RateLimitEntry] = defaultdict(RateLimitEntry)
        self._lock = Lock()

    def is_allowed(self, client_id: str) -> tuple[bool, int, int]:
        """
        Check if a request is allowed for the given client.

        Args:
            client_id: Unique identifier for the client (typically IP address)

        Returns:
            Tuple of (allowed, remaining_requests, reset_time_seconds)
        """
        now = time.time()
        window_start = now - self.window_seconds

        with self._lock:
            entry = self._storage[client_id]

            # Remove timestamps outside the window
            entry.timestamps = [ts for ts in entry.timestamps if ts > window_start]

            current_count = len(entry.timestamps)
            remaining = max(0, self.max_requests - current_count)

            # Calculate reset time (when oldest request expires from window)
            if entry.timestamps:
                reset_time = int(entry.timestamps[0] + self.window_seconds - now)
            else:
                reset_time = self.window_seconds

            if current_count >= self.max_requests:
                return False, 0, max(1, reset_time)

            # Record this request
            entry.timestamps.append(now)
            return True, remaining - 1, reset_time

    def cleanup(self) -> int:
        """
        Remove expired entries from storage.

        Returns:
            Number of entries removed
        """
        now = time.time()
        window_start = now - self.window_seconds
        removed = 0

        with self._lock:
            expired_keys = []
            for key, entry in self._storage.items():
                entry.timestamps = [ts for ts in entry.timestamps if ts > window_start]
                if not entry.timestamps:
                    expired_keys.append(key)

            for key in expired_keys:
                del self._storage[key]
                removed += 1

        return removed

    def get_stats(self) -> dict:
        """Get rate limiter statistics."""
        with self._lock:
            active_clients = len(self._storage)
            total_tracked = sum(len(e.timestamps) for e in self._storage.values())

        return {
            "active_clients": active_clients,
            "total_tracked_requests": total_tracked,
            "max_requests": self.max_requests,
            "window_seconds": self.window_seconds,
        }


# Global rate limiter instance
_rate_limiter: RateLimiter | None = None


def get_rate_limiter() -> RateLimiter:
    """Get or create the global rate limiter instance."""
    global _rate_limiter
    if _rate_limiter is None:
        settings = get_settings()
        _rate_limiter = RateLimiter(
            max_requests=settings.rate_limit_requests,
            window_seconds=settings.rate_limit_window,
        )
    return _rate_limiter


def get_client_ip(request: Request) -> str:
    """
    Extract client IP from request, considering proxy headers.

    Checks X-Forwarded-For first (for reverse proxy setups),
    then falls back to direct client IP.
    """
    # Check for forwarded IP (reverse proxy)
    forwarded = request.headers.get("X-Forwarded-For")
    if forwarded:
        # Take the first IP in the chain (original client)
        return forwarded.split(",")[0].strip()

    # Fall back to direct connection IP
    if request.client:
        return request.client.host

    return "unknown"


class RateLimitMiddleware(BaseHTTPMiddleware):
    """
    FastAPI middleware that enforces rate limiting.

    Adds standard rate limit headers to all responses:
    - X-RateLimit-Limit: Maximum requests per window
    - X-RateLimit-Remaining: Requests remaining in current window
    - X-RateLimit-Reset: Seconds until window resets

    Returns 429 Too Many Requests when limit exceeded.
    """

    # Paths to exclude from rate limiting
    EXCLUDED_PATHS = {"/health", "/health/db", "/health/cache", "/docs", "/redoc", "/openapi.json", "/"}

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        settings = get_settings()

        # Skip if rate limiting is disabled
        if not settings.rate_limit_enabled:
            return await call_next(request)

        # Skip excluded paths
        if request.url.path in self.EXCLUDED_PATHS:
            return await call_next(request)

        # Only rate limit GET requests to API endpoints
        if not request.url.path.startswith("/api/"):
            return await call_next(request)

        client_ip = get_client_ip(request)
        limiter = get_rate_limiter()

        allowed, remaining, reset_time = limiter.is_allowed(client_ip)

        if not allowed:
            return JSONResponse(
                status_code=429,
                content={
                    "error": {
                        "code": "RATE_LIMITED",
                        "message": "Too many requests. Please slow down.",
                        "retry_after": reset_time,
                    }
                },
                headers={
                    "X-RateLimit-Limit": str(limiter.max_requests),
                    "X-RateLimit-Remaining": "0",
                    "X-RateLimit-Reset": str(reset_time),
                    "Retry-After": str(reset_time),
                },
            )

        # Process request
        response = await call_next(request)

        # Add rate limit headers to successful responses
        response.headers["X-RateLimit-Limit"] = str(limiter.max_requests)
        response.headers["X-RateLimit-Remaining"] = str(remaining)
        response.headers["X-RateLimit-Reset"] = str(reset_time)

        return response
