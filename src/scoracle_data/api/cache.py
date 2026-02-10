"""
High-performance caching layer with Redis support and in-memory fallback.

Since data updates at most once daily, we use aggressive TTLs:
- Historical data (past seasons): 24 hours
- Current season data: 1 hour
- Entity info (rarely changes): 24 hours
"""

from __future__ import annotations

import json
import logging
import os
import threading
from abc import ABC, abstractmethod
from datetime import datetime, timedelta, timezone
from typing import Any, Optional

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# TTL constants (seconds) -- single source of truth for all cache durations.
# Optimized for once-daily data updates.
# ---------------------------------------------------------------------------
TTL_HISTORICAL = 86400  # 24h -- past season data (never changes)
TTL_CURRENT_SEASON = 3600  # 1h  -- current season stats/profiles
TTL_ENTITY_INFO = 86400  # 24h -- player/team basic info
TTL_DEFAULT = 3600  # 1h  -- fallback
TTL_NEWS = 600  # 10m -- news articles (high churn)


class CacheBackend(ABC):
    """Abstract base class for cache backends."""

    @abstractmethod
    def get(self, key: str) -> Optional[Any]:
        """Get value from cache."""
        pass

    @abstractmethod
    def set(self, key: str, value: Any, ttl: int) -> None:
        """Set value in cache with TTL."""
        pass

    @abstractmethod
    def delete(self, key: str) -> None:
        """Delete key from cache."""
        pass

    @abstractmethod
    def clear(self) -> None:
        """Clear all cached values."""
        pass

    @abstractmethod
    def size(self) -> int:
        """Get number of cached entries."""
        pass

    @abstractmethod
    def exists(self, key: str) -> bool:
        """Check if key exists."""
        pass

    @abstractmethod
    def keys(self, pattern: str = "*") -> list[str]:
        """Get keys matching pattern."""
        pass


class InMemoryBackend(CacheBackend):
    """Thread-safe in-memory cache backend with max-size eviction."""

    MAX_ENTRIES = 10_000  # Prevent unbounded memory growth

    def __init__(self):
        self._cache: dict[str, tuple[Any, datetime]] = {}
        self._lock = threading.RLock()

    def get(self, key: str) -> Optional[Any]:
        with self._lock:
            if key in self._cache:
                value, expiry = self._cache[key]
                if datetime.now(tz=timezone.utc) < expiry:
                    return value
                else:
                    del self._cache[key]
        return None

    def set(self, key: str, value: Any, ttl: int) -> None:
        expiry = datetime.now(tz=timezone.utc) + timedelta(seconds=ttl)
        with self._lock:
            # Evict expired entries if at capacity
            if len(self._cache) >= self.MAX_ENTRIES and key not in self._cache:
                self._evict_expired_locked()
                # If still at capacity after evicting expired, drop oldest entry
                if len(self._cache) >= self.MAX_ENTRIES:
                    oldest_key = next(iter(self._cache))
                    del self._cache[oldest_key]
            self._cache[key] = (value, expiry)

    def _evict_expired_locked(self) -> None:
        """Remove expired entries (caller must hold lock)."""
        now = datetime.now(tz=timezone.utc)
        expired = [k for k, (_, exp) in self._cache.items() if now >= exp]
        for k in expired:
            del self._cache[k]

    def delete(self, key: str) -> None:
        with self._lock:
            self._cache.pop(key, None)

    def clear(self) -> None:
        with self._lock:
            self._cache.clear()

    def size(self) -> int:
        with self._lock:
            return len(self._cache)

    def exists(self, key: str) -> bool:
        with self._lock:
            if key in self._cache:
                _, expiry = self._cache[key]
                return datetime.now(tz=timezone.utc) < expiry
            return False

    def keys(self, pattern: str = "*") -> list[str]:
        import fnmatch

        with self._lock:
            now = datetime.now(tz=timezone.utc)
            return [
                k
                for k, (_, exp) in self._cache.items()
                if now < exp and fnmatch.fnmatch(k, pattern)
            ]

    def cleanup_expired(self) -> int:
        """Remove all expired entries. Returns count of removed entries."""
        now = datetime.now(tz=timezone.utc)
        removed = 0
        with self._lock:
            expired_keys = [
                key for key, (_, expiry) in self._cache.items() if now >= expiry
            ]
            for key in expired_keys:
                del self._cache[key]
                removed += 1
        return removed


