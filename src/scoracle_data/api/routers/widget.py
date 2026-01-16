"""
Widget router - serves entity info and stats for frontend widgets.

Endpoints:
- GET /info/{entity_type}/{entity_id} - Basic entity info from players/teams tables
- GET /stats/{entity_type}/{entity_id} - Stats from sport-specific stats tables
- GET /stats/{entity_type}/{entity_id}/seasons - Available seasons for an entity

Performance Features:
- In-memory caching with TTLs
- ETag support for conditional requests (304 Not Modified)
- Unified profile endpoint to reduce API calls
"""

import hashlib
import logging
from datetime import datetime
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_ENTITY_INFO, TTL_CURRENT_SEASON, TTL_HISTORICAL
from ..dependencies import DBDependency
from ..errors import NotFoundError, ValidationError
from ..types import (
    EntityType,
    Sport,
    CURRENT_SEASONS,
    PLAYER_STATS_TABLES,
    TEAM_STATS_TABLES,
    PLAYER_PROFILE_TABLES,
    TEAM_PROFILE_TABLES,
    get_sport_config,
)

logger = logging.getLogger(__name__)

router = APIRouter()

# Season validation constants
MIN_SEASON_YEAR = 2000
MAX_SEASON_YEAR_OFFSET = 1  # Current year + 1


def _validate_season(season: int | None, sport: str) -> int:
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
    if season is None:
        return CURRENT_SEASONS.get(sport, datetime.now().year)

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


def _set_cache_headers(response: Response, ttl: int, cache_hit: bool) -> None:
    """Set standard cache headers."""
    response.headers["X-Cache"] = "HIT" if cache_hit else "MISS"
    response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"


def _compute_etag(data: Any) -> str:
    """
    Compute ETag from response data.

    Uses MD5 hash of the JSON representation for fast computation.
    The 'W/' prefix indicates a weak validator (semantically equivalent content).
    """
    import json
    content = json.dumps(data, sort_keys=True, default=str)
    hash_value = hashlib.md5(content.encode()).hexdigest()[:16]
    return f'W/"{hash_value}"'


def _check_etag_match(if_none_match: str | None, etag: str) -> bool:
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


def _set_etag_headers(response: Response, etag: str, ttl: int) -> None:
    """Set ETag and cache headers for conditional request support."""
    response.headers["ETag"] = etag
    response.headers["Vary"] = "Accept-Encoding"


def _get_stats_ttl(sport: str, season: int) -> int:
    """Get cache TTL based on whether season is current or historical."""
    current = CURRENT_SEASONS.get(sport, 2025)
    return TTL_CURRENT_SEASON if season >= current else TTL_HISTORICAL


# =============================================================================
# INFO ENDPOINT
# =============================================================================

