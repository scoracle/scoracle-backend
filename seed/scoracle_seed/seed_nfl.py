"""NFL seed orchestration.

Three-phase flow: teams -> player stats (with embedded profiles) -> team stats.
"""

from __future__ import annotations

import logging

import psycopg

from .bdl_nfl import NFLHandler
from .models import SeedResult
from .upsert import upsert_player, upsert_player_stats, upsert_team, upsert_team_stats

logger = logging.getLogger(__name__)

SPORT = "NFL"


def seed_nfl(conn: psycopg.Connection, handler: NFLHandler, season: int) -> SeedResult:
    """Run the full NFL seed: teams -> player stats -> team stats."""
    result = SeedResult()

    # Phase 1: Teams
    logger.info("Seeding NFL teams...")
    try:
        teams = handler.get_teams()
    except Exception as exc:
        result.add_error(f"fetch NFL teams: {exc}")
        return result

    for team in teams:
        try:
            upsert_team(conn, SPORT, team)
            result.teams_upserted += 1
        except Exception as exc:
            result.add_error(f"upsert team {team.id}: {exc}")
    logger.info("NFL teams done: %d", result.teams_upserted)

    # Phase 2: Player stats
    logger.info("Seeding NFL player stats (season=%d)...", season)
    count = 0

    def on_player_stats(ps):
        nonlocal count
        if ps.player:
            try:
                upsert_player(conn, SPORT, ps.player)
                result.players_upserted += 1
            except Exception as exc:
                result.add_error(f"upsert player {ps.player_id}: {exc}")
        try:
            upsert_player_stats(conn, SPORT, season, 0, ps)
            result.player_stats_upserted += 1
        except Exception as exc:
            result.add_error(f"upsert player stats {ps.player_id}: {exc}")
        count += 1
        if count % 50 == 0:
            logger.info("NFL player stats progress: %d", count)

    try:
        handler.get_player_stats(season, postseason=False, callback=on_player_stats)
    except Exception as exc:
        result.add_error(f"fetch NFL player stats: {exc}")
    logger.info("NFL player stats done: %d", result.player_stats_upserted)

    # Phase 3: Team stats (standings)
    logger.info("Seeding NFL team stats (season=%d)...", season)
    try:
        team_stats = handler.get_team_stats(season, "regular")
    except Exception as exc:
        result.add_error(f"fetch NFL team stats: {exc}")
        return result

    for ts in team_stats:
        try:
            upsert_team_stats(conn, SPORT, season, 0, ts)
            result.team_stats_upserted += 1
        except Exception as exc:
            result.add_error(f"upsert team stats {ts.team_id}: {exc}")
    logger.info("NFL team stats done: %d", result.team_stats_upserted)

    logger.info("NFL seed complete: %s", result.summary())
    return result