class RedisBackend(CacheBackend):
    """Redis cache backend for distributed caching."""

    def __init__(self, url: str, prefix: str = "scoracle:"):
        try:
            import redis

            self._redis = redis.from_url(url, decode_responses=False)
            self._prefix = prefix
            # Test connection
            self._redis.ping()
            logger.info("Redis cache backend connected")
        except Exception as e:
            logger.warning(f"Redis connection failed: {e}")
            raise

    def _key(self, key: str) -> str:
        return f"{self._prefix}{key}"

    def get(self, key: str) -> Optional[Any]:
        try:
            data = self._redis.get(self._key(key))
            if data:
                return json.loads(data)
        except Exception as e:
            logger.warning(f"Redis get error: {e}")
        return None

    def set(self, key: str, value: Any, ttl: int) -> None:
        try:
            self._redis.setex(self._key(key), ttl, json.dumps(value, default=str))
        except Exception as e:
            logger.warning(f"Redis set error: {e}")

    def delete(self, key: str) -> None:
        try:
            self._redis.delete(self._key(key))
        except Exception as e:
            logger.warning(f"Redis delete error: {e}")

    def clear(self) -> None:
        try:
            keys = self._redis.keys(f"{self._prefix}*")
            if keys:
                self._redis.delete(*keys)
        except Exception as e:
            logger.warning(f"Redis clear error: {e}")

    def size(self) -> int:
        try:
            return len(self._redis.keys(f"{self._prefix}*"))
        except Exception as e:
            logger.warning(f"Redis size error: {e}")
            return 0

    def exists(self, key: str) -> bool:
        try:
            return bool(self._redis.exists(self._key(key)))
        except Exception:
            return False

    def keys(self, pattern: str = "*") -> list[str]:
        try:
            prefix_len = len(self._prefix)
            keys = self._redis.keys(f"{self._prefix}{pattern}")
            return [
                k.decode()[prefix_len:] if isinstance(k, bytes) else k[prefix_len:]
                for k in keys
            ]
        except Exception as e:
            logger.warning(f"Redis keys error: {e}")
            return []