@router.get("/info/{entity_type}/{entity_id}", response_model=None)
async def get_entity_info(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get basic entity info for widget rendering.

    Returns all fields from the players or teams table, with related
    team/league info included via JOINs.

    Supports conditional requests via If-None-Match header for ETag validation.
    Returns 304 Not Modified if content hasn't changed.
    """
    cache = get_cache()
    cache_key = ("info", entity_type.value, entity_id, sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = _compute_etag(cached)
        if _check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        _set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=True)
        _set_etag_headers(response, etag, TTL_ENTITY_INFO)
        return cached

    if entity_type == EntityType.player:
        result = _get_player_info(db, entity_id, sport.value)
    else:
        result = _get_team_info(db, entity_id, sport.value)

    if not result:
        raise NotFoundError(
            resource=entity_type.value.title(),
            identifier=entity_id,
            context=sport.value,
        )

    cache.set(result, *cache_key, ttl=TTL_ENTITY_INFO)
    etag = _compute_etag(result)
    _set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=False)
    _set_etag_headers(response, etag, TTL_ENTITY_INFO)
    return result


def _get_player_info(db, player_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch player info with team and league data from sport-specific tables."""
    player_table = PLAYER_PROFILE_TABLES.get(sport)
    team_table = TEAM_PROFILE_TABLES.get(sport)

    if not player_table or not team_table:
        return None

    # Get sport config to check if this sport uses leagues in profiles
    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    # Build query based on sport - no sport_id filtering needed (table IS the filter)
    if has_leagues:
        # Football: include league info
        query = f"""
            SELECT
                p.id, '{sport}' as sport_id, p.first_name, p.last_name, p.full_name,
                p.position, p.position_group, p.nationality,
                p.birth_date::text as birth_date, p.birth_place, p.birth_country,
                p.height_inches, p.weight_lbs, p.photo_url,
                p.current_team_id, p.current_league_id, p.jersey_number,
                p.is_active,
                t.id as team_id, t.name as team_name, t.abbreviation as team_abbr,
                t.logo_url as team_logo, t.country as team_country, t.city as team_city,
                l.id as league_id, l.name as league_name, l.country as league_country, l.logo_url as league_logo
            FROM {player_table} p
            LEFT JOIN {team_table} t ON t.id = p.current_team_id
            LEFT JOIN leagues l ON l.id = p.current_league_id
            WHERE p.id = %s
        """
    else:
        # NBA/NFL: no league, but include college/experience for American sports
        query = f"""
            SELECT
                p.id, '{sport}' as sport_id, p.first_name, p.last_name, p.full_name,
                p.position, p.position_group, p.nationality,
                p.birth_date::text as birth_date, p.birth_place, p.birth_country,
                p.height_inches, p.weight_lbs, p.photo_url,
                p.current_team_id, p.jersey_number,
                p.college, p.experience_years, p.is_active,
                t.id as team_id, t.name as team_name, t.abbreviation as team_abbr,
                t.logo_url as team_logo, t.conference, t.division, t.city as team_city
            FROM {player_table} p
            LEFT JOIN {team_table} t ON t.id = p.current_team_id
            WHERE p.id = %s
        """

    row = db.fetchone(query, (player_id,))

    if not row:
        return None

    data = dict(row)

    # Build nested team object
    team = None
    if data.get("team_id"):
        team = {
            "id": data.pop("team_id"),
            "name": data.pop("team_name"),
            "abbreviation": data.pop("team_abbr"),
            "logo_url": data.pop("team_logo"),
        }
        # Add conference/division for American sports
        if "conference" in data:
            team["conference"] = data.pop("conference")
            team["division"] = data.pop("division")
        # Add country for Football
        if "team_country" in data:
            team["country"] = data.pop("team_country")
        if "team_city" in data:
            team["city"] = data.pop("team_city")
    else:
        # Remove team fields even if null
        for k in ["team_id", "team_name", "team_abbr", "team_logo", "conference", "division", "team_city", "team_country"]:
            data.pop(k, None)

    # Build nested league object (Football only)
    league = None
    if has_leagues and data.get("league_id"):
        league = {
            "id": data.pop("league_id"),
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_id", "league_name", "league_country", "league_logo", "current_league_id"]:
            data.pop(k, None)

    data["team"] = team
    data["league"] = league

    return data


def _get_team_info(db, team_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch team info with league data from sport-specific tables."""
    team_table = TEAM_PROFILE_TABLES.get(sport)

    if not team_table:
        return None

    # Get sport config to check if this sport uses leagues
    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    # Build query based on sport - no sport_id filtering needed (table IS the filter)
    if has_leagues:
        # Football: include league info and is_national flag
        query = f"""
            SELECT
                t.id, '{sport}' as sport_id, t.league_id, t.name, t.abbreviation, t.logo_url,
                t.country, t.city, t.founded, t.is_national,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city, t.venue_surface, t.venue_image,
                t.is_active,
                l.name as league_name, l.country as league_country, l.logo_url as league_logo
            FROM {team_table} t
            LEFT JOIN leagues l ON l.id = t.league_id
            WHERE t.id = %s
        """
    else:
        # NBA/NFL: include conference/division
        query = f"""
            SELECT
                t.id, '{sport}' as sport_id, t.name, t.abbreviation, t.logo_url,
                t.conference, t.division, t.country, t.city, t.founded,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city, t.venue_surface, t.venue_image,
                t.is_active
            FROM {team_table} t
            WHERE t.id = %s
        """

    row = db.fetchone(query, (team_id,))

    if not row:
        return None

    data = dict(row)

    # Build nested league object (Football only)
    league = None
    if has_leagues and data.get("league_id"):
        league = {
            "id": data["league_id"],
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_name", "league_country", "league_logo"]:
            data.pop(k, None)

    data["league"] = league

    return data


# =============================================================================
# STATS ENDPOINT
# =============================================================================

@router.get("/stats/{entity_type}/{entity_id}", response_model=None)
async def get_entity_stats(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int | None, Query(description="Season year (defaults to current)")] = None,
    league_id: Annotated[int | None, Query(description="League ID (for FOOTBALL)")] = None,
    response: Response = None,
    db: DBDependency = None,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get entity statistics for a season.

    Returns all stat fields from the appropriate stats table.
    Season defaults to current season if not specified.

    Supports conditional requests via If-None-Match header.
    """
    # Validate and normalize season
    season = _validate_season(season, sport.value)

    cache = get_cache()
    cache_key = ("stats", entity_type.value, entity_id, sport.value, season, league_id)
    ttl = _get_stats_ttl(sport.value, season)

    cached = cache.get(*cache_key)
    if cached:
        etag = _compute_etag(cached)
        if _check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        _set_cache_headers(response, ttl, cache_hit=True)
        _set_etag_headers(response, etag, ttl)
        return cached

    # Get season ID
    season_row = db.fetchone(
        "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
        (sport.value, season),
    )
    if not season_row:
        raise NotFoundError(resource="Season", identifier=season, context=sport.value)

    season_id = season_row["id"]

    # Get stats
    if entity_type == EntityType.player:
        stats = _get_stats(db, PLAYER_STATS_TABLES, sport.value, "player_id", entity_id, season_id, league_id)
    else:
        stats = _get_stats(db, TEAM_STATS_TABLES, sport.value, "team_id", entity_id, season_id, league_id)

    if not stats:
        raise NotFoundError(
            resource=f"{entity_type.value.title()} stats",
            identifier=entity_id,
            context=f"{sport.value} {season}",
        )

    result = {
        "entity_id": entity_id,
        "entity_type": entity_type.value,
        "sport": sport.value,
        "season": season,
        "stats": stats,
    }
    if league_id:
        result["league_id"] = league_id

    cache.set(result, *cache_key, ttl=ttl)
    etag = _compute_etag(result)
    _set_cache_headers(response, ttl, cache_hit=False)
    _set_etag_headers(response, etag, ttl)
    return result


def _get_stats(
    db,
    table_map: dict[str, str],
    sport: str,
    id_column: str,
    entity_id: int,
    season_id: int,
    league_id: int | None,
) -> dict[str, Any] | None:
    """Fetch stats from sport-specific table."""
    table = table_map.get(sport)
    if not table:
        return None

    if sport == "FOOTBALL" and league_id:
        row = db.fetchone(
            f"SELECT * FROM {table} WHERE {id_column} = %s AND season_id = %s AND league_id = %s",
            (entity_id, season_id, league_id),
        )
    else:
        row = db.fetchone(
            f"SELECT * FROM {table} WHERE {id_column} = %s AND season_id = %s",
            (entity_id, season_id),
        )

    if not row:
        return None

    stats = dict(row)
    # Remove internal IDs
    for key in ["id", "player_id", "team_id", "season_id", "league_id"]:
        stats.pop(key, None)

    # Convert timestamp to string
    if stats.get("updated_at"):
        stats["updated_at"] = str(stats["updated_at"])

    return stats


# =============================================================================
# SEASONS ENDPOINT
# =============================================================================

@router.get("/stats/{entity_type}/{entity_id}/seasons", response_model=None)
async def get_available_seasons(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response = None,
    db: DBDependency = None,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get list of seasons with stats available for an entity.

    Useful for building season dropdown selectors.
    Supports conditional requests via If-None-Match header.
    """
    cache = get_cache()
    cache_key = ("seasons_available", entity_type.value, entity_id, sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = _compute_etag(cached)
        if _check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        _set_cache_headers(response, TTL_CURRENT_SEASON, cache_hit=True)
        _set_etag_headers(response, etag, TTL_CURRENT_SEASON)
        return cached

    table_map = PLAYER_STATS_TABLES if entity_type == EntityType.player else TEAM_STATS_TABLES
    table = table_map.get(sport.value)
    id_column = "player_id" if entity_type == EntityType.player else "team_id"

    rows = db.fetchall(
        f"""
        SELECT DISTINCT s.season_year
        FROM {table} st
        JOIN seasons s ON s.id = st.season_id
        WHERE st.{id_column} = %s AND s.sport_id = %s
        ORDER BY s.season_year DESC
        """,
        (entity_id, sport.value),
    )

    result = {
        "entity_id": entity_id,
        "entity_type": entity_type.value,
        "sport": sport.value,
        "seasons": [row["season_year"] for row in rows],
    }

    cache.set(result, *cache_key, ttl=TTL_CURRENT_SEASON)
    etag = _compute_etag(result)
    _set_cache_headers(response, TTL_CURRENT_SEASON, cache_hit=False)
    _set_etag_headers(response, etag, TTL_CURRENT_SEASON)
    return result


# =============================================================================
# UNIFIED PROFILE ENDPOINT (info + stats + percentiles in one call)
# =============================================================================

@router.get("/profile/{entity_type}/{entity_id}", response_model=None)
async def get_entity_profile(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int | None, Query(description="Season year (defaults to current)")] = None,
    league_id: Annotated[int | None, Query(description="League ID (for FOOTBALL)")] = None,
    include_percentiles: Annotated[bool, Query(description="Include percentile rankings")] = True,
    response: Response = None,
    db: DBDependency = None,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get complete entity profile in a single optimized request.

    Returns:
    - info: Basic entity information (from players/teams table)
    - stats: Season statistics (from sport-specific stats table)
    - percentiles: Stat percentile rankings (from percentile_cache)

    This endpoint eliminates the need for 3 separate API calls,
    reducing frontend latency and connection overhead.

    Supports conditional requests via If-None-Match header.
    """
    # Validate season
    season = _validate_season(season, sport.value)

    cache = get_cache()
    cache_key = ("profile", entity_type.value, entity_id, sport.value, season, league_id, include_percentiles)
    ttl = _get_stats_ttl(sport.value, season)

    cached = cache.get(*cache_key)
    if cached:
        etag = _compute_etag(cached)
        if _check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        _set_cache_headers(response, ttl, cache_hit=True)
        _set_etag_headers(response, etag, ttl)
        return cached

    # Build profile using optimized single query
    if entity_type == EntityType.player:
        result = _get_player_profile(db, entity_id, sport.value, season, include_percentiles)
    else:
        result = _get_team_profile(db, entity_id, sport.value, season, league_id, include_percentiles)

    if not result:
        raise NotFoundError(
            resource=entity_type.value.title(),
            identifier=entity_id,
            context=sport.value,
        )

    cache.set(result, *cache_key, ttl=ttl)
    etag = _compute_etag(result)
    _set_cache_headers(response, ttl, cache_hit=False)
    _set_etag_headers(response, etag, ttl)
    return result


def _get_player_profile(
    db, player_id: int, sport: str, season: int, include_percentiles: bool
) -> dict[str, Any] | None:
    """
    Fetch complete player profile in a single optimized query.

    Uses a JOIN to get player info, team info, and stats in one database call.
    Now uses sport-specific profile tables.
    """
    player_table = PLAYER_PROFILE_TABLES.get(sport)
    team_table = TEAM_PROFILE_TABLES.get(sport)
    stats_table = PLAYER_STATS_TABLES.get(sport)
    if not player_table or not team_table or not stats_table:
        return None

    # Get sport config
    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    # Get season ID
    season_row = db.fetchone(
        "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
        (sport, season),
    )
    if not season_row:
        return None
    season_id = season_row["id"]

    # Build query based on sport - no sport_id filtering needed (table IS the filter)
    if has_leagues:
        # Football: include league info
        query = f"""
            SELECT
                -- Player info
                p.id, '{sport}' as sport_id, p.first_name, p.last_name, p.full_name,
                p.position, p.position_group, p.nationality,
                p.birth_date::text as birth_date, p.birth_place, p.birth_country,
                p.height_inches, p.weight_lbs, p.photo_url,
                p.current_team_id, p.current_league_id, p.jersey_number, p.is_active,
                -- Team info
                t.id as team_id, t.name as team_name, t.abbreviation as team_abbr,
                t.logo_url as team_logo, t.country as team_country, t.city as team_city,
                -- League info
                l.id as league_id, l.name as league_name, l.country as league_country,
                l.logo_url as league_logo,
                -- Stats (all columns)
                row_to_json(s.*) as stats_json
            FROM {player_table} p
            LEFT JOIN {team_table} t ON t.id = p.current_team_id
            LEFT JOIN leagues l ON l.id = p.current_league_id
            LEFT JOIN {stats_table} s ON s.player_id = p.id AND s.season_id = %s
            WHERE p.id = %s
        """
        params = (season_id, player_id)
    else:
        # NBA/NFL: include college/experience
        query = f"""
            SELECT
                -- Player info
                p.id, '{sport}' as sport_id, p.first_name, p.last_name, p.full_name,
                p.position, p.position_group, p.nationality,
                p.birth_date::text as birth_date, p.birth_place, p.birth_country,
                p.height_inches, p.weight_lbs, p.photo_url,
                p.current_team_id, p.jersey_number,
                p.college, p.experience_years, p.is_active,
                -- Team info
                t.id as team_id, t.name as team_name, t.abbreviation as team_abbr,
                t.logo_url as team_logo, t.conference, t.division, t.city as team_city,
                -- Stats (all columns)
                row_to_json(s.*) as stats_json
            FROM {player_table} p
            LEFT JOIN {team_table} t ON t.id = p.current_team_id
            LEFT JOIN {stats_table} s ON s.player_id = p.id AND s.season_id = %s
            WHERE p.id = %s
        """
        params = (season_id, player_id)

    row = db.fetchone(query, params)

    if not row:
        return None

    data = dict(row)

    # Build nested team object
    team = None
    if data.get("team_id"):
        team = {
            "id": data.pop("team_id"),
            "name": data.pop("team_name"),
            "abbreviation": data.pop("team_abbr"),
            "logo_url": data.pop("team_logo"),
        }
        # Add conference/division for American sports
        if "conference" in data:
            team["conference"] = data.pop("conference")
            team["division"] = data.pop("division")
        # Add country for Football
        if "team_country" in data:
            team["country"] = data.pop("team_country")
        if "team_city" in data:
            team["city"] = data.pop("team_city")
    else:
        for k in ["team_id", "team_name", "team_abbr", "team_logo", "conference", "division", "team_city", "team_country"]:
            data.pop(k, None)

    # Build nested league object (Football only)
    league = None
    if has_leagues and data.get("league_id"):
        league = {
            "id": data.pop("league_id"),
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_id", "league_name", "league_country", "league_logo", "current_league_id"]:
            data.pop(k, None)

    # Parse stats JSON
    stats_json = data.pop("stats_json", None)
    stats = None
    if stats_json:
        import json
        stats = json.loads(stats_json) if isinstance(stats_json, str) else stats_json
        # Clean up stats
        for key in ["id", "player_id", "team_id", "season_id", "league_id"]:
            stats.pop(key, None)
        if stats.get("updated_at"):
            stats["updated_at"] = str(stats["updated_at"])

    # Get percentiles if requested
    percentiles = []
    if include_percentiles:
        percentile_rows = db.fetchall(
            """
            SELECT stat_category, stat_value, percentile, rank, sample_size, comparison_group
            FROM percentile_cache
            WHERE entity_type = 'player' AND entity_id = %s
              AND sport_id = %s AND season_id = %s
            ORDER BY stat_category
            """,
            (player_id, sport, season_id),
        )
        percentiles = [dict(r) for r in percentile_rows]

    # Build info object (excluding stats)
    info = {k: v for k, v in data.items()}
    info["team"] = team
    info["league"] = league

    return {
        "entity_id": player_id,
        "entity_type": "player",
        "sport": sport,
        "season": season,
        "info": info,
        "stats": stats,
        "percentiles": percentiles if include_percentiles else None,
    }


def _get_team_profile(
    db, team_id: int, sport: str, season: int, league_id: int | None, include_percentiles: bool
) -> dict[str, Any] | None:
    """
    Fetch complete team profile in a single optimized query.
    Now uses sport-specific profile tables.
    """
    team_table = TEAM_PROFILE_TABLES.get(sport)
    stats_table = TEAM_STATS_TABLES.get(sport)
    if not team_table or not stats_table:
        return None

    # Get sport config
    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    # Get season ID
    season_row = db.fetchone(
        "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
        (sport, season),
    )
    if not season_row:
        return None
    season_id = season_row["id"]

    # Build stats join condition
    stats_condition = "s.team_id = t.id AND s.season_id = %s"
    stats_params = [season_id]
    if has_leagues and league_id:
        stats_condition += " AND s.league_id = %s"
        stats_params.append(league_id)

    # Build query based on sport - no sport_id filtering needed (table IS the filter)
    if has_leagues:
        # Football: include league info and is_national flag
        query = f"""
            SELECT
                -- Team info
                t.id, '{sport}' as sport_id, t.league_id, t.name, t.abbreviation, t.logo_url,
                t.country, t.city, t.founded, t.is_national,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city,
                t.venue_surface, t.venue_image, t.is_active,
                -- League info
                l.name as league_name, l.country as league_country, l.logo_url as league_logo,
                -- Stats
                row_to_json(s.*) as stats_json
            FROM {team_table} t
            LEFT JOIN leagues l ON l.id = t.league_id
            LEFT JOIN {stats_table} s ON {stats_condition}
            WHERE t.id = %s
        """
    else:
        # NBA/NFL: include conference/division
        query = f"""
            SELECT
                -- Team info
                t.id, '{sport}' as sport_id, t.name, t.abbreviation, t.logo_url,
                t.conference, t.division, t.country, t.city, t.founded,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city,
                t.venue_surface, t.venue_image, t.is_active,
                -- Stats
                row_to_json(s.*) as stats_json
            FROM {team_table} t
            LEFT JOIN {stats_table} s ON {stats_condition}
            WHERE t.id = %s
        """

    row = db.fetchone(query, (*stats_params, team_id))

    if not row:
        return None

    data = dict(row)

    # Build nested league object (Football only)
    league = None
    if has_leagues and data.get("league_id"):
        league = {
            "id": data["league_id"],
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_name", "league_country", "league_logo"]:
            data.pop(k, None)

    # Parse stats JSON
    stats_json = data.pop("stats_json", None)
    stats = None
    if stats_json:
        import json
        stats = json.loads(stats_json) if isinstance(stats_json, str) else stats_json
        for key in ["id", "team_id", "season_id", "league_id"]:
            stats.pop(key, None)
        if stats.get("updated_at"):
            stats["updated_at"] = str(stats["updated_at"])

    # Get percentiles if requested
    percentiles = []
    if include_percentiles:
        percentile_rows = db.fetchall(
            """
            SELECT stat_category, stat_value, percentile, rank, sample_size, comparison_group
            FROM percentile_cache
            WHERE entity_type = 'team' AND entity_id = %s
              AND sport_id = %s AND season_id = %s
            ORDER BY stat_category
            """,
            (team_id, sport, season_id),
        )
        percentiles = [dict(r) for r in percentile_rows]

    # Build info object
    info = {k: v for k, v in data.items()}
    info["league"] = league

    result = {
        "entity_id": team_id,
        "entity_type": "team",
        "sport": sport,
        "season": season,
        "info": info,
        "stats": stats,
        "percentiles": percentiles if include_percentiles else None,
    }
    if league_id:
        result["league_id"] = league_id

    return result
