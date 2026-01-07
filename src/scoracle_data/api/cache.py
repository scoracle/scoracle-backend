"""In-memory caching layer with TTL support."""

from datetime import datetime, timedelta
from typing import Any, Optional
import hashlib
import threading


class SimpleCache:
    """Thread-safe in-memory cache with TTL."""

    def __init__(self, default_ttl: int = 300):
        """
        Initialize cache with default TTL.

        Args:
            default_ttl: Default time-to-live in seconds (default: 5 minutes)
        """
        self._cache: dict[str, tuple[Any, datetime]] = {}
        self._lock = threading.RLock()
        self.default_ttl = default_ttl

    def _make_key(self, *args) -> str:
        """
        Create cache key from arguments.

        Args:
            *args: Arguments to create key from

        Returns:
            MD5 hash of concatenated arguments
        """
        key_str = "|".join(str(arg) for arg in args)
        return hashlib.md5(key_str.encode()).hexdigest()

    def get(self, *args) -> Optional[Any]:
        """
        Get cached value if not expired.

        Args:
            *args: Arguments to create cache key from

        Returns:
            Cached value if exists and not expired, None otherwise
        """
        key = self._make_key(*args)

        with self._lock:
            if key in self._cache:
                value, expiry = self._cache[key]
                if datetime.utcnow() < expiry:
                    return value
                else:
                    # Remove expired entry
                    del self._cache[key]

        return None

    def set(self, value: Any, *args, ttl: Optional[int] = None):
        """
        Set cached value with TTL.

        Args:
            value: Value to cache
            *args: Arguments to create cache key from
            ttl: Time-to-live in seconds (uses default_ttl if None)
        """
        key = self._make_key(*args)
        ttl = ttl or self.default_ttl
        expiry = datetime.utcnow() + timedelta(seconds=ttl)

        with self._lock:
            self._cache[key] = (value, expiry)

    def clear(self):
        """Clear all cached values."""
        with self._lock:
            self._cache.clear()

    def size(self) -> int:
        """Get number of cached entries."""
        with self._lock:
            return len(self._cache)

    def cleanup_expired(self):
        """Remove all expired entries from cache."""
        now = datetime.utcnow()
        with self._lock:
            expired_keys = [
                key for key, (_, expiry) in self._cache.items()
                if now >= expiry
            ]
            for key in expired_keys:
                del self._cache[key]


# Global cache instance
_cache = SimpleCache(default_ttl=300)  # 5 minutes


def get_cache() -> SimpleCache:
    """
    Get the global cache instance.

    Returns:
        Global SimpleCache instance
    """
    return _cache