class HybridCache:
    """
    High-performance cache with Redis primary and in-memory fallback.

    Features:
    - Redis for distributed caching across workers
    - In-memory L1 cache for ultra-fast repeated access
    - Automatic fallback if Redis unavailable
    - TTL-aware for data freshness
    """

    def __init__(
        self,
        redis_url: Optional[str] = None,
        default_ttl: int = TTL_DEFAULT,
        enable_l1_cache: bool = True,
        l1_ttl: int = 60,  # Short L1 TTL to stay fresh
    ):
        self.default_ttl = default_ttl
        self.enable_l1_cache = enable_l1_cache
        self.l1_ttl = l1_ttl
        self._stats = {"hits": 0, "misses": 0, "l1_hits": 0}

        # L1 in-memory cache (always available)
        self._l1 = InMemoryBackend()

        # Primary backend
        self._primary: CacheBackend
        redis_url = redis_url or os.environ.get("REDIS_URL")

        if redis_url:
            try:
                self._primary = RedisBackend(redis_url)
                self._using_redis = True
                logger.info("Using Redis as primary cache")
            except Exception:
                logger.info("Redis unavailable, using in-memory cache")
                self._primary = InMemoryBackend()
                self._using_redis = False
                # L1 only adds value over a network hop — disable when primary is in-memory
                self.enable_l1_cache = False
        else:
            logger.info("No REDIS_URL configured, using in-memory cache")
            self._primary = InMemoryBackend()
            self._using_redis = False
            # L1 only adds value over a network hop — disable when primary is in-memory
            self.enable_l1_cache = False

    def _make_key(self, *args) -> str:
        """Create structured cache key from arguments.

        Uses colon-delimited human-readable keys (e.g. "stats:NBA:player:123:2025")
        instead of MD5 hashes. This enables pattern-based invalidation by sport,
        entity type, or season.
        """
        return ":".join(str(arg) for arg in args)

    def get(self, *args) -> Optional[Any]:
        """
        Get cached value with L1 check first.

        Args:
            *args: Arguments to create cache key from

        Returns:
            Cached value if exists and not expired, None otherwise
        """
        key = self._make_key(*args)

        # Check L1 cache first (ultra-fast path)
        if self.enable_l1_cache:
            value = self._l1.get(key)
            if value is not None:
                self._stats["l1_hits"] += 1
                self._stats["hits"] += 1
                return value

        # Check primary cache
        value = self._primary.get(key)
        if value is not None:
            self._stats["hits"] += 1
            # Populate L1 for faster subsequent access
            if self.enable_l1_cache:
                self._l1.set(key, value, self.l1_ttl)
            return value

        self._stats["misses"] += 1
        return None

    def set(self, value: Any, *args, ttl: Optional[int] = None) -> None:
        """
        Set cached value with TTL.

        Args:
            value: Value to cache
            *args: Arguments to create cache key from
            ttl: Time-to-live in seconds (uses default_ttl if None)
        """
        key = self._make_key(*args)
        ttl = ttl or self.default_ttl

        # Set in primary
        self._primary.set(key, value, ttl)

        # Set in L1 with shorter TTL
        if self.enable_l1_cache:
            self._l1.set(key, value, min(ttl, self.l1_ttl))

    def delete(self, *args) -> None:
        """Delete cached value."""
        key = self._make_key(*args)
        self._primary.delete(key)
        if self.enable_l1_cache:
            self._l1.delete(key)

    def invalidate_by_pattern(self, pattern: str) -> int:
        """
        Invalidate all cache entries matching a pattern.

        Args:
            pattern: Glob-style pattern (e.g., "*|player|*|NBA|*")

        Returns:
            Number of entries invalidated
        """
        count = 0
        keys = self._primary.keys(pattern)
        for key in keys:
            self._primary.delete(key)
            count += 1
        if self.enable_l1_cache:
            l1_keys = self._l1.keys(pattern)
            for key in l1_keys:
                self._l1.delete(key)
        return count

    def invalidate_stats_cache(self, sport_id: str, season: int | None = None) -> int:
        """
        Invalidate stats cache entries for a sport (and optionally season).

        With structured keys (e.g. "stats:NBA:player:123:2025"), we can
        selectively invalidate only the affected sport/season.

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season: Optional season year to limit invalidation

        Returns:
            Number of entries invalidated
        """
        if season:
            pattern = f"*:{sport_id}:*:{season}*"
        else:
            pattern = f"*:{sport_id}:*"

        count = self.invalidate_by_pattern(pattern)
        logger.info(f"Invalidated {count} cache entries for {sport_id} stats refresh")
        return count

    def clear(self) -> None:
        """Clear all cached values."""
        self._primary.clear()
        self._l1.clear()

    def size(self) -> int:
        """Get number of cached entries in primary."""
        return self._primary.size()

    def get_stats(self) -> dict[str, Any]:
        """Get cache statistics."""
        total = self._stats["hits"] + self._stats["misses"]
        hit_rate = (self._stats["hits"] / total * 100) if total > 0 else 0
        return {
            "hits": self._stats["hits"],
            "misses": self._stats["misses"],
            "l1_hits": self._stats["l1_hits"],
            "hit_rate_percent": round(hit_rate, 2),
            "total_requests": total,
            "primary_entries": self._primary.size(),
            "l1_entries": self._l1.size() if self.enable_l1_cache else 0,
            "using_redis": self._using_redis,
            "default_ttl": self.default_ttl,
        }

    def cleanup_expired(self) -> None:
        """Clean up expired entries from in-memory caches."""
        if isinstance(self._primary, InMemoryBackend):
            self._primary.cleanup_expired()
        self._l1.cleanup_expired()


# Global cache instance

_cache: Optional[HybridCache] = None


def get_cache() -> HybridCache:
    """
    Get the global cache instance.

    Returns:
        HybridCache instance (Redis if available, in-memory fallback)
    """
    global _cache
    if _cache is None:
        _cache = HybridCache(default_ttl=TTL_DEFAULT)
    return _cache


def get_ttl_for_season(season_year: int, current_season_year: int) -> int:
    """
    Get appropriate TTL based on whether season is historical or current.

    Args:
        season_year: The season year being cached
        current_season_year: The current season year

    Returns:
        TTL in seconds (24h for historical, 1h for current)
    """
    if season_year < current_season_year:
        return TTL_HISTORICAL  # 24 hours for past seasons
    return TTL_CURRENT_SEASON  # 1 hour for current season


def get_ttl_for_entity() -> int:
    """Get TTL for entity info (player/team basic info)."""
    return TTL_ENTITY_INFO  # 24 hours - this data rarely changes
