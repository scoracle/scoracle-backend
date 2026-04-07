"""Football seed orchestration.

Three-phase flow: teams -> players+stats (squad iteration) -> team stats (standings).
Requires resolving SportMonks season/league IDs from the database.
"""

from __future__ import annotations

import logging

import psycopg

from shared.models import SeedResult
from shared.upsert import (
    upsert_player,
    upsert_player_stats,
    upsert_team,
    upsert_team_stats,
)
from .handlers.sportmonks_football import FootballHandler

logger = logging.getLogger(__name__)

SPORT = "FOOTBALL"


def seed_football_season(
    conn: psycopg.Connection,
    handler: FootballHandler,
    sm_season_id: int,
    league_id: int,
    season_year: int,
    sm_league_id: int,
) -> SeedResult:
    """Seed all data for a single Football league-season."""
    result = SeedResult()

    logger.info(
        "Seeding football season sm_season_id=%d league_id=%d season=%d",
        sm_season_id,
        league_id,
        season_year,
    )

    # Phase 1: Teams
    logger.info("Phase 1/3: Seeding teams...")
    try:
        teams = handler.get_teams(sm_season_id)
    except Exception as exc:
        result.add_error(f"fetch teams: {exc}")
        teams = []

    for team in teams:
        try:
            upsert_team(conn, SPORT, team)
            result.teams_upserted += 1
        except Exception as exc:
            result.add_error(f"upsert team {team.id}: {exc}")
    logger.info("Teams done: %d", result.teams_upserted)

    # Phase 2: Players + Player Stats (via squad iteration)
    logger.info("Phase 2/3: Seeding players + stats...")
    team_ids = [t.id for t in teams]
    count = 0

    def on_player_stats(ps):
        nonlocal count
        if ps.player:
            try:
                upsert_player(conn, SPORT, ps.player)
                result.players_upserted += 1
            except Exception as exc:
                result.add_error(f"upsert player {ps.player_id}: {exc}")
        if ps.stats:
            try:
                upsert_player_stats(conn, SPORT, season_year, league_id, ps)
                result.player_stats_upserted += 1
            except Exception as exc:
                result.add_error(f"upsert player stats {ps.player_id}: {exc}")
        count += 1
        if count % 50 == 0:
            logger.info("Player progress: %d", count)

    try:
        handler.get_players_with_stats(
            sm_season_id, team_ids, sm_league_id, callback=on_player_stats
        )
    except Exception as exc:
        result.add_error(f"fetch players/stats: {exc}")
    logger.info(
        "Players + stats done: players=%d stats=%d",
        result.players_upserted,
        result.player_stats_upserted,
    )

    # Phase 3: Team Stats (Standings)
    logger.info("Phase 3/3: Seeding standings...")
    try:
        team_stats = handler.get_team_stats(sm_season_id)
    except Exception as exc:
        result.add_error(f"fetch standings: {exc}")
        team_stats = []

    for ts in team_stats:
        if ts.team:
            try:
                upsert_team(conn, SPORT, ts.team)
            except Exception:
                pass  # Non-fatal: team data from standings is supplementary
        try:
            upsert_team_stats(conn, SPORT, season_year, league_id, ts)
            result.team_stats_upserted += 1
        except Exception as exc:
            result.add_error(f"upsert team stats {ts.team_id}: {exc}")
    logger.info("Standings done: %d", result.team_stats_upserted)

    logger.info(
        "Football season seed complete league_id=%d season=%d: %s",
        league_id,
        season_year,
        result.summary(),
    )
    return result


def resolve_provider_season_id(
    conn: psycopg.Connection, league_id: int, season_year: int
) -> int | None:
    """Look up SportMonks season ID from the provider_seasons table."""
    row = conn.execute(
        "SELECT resolve_provider_season_id(%s, %s)", (league_id, season_year)
    ).fetchone()
    if row:
        val = list(row.values())[0]
        return val
    return None


def resolve_sm_league_id(
    conn: psycopg.Connection, league_id: int
) -> tuple[int | None, str]:
    """Look up SportMonks league ID and name from leagues table."""
    row = conn.execute(
        "SELECT sportmonks_id, name FROM leagues WHERE id = %s", (league_id,)
    ).fetchone()
    if row:
        return row.get("sportmonks_id"), row.get("name", "")
    return None, ""
