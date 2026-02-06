"""
Transfer service — SQL queries for transfer prediction endpoints.

Encapsulates all raw SQL for transfer-related queries.
Routers call these functions instead of building SQL inline.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from ..core.types import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


def find_player(db: "PostgresDB", player_id: int) -> dict[str, Any] | None:
    """Look up a player across all sport-specific profile tables.

    Iterates over every sport in PLAYER_PROFILE_TABLES and returns the first
    match.  The result includes the player's team via a LEFT JOIN on the
    corresponding sport-specific team profile table.

    Returns:
        A dict with keys (id, name, sport, position, team_id, team_name),
        or None if the player is not found in any sport table.
    """
    for sport_id, profile_table in PLAYER_PROFILE_TABLES.items():
        team_table = TEAM_PROFILE_TABLES[sport_id]
        row = db.fetchone(
            f"""
            SELECT p.id,
                   COALESCE(p.full_name, p.first_name || ' ' || p.last_name) as name,
                   '{sport_id}' as sport,
                   p.position,
                   t.id as team_id,
                   t.name as team_name
            FROM {profile_table} p
            LEFT JOIN {team_table} t ON t.id = p.team_id
            WHERE p.id = %s
            """,
            (player_id,),
        )
        if row:
            return row
    return None


def find_team(db: "PostgresDB", team_id: int) -> dict[str, Any] | None:
    """Look up a team across all sport-specific profile tables.

    Iterates over every sport in TEAM_PROFILE_TABLES and returns the first
    match.

    Returns:
        A dict with keys (id, name, sport), or None if not found.
    """
    for sport_id, team_table in TEAM_PROFILE_TABLES.items():
        row = db.fetchone(
            f"""
            SELECT t.id, t.name, '{sport_id}' as sport
            FROM {team_table} t
            WHERE t.id = %s
            """,
            (team_id,),
        )
        if row:
            return row
    return None


def get_team_transfer_links(db: "PostgresDB", team_id: int) -> list[dict[str, Any]]:
    """Get active transfer links for a team, ordered by probability.

    Returns:
        List of dicts with columns: id, player_id, player_name,
        player_current_team, current_probability, previous_probability,
        trend_direction, trend_change_7d, total_mentions, tier_1_mentions.
    """
    return db.fetchall(
        """
        SELECT
            tl.id, tl.player_id, tl.player_name, tl.player_current_team,
            tl.current_probability, tl.previous_probability,
            tl.trend_direction, tl.trend_change_7d,
            tl.total_mentions, tl.tier_1_mentions
        FROM transfer_links tl
        WHERE tl.team_id = %s AND tl.is_active = TRUE
        ORDER BY tl.current_probability DESC NULLS LAST
        LIMIT 20
        """,
        (team_id,),
    )


def get_transfer_headlines(
    db: "PostgresDB", link_id: int, limit: int = 3
) -> list[dict[str, Any]]:
    """Get recent headlines for a transfer link.

    Returns:
        List of dicts each containing a 'headline' key.
    """
    return db.fetchall(
        """
        SELECT headline FROM transfer_mentions
        WHERE transfer_link_id = %s
        ORDER BY mentioned_at DESC
        LIMIT %s
        """,
        (link_id, limit),
    )


def get_player_transfer_links(
    db: "PostgresDB", player_id: int
) -> list[dict[str, Any]]:
    """Get active transfer links for a player, ordered by probability.

    Returns:
        List of dicts with columns: team_id, team_name, current_probability,
        trend_direction, trend_change_7d, total_mentions.
    """
    return db.fetchall(
        """
        SELECT
            tl.team_id, tl.team_name, tl.current_probability,
            tl.trend_direction, tl.trend_change_7d, tl.total_mentions
        FROM transfer_links tl
        WHERE tl.player_id = %s AND tl.is_active = TRUE
        ORDER BY tl.current_probability DESC NULLS LAST
        """,
        (player_id,),
    )


def get_trending_transfer_links(
    db: "PostgresDB", sport: str, limit: int = 10
) -> list[dict[str, Any]]:
    """Get trending transfer rumours for a sport.

    Ranks links by a weighted score of tier-1/tier-2 mentions and overall
    probability.  Includes correlated sub-queries for 24-hour mention count
    and top source.

    Returns:
        List of dicts with columns: player_name, player_current_team,
        team_name, current_probability, trend_direction, mentions_24h,
        top_source.
    """
    return db.fetchall(
        """
        SELECT
            tl.player_name, tl.player_current_team, tl.team_name,
            tl.current_probability, tl.trend_direction,
            (SELECT COUNT(*) FROM transfer_mentions tm
             WHERE tm.transfer_link_id = tl.id
             AND tm.mentioned_at >= NOW() - INTERVAL '24 hours') as mentions_24h,
            (SELECT source_name FROM transfer_mentions tm
             WHERE tm.transfer_link_id = tl.id
             ORDER BY tm.source_tier ASC, tm.mentioned_at DESC
             LIMIT 1) as top_source
        FROM transfer_links tl
        WHERE LOWER(tl.sport) = LOWER(%s) AND tl.is_active = TRUE
        ORDER BY
            (tl.tier_1_mentions * 3 + tl.tier_2_mentions * 2 + tl.total_mentions) DESC,
            tl.current_probability DESC NULLS LAST
        LIMIT %s
        """,
        (sport, limit),
    )
