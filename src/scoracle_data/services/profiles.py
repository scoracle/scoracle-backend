"""
Profile service — sport-aware entity profile lookups.

Queries Postgres views (v_player_profile, v_team_profile) which handle
all JOINs and nested object construction via json_build_object().
Python is a thin pass-through — no dict manipulation needed.
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)


def get_player_profile(db, player_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch player profile with nested team/league objects.

    The v_player_profile view handles all JOINs and builds nested JSON
    objects for team and league data. Nulls pass through to the frontend.

    Args:
        db: Database connection (psycopg-style with fetchone).
        player_id: The player's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested team/league objects, or None if not found.
    """
    row = db.fetchone(
        "SELECT * FROM v_player_profile WHERE id = %s AND sport_id = %s",
        (player_id, sport),
    )
    return dict(row) if row else None


def get_team_profile(db, team_id: int, sport: str) -> dict[str, Any] | None:
    """Fetch team profile with nested league object.

    The v_team_profile view handles the league JOIN and builds a nested
    JSON object for league data. Nulls pass through to the frontend.

    Args:
        db: Database connection (psycopg-style with fetchone).
        team_id: The team's ID.
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Profile dict with nested league object, or None if not found.
    """
    row = db.fetchone(
        "SELECT * FROM v_team_profile WHERE id = %s AND sport_id = %s",
        (team_id, sport),
    )
    return dict(row) if row else None
