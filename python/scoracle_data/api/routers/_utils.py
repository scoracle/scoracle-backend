"""
Shared utilities for API routers.

Contains common functions used across multiple routers:
- ETag generation and validation
- Cache header management
- Season validation and lookup
"""

import hashlib
import json
from datetime import datetime
from typing import Any

from fastapi import Response

from ...core.types import get_sport_config

# Season validation constants
MIN_SEASON_YEAR = 2000
MAX_SEASON_YEAR_OFFSET = 1  # Current year + 1

# In-memory cache for season ID lookups (seasons rarely change)
_season_id_cache: dict[tuple[str, int], int] = {}


def _get_current_season(sport: str) -> int:
    """Get current season for a sport from SPORT_REGISTRY."""
    cfg = get_sport_config(sport)
    return cfg.current_season if cfg else datetime.now().year


async def get_season_id(db, sport: str, season_year: int) -> int | None:
    """
    Validate that a season year has data, with in-memory caching.

    In the v5.0 unified schema there is no separate `seasons` table.
    The season is stored as an integer directly on the stats tables.
    This function checks whether any stats exist for the sport + season
    combination and returns the year itself as the "ID" (for backward
    compatibility with the stats router which passes season_id).

    Args:
        db: Async database connection (AsyncPostgresDB)
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        season_year: Season year (e.g., 2025)

    Returns:
        The season year if data exists, or None if no stats found
    """
    cache_key = (sport, season_year)
    if cache_key in _season_id_cache:
        return _season_id_cache[cache_key]

    # Check whether any stats rows exist for this sport + season
    row = await db.fetchone(
        "SELECT 1 FROM player_stats WHERE sport = %s AND season = %s LIMIT 1",
        (sport, season_year),
    )
    if not row:
        # Also check team_stats in case only team data exists
        row = await db.fetchone(
            "SELECT 1 FROM team_stats WHERE sport = %s AND season = %s LIMIT 1",
            (sport, season_year),
        )

    if row:
        _season_id_cache[cache_key] = season_year
        return season_year
    return None


def validate_season(season: int | None, sport: str) -> int:
    """
    Validate and normalize season parameter.

    Args:
        season: Season year (can be None to use default)
        sport: Sport identifier

    Returns:
        Validated season year

    Raises:
        ValidationError: If season is out of valid range
    """
    from ..errors import ValidationError

    if season is None:
        return _get_current_season(sport)

    current_year = datetime.now().year
    max_season = current_year + MAX_SEASON_YEAR_OFFSET

    if season < MIN_SEASON_YEAR:
        raise ValidationError(
            message=f"Season year must be {MIN_SEASON_YEAR} or later",
            detail=f"Received: {season}",
        )

    if season > max_season:
        raise ValidationError(
            message=f"Season year cannot be more than {MAX_SEASON_YEAR_OFFSET} year(s) in the future",
            detail=f"Received: {season}, max allowed: {max_season}",
        )

    return season


def set_cache_headers(response: Response, ttl: int, cache_hit: bool) -> None:
    """Set standard cache headers."""
    response.headers["X-Cache"] = "HIT" if cache_hit else "MISS"
    response.headers["Cache-Control"] = (
        f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"
    )


def compute_etag(data: Any) -> str:
    """
    Compute ETag from response data.

    Uses MD5 hash of the JSON representation for fast computation.
    The 'W/' prefix indicates a weak validator (semantically equivalent content).
    """
    content = json.dumps(data, sort_keys=True, default=str)
    hash_value = hashlib.md5(content.encode()).hexdigest()[:16]
    return f'W/"{hash_value}"'


def check_etag_match(if_none_match: str | None, etag: str) -> bool:
    """
    Check if the If-None-Match header matches the current ETag.

    Returns True if matches (client should receive 304).
    Handles comma-separated ETags and the special '*' value.
    """
    if not if_none_match:
        return False

    # Handle * which matches any etag
    if if_none_match.strip() == "*":
        return True

    # Parse comma-separated ETags
    client_etags = [e.strip() for e in if_none_match.split(",")]

    # Compare with or without weak validator prefix
    etag_value = etag.lstrip("W/")
    for client_etag in client_etags:
        client_value = client_etag.lstrip("W/")
        if client_value == etag_value or client_etag == etag:
            return True

    return False


def set_etag_headers(response: Response, etag: str, ttl: int) -> None:
    """Set ETag and cache headers for conditional request support."""
    response.headers["ETag"] = etag
    response.headers["Vary"] = "Accept-Encoding"


def get_stats_ttl(sport: str, season: int) -> int:
    """Get cache TTL based on whether season is current or historical."""
    from ..cache import TTL_CURRENT_SEASON, TTL_HISTORICAL

    current = _get_current_season(sport)
    return TTL_CURRENT_SEASON if season >= current else TTL_HISTORICAL
