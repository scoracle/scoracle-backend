"""Metadata seeding CLI commands.

Commands:
  seed             — Seed team and player profiles from provider profile endpoints
"""

from __future__ import annotations

import logging
import sys
from typing import Any

import click
import psycopg

from shared import config as config_mod
from shared.db import check_connectivity, create_pool, get_conn
from shared.upsert import upsert_player, upsert_provider_entity_map, upsert_team
from ..event.handlers.bdl_nba import NBAHandler, _parse_player as parse_nba_player
from ..event.handlers.bdl_nfl import NFLHandler, _parse_player as parse_nfl_player
from ..event.handlers.sportmonks_football import (
    FootballHandler,
    _parse_player as parse_football_player,
)
from ..event.seed_football import resolve_provider_season_id

logger = logging.getLogger("meta_seeding")


@click.group(name="meta")
def cli() -> None:
    """Metadata seeding — team/player profiles."""


def _extract_player_ids(rows: list[dict[str, Any]]) -> list[int]:
    seen: set[int] = set()
    player_ids: list[int] = []
    for row in rows:
        player_id = row.get("id")
        if isinstance(player_id, int) and player_id not in seen:
            seen.add(player_id)
            player_ids.append(player_id)
    player_ids.sort()
    return player_ids


def _seed_nba_metadata(
    conn: psycopg.Connection,
    api_key: str,
    max_teams: int | None,
    max_players: int | None,
) -> tuple[int, int, int]:
    teams_seeded = 0
    players_seeded = 0
    failed = 0

    handler = NBAHandler(api_key)
    try:
        teams = handler.get_teams()
        if max_teams is not None:
            teams = teams[:max_teams]
        for team in teams:
            upsert_team(conn, "NBA", team)
            upsert_provider_entity_map(conn, "bdl", "NBA", "team", str(team.id), team.id)
            teams_seeded += 1

        player_rows = handler.get_all_players(limit=max_players)
        player_by_id = {
            row["id"]: row for row in player_rows if isinstance(row.get("id"), int)
        }
        player_ids = _extract_player_ids(player_rows)
        if max_players is not None:
            player_ids = player_ids[:max_players]

        click.echo(f"Seeding {len(player_ids)} NBA player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            profile = handler.get_player(player_id)
            if not isinstance(profile, dict):
                profile = player_by_id.get(player_id)
            if not isinstance(profile, dict):
                failed += 1
                logger.warning("NBA profile missing for player_id=%d", player_id)
                continue

            player = parse_nba_player(profile)
            if player.id == 0:
                player.id = player_id
            upsert_player(conn, "NBA", player)
            upsert_provider_entity_map(
                conn, "bdl", "NBA", "player", str(player_id), player.id
            )
            players_seeded += 1

            if idx % 100 == 0:
                click.echo(f"NBA profile progress: {idx}/{len(player_ids)}")
    finally:
        handler.close()

    return teams_seeded, players_seeded, failed


def _seed_nfl_metadata(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    max_teams: int | None,
    max_players: int | None,
) -> tuple[int, int, int]:
    teams_seeded = 0
    players_seeded = 0
    failed = 0

    handler = NFLHandler(api_key)
    try:
        teams = handler.get_teams()
        if max_teams is not None:
            teams = teams[:max_teams]
        for team in teams:
            upsert_team(conn, "NFL", team)
            upsert_provider_entity_map(conn, "bdl", "NFL", "team", str(team.id), team.id)
            teams_seeded += 1

        player_rows = handler.get_all_players(season, limit=max_players)
        player_by_id = {
            row["id"]: row for row in player_rows if isinstance(row.get("id"), int)
        }
        player_ids = _extract_player_ids(player_rows)
        if max_players is not None:
            player_ids = player_ids[:max_players]

        click.echo(f"Seeding {len(player_ids)} NFL player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            profile = handler.get_player(player_id)
            if not isinstance(profile, dict):
                profile = player_by_id.get(player_id)
            if not isinstance(profile, dict):
                failed += 1
                logger.warning("NFL profile missing for player_id=%d", player_id)
                continue

            player = parse_nfl_player(profile)
            if player.id == 0:
                player.id = player_id
            upsert_player(conn, "NFL", player)
            upsert_provider_entity_map(
                conn, "bdl", "NFL", "player", str(player_id), player.id
            )
            players_seeded += 1

            if idx % 100 == 0:
                click.echo(f"NFL profile progress: {idx}/{len(player_ids)}")
    finally:
        handler.close()

    return teams_seeded, players_seeded, failed


def _seed_football_metadata(
    conn: psycopg.Connection,
    api_token: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
) -> tuple[int, int, int]:
    teams_seeded = 0
    players_seeded = 0
    failed = 0

    sm_season_id = resolve_provider_season_id(conn, league, season)
    if not sm_season_id:
        raise RuntimeError(
            f"No SportMonks season mapping for league={league} season={season}"
        )

    handler = FootballHandler(api_token)
    try:
        teams = handler.get_teams(sm_season_id)
        if max_teams is not None:
            teams = teams[:max_teams]
        for team in teams:
            upsert_team(conn, "FOOTBALL", team)
            upsert_provider_entity_map(
                conn, "sportmonks", "FOOTBALL", "team", str(team.id), team.id
            )
            teams_seeded += 1

        player_team: dict[int, int] = {}
        player_jersey: dict[int, Any] = {}
        for team in teams:
            squad = handler.get_team_squad(sm_season_id, team.id)
            for entry in squad:
                player_id = entry.get("player_id")
                if not isinstance(player_id, int):
                    player_id = entry.get("id")
                if not isinstance(player_id, int):
                    continue
                player_team[player_id] = team.id

                jersey_number = entry.get("jersey_number")
                if jersey_number is None:
                    jersey_number = entry.get("number")
                if jersey_number is not None:
                    player_jersey[player_id] = jersey_number

        player_ids = sorted(player_team.keys())
        if max_players is not None:
            player_ids = player_ids[:max_players]

        click.echo(f"Seeding {len(player_ids)} Football player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            profile = handler.get_player_profile(player_id)
            if not isinstance(profile, dict):
                failed += 1
                logger.warning("Football profile missing for player_id=%d", player_id)
                continue

            player = parse_football_player(profile)
            if player.id == 0:
                player.id = player_id
            player.team_id = player_team.get(player_id)
            jersey_number = player_jersey.get(player_id)
            if jersey_number is not None:
                player.meta["jersey_number"] = jersey_number

            upsert_player(conn, "FOOTBALL", player)
            upsert_provider_entity_map(
                conn,
                "sportmonks",
                "FOOTBALL",
                "player",
                str(player_id),
                player.id,
            )
            players_seeded += 1

            if idx % 100 == 0:
                click.echo(f"Football profile progress: {idx}/{len(player_ids)}")
    finally:
        handler.close()

    return teams_seeded, players_seeded, failed


@cli.command("seed")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option(
    "--max-teams",
    type=int,
    default=None,
    help="Optional cap on team profile fetches",
)
@click.option(
    "--max-players",
    type=int,
    default=None,
    help="Optional cap on player profile fetches",
)
def seed(
    sport: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
) -> None:
    """Seed team/player metadata from provider profile endpoints."""
    if max_teams is not None and max_teams <= 0:
        click.echo("--max-teams must be greater than zero", err=True)
        sys.exit(1)
    if max_players is not None and max_players <= 0:
        click.echo("--max-players must be greater than zero", err=True)
        sys.exit(1)

    cfg = config_mod.load()
    pool = create_pool(cfg)

    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        sport_upper = sport.upper()

        with get_conn(pool) as conn:
            if sport_upper == "NBA":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NBA meta seed", err=True)
                    sys.exit(1)
                teams_seeded, players_seeded, failed = _seed_nba_metadata(
                    conn, cfg.bdl_api_key, max_teams, max_players
                )
            elif sport_upper == "NFL":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NFL meta seed", err=True)
                    sys.exit(1)
                teams_seeded, players_seeded, failed = _seed_nfl_metadata(
                    conn, cfg.bdl_api_key, season, max_teams, max_players
                )
            elif sport_upper == "FOOTBALL":
                if not league:
                    click.echo("--league is required for football meta seed", err=True)
                    sys.exit(1)
                if not cfg.sportmonks_api_token:
                    click.echo(
                        "SPORTMONKS_API_TOKEN is required for football meta seed", err=True
                    )
                    sys.exit(1)
                teams_seeded, players_seeded, failed = _seed_football_metadata(
                    conn,
                    cfg.sportmonks_api_token,
                    season,
                    league,
                    max_teams,
                    max_players,
                )
            else:
                click.echo(f"Unsupported sport: {sport}", err=True)
                sys.exit(1)

            click.echo(
                f"Meta seed complete sport={sport_upper} "
                f"teams={teams_seeded} players={players_seeded} failed={failed}"
            )
    finally:
        pool.close()


if __name__ == "__main__":
    cli()
