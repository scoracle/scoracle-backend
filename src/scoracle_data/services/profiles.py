"""
Profile service — sport-aware entity profile lookups.

Uses the unified players/teams tables with sport discriminator.
Sport-specific data lives in the meta JSONB column.
"""

import logging
from typing import Any

from ..core.types import PLAYERS_TABLE, TEAMS_TABLE

logger = logging.getLogger(__name__)


def get_player_profile(db, player_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch player profile with team and league data from unified tables.

    Single query using LEFT JOINs — no N+1 for league lookups.

    Args:
        db: Database connection (psycopg-style with fetchone).
        player_id: The player's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested team/league objects, or None if not found.
    """
    query = f"""
        SELECT
            p.id, p.sport as sport_id, p.name, p.first_name, p.last_name,
            p.position, p.detailed_position, p.nationality,
            p.date_of_birth::text as date_of_birth,
            p.height_cm, p.weight_kg, p.photo_url,
            p.team_id, p.league_id, p.meta,
            t.id as team_id_check, t.name as team_name, t.short_code as team_abbr,
            t.logo_url as team_logo, t.country as team_country, t.city as team_city,
            t.conference as team_conference, t.division as team_division,
            l.id as league_id_check, l.name as league_name,
            l.country as league_country, l.logo_url as league_logo
        FROM {PLAYERS_TABLE} p
        LEFT JOIN {TEAMS_TABLE} t ON t.id = p.team_id AND t.sport = p.sport
        LEFT JOIN leagues l ON l.id = p.league_id
        WHERE p.id = %s AND p.sport = %s
    """

    row = db.fetchone(query, (player_id, sport))
    if not row:
        return None

    data = row  # fetchone already returns a dict

    # Build nested team object
    team = None
    if data.get("team_id_check"):
        team = {
            "id": data.pop("team_id_check"),
            "name": data.pop("team_name"),
            "abbreviation": data.pop("team_abbr"),
            "logo_url": data.pop("team_logo"),
        }
        if data.get("team_country"):
            team["country"] = data.pop("team_country")
        if data.get("team_city"):
            team["city"] = data.pop("team_city")
        # Use typed conference/division columns (set by migration 002)
        if data.get("team_conference"):
            team["conference"] = data.pop("team_conference")
        else:
            data.pop("team_conference", None)
        if data.get("team_division"):
            team["division"] = data.pop("team_division")
        else:
            data.pop("team_division", None)
    else:
        for k in [
            "team_id_check",
            "team_name",
            "team_abbr",
            "team_logo",
            "team_country",
            "team_city",
            "team_conference",
            "team_division",
        ]:
            data.pop(k, None)

    # Build nested league object from the JOIN (no separate query)
    league = None
    if data.get("league_id_check"):
        league = {
            "id": data.pop("league_id_check"),
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_id_check", "league_name", "league_country", "league_logo"]:
            data.pop(k, None)

    data["team"] = team
    data["league"] = league

    return data


def get_team_profile(db, team_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch team profile with league data from unified tables.

    Single query using LEFT JOIN — no N+1 for league lookups.

    Args:
        db: Database connection (psycopg-style with fetchone).
        team_id: The team's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested league object, or None if not found.
    """
    query = f"""
        SELECT
            t.id, t.sport as sport_id, t.name, t.short_code, t.logo_url,
            t.country, t.city, t.founded, t.league_id,
            t.conference, t.division,
            t.venue_name, t.venue_capacity, t.meta,
            l.id as league_id_check, l.name as league_name,
            l.country as league_country, l.logo_url as league_logo
        FROM {TEAMS_TABLE} t
        LEFT JOIN leagues l ON l.id = t.league_id
        WHERE t.id = %s AND t.sport = %s
    """

    row = db.fetchone(query, (team_id, sport))
    if not row:
        return None

    data = row  # fetchone already returns a dict

    # Build nested league object from the JOIN (no separate query)
    league = None
    if data.get("league_id_check"):
        league = {
            "id": data.pop("league_id_check"),
            "name": data.pop("league_name"),
            "country": data.pop("league_country"),
            "logo_url": data.pop("league_logo"),
        }
    else:
        for k in ["league_id_check", "league_name", "league_country", "league_logo"]:
            data.pop(k, None)

    data["league"] = league

    return data
