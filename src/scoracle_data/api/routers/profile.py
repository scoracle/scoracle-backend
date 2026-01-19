"""
Profile router - serves entity profiles for frontend widgets.

Endpoints:
- GET /{entity_type}/{entity_id} - Entity profile from players/teams tables

Data:
- Serves entity info from profile tables (nba_players, nba_teams, etc.)
- Includes related team/league info via JOINs

Performance Features:
- In-memory caching with TTLs
- ETag support for conditional requests (304 Not Modified)
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_ENTITY_INFO
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ..types import (
    EntityType,
    Sport,
    PLAYER_PROFILE_TABLES,
    TEAM_PROFILE_TABLES,
    get_sport_config,
)
from ._utils import (
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
)

logger = logging.getLogger(__name__)

router = APIRouter()


@router.get("/{entity_type}/{entity_id}", response_model=None)
async def get_entity_profile(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> dict[str, Any] | Response:
    """
    Get entity profile for widget rendering.

    Returns all fields from the players or teams profile table, with related
    team/league info included via JOINs.

    This endpoint serves entity info ONLY (name, photo, team, position, etc.).
    For statistics, use the /stats endpoint.

    Supports conditional requests via If-None-Match header for ETag validation.
    Returns 304 Not Modified if content hasn't changed.
    """
    cache = get_cache()
    cache_key = ("profile", entity_type.value, entity_id, sport.value)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=True)
        set_etag_headers(response, etag, TTL_ENTITY_INFO)
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
    etag = compute_etag(result)
    set_cache_headers(response, TTL_ENTITY_INFO, cache_hit=False)
    set_etag_headers(response, etag, TTL_ENTITY_INFO)
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

    # For FOOTBALL: use first_name + last_name as full_name since API returns abbreviated names
    # e.g., "C. Palmer" instead of "Cole Palmer"
    if has_leagues:  # FOOTBALL
        first = data.get("first_name") or ""
        last = data.get("last_name") or ""
        combined = f"{first} {last}".strip()
        if combined:
            data["full_name"] = combined

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
