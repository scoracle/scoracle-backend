"""Roster seeding CLI commands.

Commands:
  seed             — Seed team_rosters membership (top-down by team)
"""

from __future__ import annotations

import logging
import sys
from typing import Any

import click
import psycopg

from shared import config as config_mod
from shared.api_errors import RateLimitExhausted
from shared.db import (
    check_connectivity,
    create_pool,
    get_conn,
    get_football_league_ids,
    resolve_provider_season_id,
)
from shared.upsert import (
    deactivate_missing_team_rosters,
    sync_player_team_membership,
    upsert_player,
    upsert_provider_entity_map,
    upsert_team,
    upsert_team_roster,
)
from ..event.handlers.bdl_nba import NBAHandler, _parse_player as parse_nba_player
from ..event.handlers.bdl_nfl import NFLHandler, _parse_player as parse_nfl_player
from ..event.handlers.sportmonks_football import (
    FootballHandler,
    _parse_player as parse_football_player,
)

logger = logging.getLogger("roster_seeding")

_FOOTBALL_POSITIONS = {
    24: "Goalkeeper",
    25: "Defender",
    26: "Midfielder",
    27: "Attacker",
}


@click.group(name="roster")
def cli() -> None:
    """Roster seeding — top-down team memberships."""


def _as_int(value: Any) -> int | None:
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        try:
            return int(value.strip())
        except ValueError:
            return None
    return None


def _as_jersey(value: Any) -> str | None:
    if value is None:
        return None
    if isinstance(value, (int, float)):
        return str(int(value))
    if isinstance(value, str):
        v = value.strip()
        return v or None
    return None


def _bdl_active_players(
    handler: NBAHandler | NFLHandler,
    sport_upper: str,
    team_id: int,
    season: int,
) -> list[dict[str, Any]]:
    """Best-effort active-roster pull from BallDontLie.

    Tries active endpoints first, then falls back to filtered historical list.
    """
    if sport_upper == "NBA":
        paths = ["/v1/players/active", "/nba/v1/players/active"]
    else:
        paths = ["/nfl/v1/players/active", "/v1/players/active"]

    params_list: list[dict[str, Any]] = [
        {"team_ids[]": team_id, "per_page": 100},
        {"team_id": team_id, "per_page": 100},
        {"team_ids": team_id, "per_page": 100},
    ]
    if sport_upper == "NFL":
        for p in params_list:
            p["season"] = season

    last_exc: Exception | None = None
    for path in paths:
        for params in params_list:
            try:
                rows: list[dict[str, Any]] = []
                for page in handler.client.get_paginated(path, params):
                    rows.extend(page)
                if rows:
                    return rows
            except RateLimitExhausted:
                raise
            except Exception as exc:
                last_exc = exc
                continue

    if last_exc is not None:
        logger.info(
            "active endpoint unavailable; falling back to historical player list",
            extra={"sport": sport_upper, "team_id": team_id, "error": str(last_exc)},
        )

    if sport_upper == "NBA":
        rows = handler.get_all_players()
    else:
        rows = handler.get_all_players(season)

    out: list[dict[str, Any]] = []
    for row in rows:
        team = row.get("team") if isinstance(row, dict) else None
        if isinstance(team, dict) and _as_int(team.get("id")) == team_id:
            out.append(row)
    return out


def _seed_bdl_rosters(
    conn: psycopg.Connection,
    *,
    sport_upper: str,
    season: int,
    max_teams: int | None,
    max_players: int | None,
    api_key: str,
) -> tuple[int, int, int, int, int]:
    teams_seeded = 0
    unique_players: set[int] = set()
    roster_rows = 0
    deactivated = 0
    failed = 0

    if sport_upper == "NBA":
        handler: NBAHandler | NFLHandler = NBAHandler(api_key)
        parse_player = parse_nba_player
    else:
        handler = NFLHandler(api_key)
        parse_player = parse_nfl_player

    try:
        teams = handler.get_teams()
        if max_teams is not None:
            teams = teams[:max_teams]

        for team in teams:
            upsert_team(conn, sport_upper, team)
            upsert_provider_entity_map(
                conn, "bdl", sport_upper, "team", str(team.id), team.id
            )
            teams_seeded += 1

            try:
                rows = _bdl_active_players(handler, sport_upper, team.id, season)
            except RateLimitExhausted:
                raise
            except Exception as exc:
                failed += 1
                logger.warning(
                    "roster pull failed sport=%s team_id=%d error=%s",
                    sport_upper,
                    team.id,
                    exc,
                )
                continue

            active_ids: list[int] = []
            for raw in rows:
                player_id = _as_int(raw.get("id")) if isinstance(raw, dict) else None
                if not player_id:
                    continue

                if (
                    max_players is not None
                    and player_id not in unique_players
                    and len(unique_players) >= max_players
                ):
                    break

                player = parse_player(raw)
                if player.id == 0:
                    player.id = player_id
                player.team_id = team.id

                upsert_player(conn, sport_upper, player)
                upsert_provider_entity_map(
                    conn, "bdl", sport_upper, "player", str(player_id), player.id
                )
                upsert_team_roster(
                    conn,
                    sport_upper,
                    season,
                    team.id,
                    player.id,
                    position=player.position,
                    source="bdl_active",
                )
                sync_player_team_membership(
                    conn, sport_upper, player.id, team.id, team.league_id
                )

                unique_players.add(player.id)
                active_ids.append(player.id)
                roster_rows += 1

            deactivated += deactivate_missing_team_rosters(
                conn,
                sport_upper,
                season,
                team.id,
                active_ids,
            )
    finally:
        handler.close()

    return teams_seeded, len(unique_players), roster_rows, deactivated, failed


