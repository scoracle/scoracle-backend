"""
Bootstrap service — builds autofill entity databases from live DB data.

Queries the players and teams tables with sport-specific JOINs to produce
the v2.0 entity format used by the frontend for fuzzy-search autocomplete.

All functions are async — they accept an AsyncPostgresDB and await queries.
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import Any

from ..core.autofill import build_player_entity, build_team_entity

logger = logging.getLogger(__name__)

# =============================================================================
# SQL Queries
# =============================================================================

# NBA/NFL players: simple join to teams for abbreviation
_SQL_PLAYERS_STANDARD = """
    SELECT p.id, p.name, p.position, p.meta,
           t.short_code AS team_abbr
    FROM players p
    LEFT JOIN teams t
        ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = %s
"""

# Football players: resolve league_id from player_stats (players.league_id may be NULL)
_SQL_PLAYERS_FOOTBALL = """
    SELECT DISTINCT ON (p.id)
        p.id, p.name, p.position, p.detailed_position, p.meta,
        t.name AS team_name, t.short_code AS team_abbr,
        ps.league_id,
        l.name AS league_name
    FROM players p
    LEFT JOIN teams t
        ON t.id = p.team_id AND t.sport = p.sport
    LEFT JOIN player_stats ps
        ON ps.player_id = p.id AND ps.sport = p.sport AND ps.season = %s
    LEFT JOIN leagues l
        ON l.id = ps.league_id
    WHERE p.sport = %s
    ORDER BY p.id, ps.league_id
"""

# NBA/NFL teams: conference/division metadata
_SQL_TEAMS_STANDARD = """
    SELECT t.id, t.name, t.short_code, t.conference, t.division, t.country
    FROM teams t
    WHERE t.sport = %s
"""

# Football teams: resolve league_id from team_stats
_SQL_TEAMS_FOOTBALL = """
    SELECT DISTINCT ON (t.id)
        t.id, t.name, t.short_code, t.country,
        ts.league_id,
        l.name AS league_name
    FROM teams t
    LEFT JOIN team_stats ts
        ON ts.team_id = t.id AND ts.sport = t.sport AND ts.season = %s
    LEFT JOIN leagues l
        ON l.id = ts.league_id
    WHERE t.sport = %s
    ORDER BY t.id, ts.league_id
"""


# =============================================================================
# Public API
# =============================================================================


async def get_autofill_database(
    db: Any,
    sport: str,
    season: int,
) -> dict[str, Any]:
    """Build the autofill entity database for a single sport.

    Queries the live DB and returns a v2.0 bootstrap dict with all players
    and teams for the given sport, ready for frontend consumption.

    Args:
        db: AsyncPostgresDB instance.
        sport: Sport identifier (NBA, NFL, FOOTBALL).
        season: Season year for resolving league associations (Football).

    Returns:
        Dict with keys: version, generated_at, sport, count, entities.
    """
    entities: list[dict[str, Any]] = []

    # -- Players --
    if sport == "FOOTBALL":
        player_rows = await db.fetchall(_SQL_PLAYERS_FOOTBALL, (season, sport))
    else:
        player_rows = await db.fetchall(_SQL_PLAYERS_STANDARD, (sport,))

    for row in player_rows:
        entity = build_player_entity(dict(row), sport)
        if entity:
            entities.append(entity)

    # -- Teams --
    if sport == "FOOTBALL":
        team_rows = await db.fetchall(_SQL_TEAMS_FOOTBALL, (season, sport))
    else:
        team_rows = await db.fetchall(_SQL_TEAMS_STANDARD, (sport,))

    for row in team_rows:
        entity = build_team_entity(dict(row), sport)
        if entity:
            entities.append(entity)

    logger.info(
        "Built %s autofill database: %d entities (%d players, %d teams)",
        sport,
        len(entities),
        sum(1 for e in entities if e["type"] == "player"),
        sum(1 for e in entities if e["type"] == "team"),
    )

    return {
        "version": "2.0",
        "generated_at": datetime.now(tz=timezone.utc).isoformat(),
        "sport": sport,
        "count": len(entities),
        "entities": entities,
    }
