"""
Profile service — sport-aware entity profile lookups.

Encapsulates all raw SQL for profile queries, using psycopg.sql.Identifier
for safe dynamic table injection. Routers call this instead of building SQL.
"""

import logging
from typing import Any

from psycopg import sql

from ..core.types import (
    PLAYER_PROFILE_TABLES,
    TEAM_PROFILE_TABLES,
    get_sport_config,
)

logger = logging.getLogger(__name__)


def get_player_profile(db, player_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch player profile with team and league data from sport-specific tables.

    Args:
        db: Database connection (psycopg-style with fetchone).
        player_id: The player's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested team/league objects, or None if not found.
    """
    player_table = PLAYER_PROFILE_TABLES.get(sport)
    team_table = TEAM_PROFILE_TABLES.get(sport)

    if not player_table or not team_table:
        return None

    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    p_tbl = sql.Identifier(player_table)
    t_tbl = sql.Identifier(team_table)

    if has_leagues:
        # Football: include league info
        query = sql.SQL("""
            SELECT
                p.id, {sport_literal} as sport_id, p.first_name, p.last_name, p.full_name,
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
        """).format(
            sport_literal=sql.Literal(sport),
            player_table=p_tbl,
            team_table=t_tbl,
        )
    else:
        # NBA/NFL: no league, include college/experience for American sports
        query = sql.SQL("""
            SELECT
                p.id, {sport_literal} as sport_id, p.first_name, p.last_name, p.full_name,
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
        """).format(
            sport_literal=sql.Literal(sport),
            player_table=p_tbl,
            team_table=t_tbl,
        )

    row = db.fetchone(query, (player_id,))
    if not row:
        return None

    data = dict(row)

    # Football: use first_name + last_name as full_name since API returns abbreviated names
    if has_leagues:
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
        if "conference" in data:
            team["conference"] = data.pop("conference")
            team["division"] = data.pop("division")
        if "team_country" in data:
            team["country"] = data.pop("team_country")
        if "team_city" in data:
            team["city"] = data.pop("team_city")
    else:
        for k in ["team_id", "team_name", "team_abbr", "team_logo",
                   "conference", "division", "team_city", "team_country"]:
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
        for k in ["league_id", "league_name", "league_country",
                   "league_logo", "current_league_id"]:
            data.pop(k, None)

    data["team"] = team
    data["league"] = league

    return data


def get_team_profile(db, team_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch team profile with league data from sport-specific tables.

    Args:
        db: Database connection (psycopg-style with fetchone).
        team_id: The team's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested league object, or None if not found.
    """
    team_table = TEAM_PROFILE_TABLES.get(sport)
    if not team_table:
        return None

    sport_config = get_sport_config(sport)
    has_leagues = sport_config.has_league_in_profiles

    t_tbl = sql.Identifier(team_table)

    if has_leagues:
        # Football: include league info and is_national flag
        query = sql.SQL("""
            SELECT
                t.id, {sport_literal} as sport_id, t.league_id, t.name, t.abbreviation, t.logo_url,
                t.country, t.city, t.founded, t.is_national,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city, t.venue_surface, t.venue_image,
                t.is_active,
                l.name as league_name, l.country as league_country, l.logo_url as league_logo
            FROM {team_table} t
            LEFT JOIN leagues l ON l.id = t.league_id
            WHERE t.id = %s
        """).format(
            sport_literal=sql.Literal(sport),
            team_table=t_tbl,
        )
    else:
        # NBA/NFL: include conference/division
        query = sql.SQL("""
            SELECT
                t.id, {sport_literal} as sport_id, t.name, t.abbreviation, t.logo_url,
                t.conference, t.division, t.country, t.city, t.founded,
                t.venue_name, t.venue_address, t.venue_capacity, t.venue_city, t.venue_surface, t.venue_image,
                t.is_active
            FROM {team_table} t
            WHERE t.id = %s
        """).format(
            sport_literal=sql.Literal(sport),
            team_table=t_tbl,
        )

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