def _seed_football_rosters(
    conn: psycopg.Connection,
    api_token: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
) -> tuple[int, int, int, int, int]:
    teams_seeded = 0
    unique_players: set[int] = set()
    roster_rows = 0
    deactivated = 0
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
            team.league_id = league
            upsert_team(conn, "FOOTBALL", team)
            upsert_provider_entity_map(
                conn, "sportmonks", "FOOTBALL", "team", str(team.id), team.id
            )
            teams_seeded += 1

            squad = handler.get_team_squad(sm_season_id, team.id)
            active_ids: list[int] = []

            for entry in squad:
                player_id = _as_int(entry.get("player_id"))
                if player_id is None:
                    player_id = _as_int(entry.get("id"))
                if player_id is None:
                    continue

                if (
                    max_players is not None
                    and player_id not in unique_players
                    and len(unique_players) >= max_players
                ):
                    break

                profile = handler.get_player_profile(player_id)
                if isinstance(profile, dict):
                    player = parse_football_player(profile)
                    if player.id == 0:
                        player.id = player_id
                else:
                    name = (
                        entry.get("display_name")
                        or entry.get("name")
                        or f"Player {player_id}"
                    )
                    player = parse_football_player(
                        {"id": player_id, "display_name": name}
                    )

                player.team_id = team.id

                position = entry.get("position")
                if not isinstance(position, str):
                    position = _FOOTBALL_POSITIONS.get(
                        _as_int(entry.get("position_id"))
                    )
                if not isinstance(position, str) or not position.strip():
                    position = player.position

                jersey_number = _as_jersey(
                    entry.get("jersey_number") or entry.get("number")
                )
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
                upsert_team_roster(
                    conn,
                    "FOOTBALL",
                    season,
                    team.id,
                    player.id,
                    jersey_number=jersey_number,
                    position=position,
                    source="sportmonks_squad",
                )
                sync_player_team_membership(
                    conn, "FOOTBALL", player.id, team.id, league
                )

                unique_players.add(player.id)
                active_ids.append(player.id)
                roster_rows += 1

            deactivated += deactivate_missing_team_rosters(
                conn,
                "FOOTBALL",
                season,
                team.id,
                active_ids,
            )
    finally:
        handler.close()

    return teams_seeded, len(unique_players), roster_rows, deactivated, failed


@cli.command("seed")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Canonical season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option(
    "--max-teams", type=int, default=None, help="Optional cap on teams to process"
)
@click.option(
    "--max-players",
    type=int,
    default=None,
    help="Optional cap on unique players to process",
)
def seed(
    sport: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
) -> None:
    """Seed team_rosters membership from provider roster sources."""
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
                    click.echo(
                        "BALLDONTLIE_API_KEY is required for NBA roster seed", err=True
                    )
                    sys.exit(1)
                teams, players, rows, deactivated, failed = _seed_bdl_rosters(
                    conn,
                    sport_upper="NBA",
                    season=season,
                    max_teams=max_teams,
                    max_players=max_players,
                    api_key=cfg.bdl_api_key,
                )
            elif sport_upper == "NFL":
                if not cfg.bdl_api_key:
                    click.echo(
                        "BALLDONTLIE_API_KEY is required for NFL roster seed", err=True
                    )
                    sys.exit(1)
                teams, players, rows, deactivated, failed = _seed_bdl_rosters(
                    conn,
                    sport_upper="NFL",
                    season=season,
                    max_teams=max_teams,
                    max_players=max_players,
                    api_key=cfg.bdl_api_key,
                )
            else:
                if not cfg.sportmonks_api_token:
                    click.echo(
                        "SPORTMONKS_API_TOKEN is required for football roster seed",
                        err=True,
                    )
                    sys.exit(1)

                league_ids: list[int]
                if league:
                    league_ids = [league]
                else:
                    league_ids = get_football_league_ids(conn, season)
                    if not league_ids:
                        click.echo(
                            f"No provider_seasons rows found for football season={season}. "
                            "Add them or pass --league explicitly.",
                            err=True,
                        )
                        sys.exit(1)

                teams = players = rows = deactivated = failed = 0
                for lid in league_ids:
                    t, p, r, d, f = _seed_football_rosters(
                        conn,
                        cfg.sportmonks_api_token,
                        season,
                        lid,
                        max_teams,
                        max_players,
                    )
                    teams += t
                    players += p
                    rows += r
                    deactivated += d
                    failed += f

            click.echo(
                f"Roster seed complete sport={sport_upper} season={season} "
                f"teams={teams} players={players} roster_rows={rows} "
                f"deactivated={deactivated} failed={failed}"
            )
    finally:
        pool.close()


if __name__ == "__main__":
    cli()
