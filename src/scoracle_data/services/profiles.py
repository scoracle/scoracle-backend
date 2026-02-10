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
            t.logo_url as team_logo, t.country as team_country, t.city as team_city, t.meta as team_meta
        FROM {PLAYERS_TABLE} p
        LEFT JOIN {TEAMS_TABLE} t ON t.id = p.team_id AND t.sport = p.sport
        WHERE p.id = %s AND p.sport = %s
    """

    row = db.fetchone(query, (player_id, sport))
    if not row:
        return None

    data = dict(row)

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
        # Pull conference/division from team meta for NBA/NFL
        team_meta = data.pop("team_meta", None) or {}
        if isinstance(team_meta, dict):
            if "conference" in team_meta:
                team["conference"] = team_meta["conference"]
            if "division" in team_meta:
                team["division"] = team_meta["division"]
    else:
        for k in [
            "team_id_check",
            "team_name",
            "team_abbr",
            "team_logo",
            "team_country",
            "team_city",
            "team_meta",
        ]:
            data.pop(k, None)

    # Build nested league object (Football only)
    league = None
    if sport == "FOOTBALL" and data.get("league_id"):
        league_row = db.fetchone(
            "SELECT id, name, country, logo_url FROM leagues WHERE id = %s",
            (data["league_id"],),
        )
        if league_row:
            league = dict(league_row)

    data["team"] = team
    data["league"] = league

    return data


def get_team_profile(db, team_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch team profile with league data from unified tables.

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
            t.venue_name, t.venue_capacity, t.meta
        FROM {TEAMS_TABLE} t
        WHERE t.id = %s AND t.sport = %s
    """

    row = db.fetchone(query, (team_id, sport))
    if not row:
        return None

    data = dict(row)

    # Build nested league object (Football only)
    league = None
    if sport == "FOOTBALL" and data.get("league_id"):
        league_row = db.fetchone(
            "SELECT id, name, country, logo_url FROM leagues WHERE id = %s",
            (data["league_id"],),
        )
        if league_row:
            league = dict(league_row)

    data["league"] = league

    return data
