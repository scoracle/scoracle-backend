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
from .handlers.apisports_images import seed_nba_images, seed_nfl_images
from shared.db import get_football_league_ids, resolve_provider_season_id

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
    *,
    purge_statless: bool = True,
) -> tuple[int, int, int, int]:
    """Seed NBA metadata via the BDL provider.

    BDL's /v1/players returns the all-time historical roster, so this shim
    follows the seed with a stat-less purge by default. The trigger lives
    here (not in the CLI) so a future provider swap can replace this shim
    with one that doesn't need the cleanup.
    """
    teams_seeded = 0
    players_seeded = 0
    failed = 0
    purged = 0

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

    if purge_statless:
        purged = _purge_statless(conn, "NBA")

    return teams_seeded, players_seeded, failed, purged


def _seed_nfl_metadata(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    max_teams: int | None,
    max_players: int | None,
    *,
    purge_statless: bool = True,
) -> tuple[int, int, int, int]:
    """Seed NFL metadata via the BDL provider.

    BDL's /nfl/v1/players returns the historical roster, so this shim
    follows the seed with a stat-less purge by default. The trigger lives
    here (not in the CLI) so a future provider swap can replace this shim
    with one that doesn't need the cleanup.
    """
    teams_seeded = 0
    players_seeded = 0
    failed = 0
    purged = 0

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

    if purge_statless:
        purged = _purge_statless(conn, "NFL")

    return teams_seeded, players_seeded, failed, purged


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
            team.league_id = league
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
@click.option(
    "--purge-statless/--no-purge-statless",
    default=True,
    help="Forwarded to provider shims. The BDL shims (NBA/NFL) drop "
    "players with no event_box_scores after seeding (rookies exempted). "
    "Football's SportMonks shim ignores this — its roster source is "
    "scoped, so no purge needed. Default: on.",
)
def seed(
    sport: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
    purge_statless: bool,
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
            purged = 0
            if sport_upper == "NBA":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NBA meta seed", err=True)
                    sys.exit(1)
                teams_seeded, players_seeded, failed, purged = _seed_nba_metadata(
                    conn, cfg.bdl_api_key, max_teams, max_players,
                    purge_statless=purge_statless,
                )
            elif sport_upper == "NFL":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NFL meta seed", err=True)
                    sys.exit(1)
                teams_seeded, players_seeded, failed, purged = _seed_nfl_metadata(
                    conn, cfg.bdl_api_key, season, max_teams, max_players,
                    purge_statless=purge_statless,
                )
            elif sport_upper == "FOOTBALL":
                if not cfg.sportmonks_api_token:
                    click.echo(
                        "SPORTMONKS_API_TOKEN is required for football meta seed", err=True
                    )
                    sys.exit(1)

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
                    click.echo(
                        f"Iterating {len(league_ids)} football leagues: {league_ids}"
                    )

                teams_seeded = 0
                players_seeded = 0
                failed = 0
                for lid in league_ids:
                    click.echo(f"--- league={lid} ---")
                    t, p, f = _seed_football_metadata(
                        conn,
                        cfg.sportmonks_api_token,
                        season,
                        lid,
                        max_teams,
                        max_players,
                    )
                    teams_seeded += t
                    players_seeded += p
                    failed += f
            else:
                click.echo(f"Unsupported sport: {sport}", err=True)
                sys.exit(1)

            # Purge (if any) is owned by the provider shim, not the CLI —
            # the BDL shims trigger it because BDL returns the all-time
            # roster; SportMonks doesn't, so the football shim returns 0.
            click.echo(
                f"Meta seed complete sport={sport_upper} "
                f"teams={teams_seeded} players={players_seeded} "
                f"failed={failed} purged={purged}"
            )
    finally:
        pool.close()


def _purge_statless(conn: psycopg.Connection, sport_upper: str) -> int:
    """Drop players for `sport_upper` that have no event_box_scores rows,
    keeping current-season rookies. Returns rowcount.

    Mirrors the rookie-aware filter in `purge-inactive` so the meta-seed
    auto-purge and the standalone command behave identically.
    """
    if sport_upper == "NBA":
        rookie_clause = (
            "AND (p.meta->>'draft_year')::int IS DISTINCT FROM "
            "(SELECT current_season FROM sports WHERE id = 'NBA')"
        )
    elif sport_upper == "NFL":
        # %% escapes the literal % so psycopg doesn't treat it as a placeholder.
        rookie_clause = (
            "AND (p.meta->>'experience' IS NULL "
            "OR p.meta->>'experience' NOT ILIKE 'rookie%%')"
        )
    else:
        return 0

    cur = conn.execute(
        f"""
        DELETE FROM players p
        WHERE p.sport = %s
          AND NOT EXISTS (
              SELECT 1 FROM event_box_scores ebs
              WHERE ebs.player_id = p.id AND ebs.sport = p.sport
          )
          {rookie_clause}
        """,
        (sport_upper,),
    )
    return cur.rowcount


@cli.command("images")
@click.argument("sport", type=click.Choice(["nba", "nfl"], case_sensitive=False))
@click.option("--season", type=int, required=True, help="Season year")
@click.option(
    "--dry-run",
    is_flag=True,
    default=False,
    help="Match + log only; don't write to DB. Still consumes API quota.",
)
def images(sport: str, season: int, dry_run: bool) -> None:
    """Seed logo + headshot URLs from api-sports (NBA and NFL).

    Box scores and stats continue to come from BDL. This command only
    populates team logo_url and player photo_url when they're currently
    NULL, and records api-sports entity mappings in provider_entity_map.
    """
    cfg = config_mod.load()
    if not cfg.api_sports_key:
        click.echo("API_SPORTS_KEY is required for image seed", err=True)
        sys.exit(1)

    pool = create_pool(cfg)
    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        with get_conn(pool) as conn:
            sport_upper = sport.upper()
            if sport_upper == "NBA":
                report = seed_nba_images(
                    conn, cfg.api_sports_key, season, dry_run=dry_run,
                )
            elif sport_upper == "NFL":
                report = seed_nfl_images(
                    conn, cfg.api_sports_key, season, dry_run=dry_run,
                )
            else:
                click.echo(f"Unsupported sport: {sport}", err=True)
                sys.exit(1)

            click.echo(
                f"Image seed complete sport={sport_upper} dry_run={dry_run} "
                f"api_calls={report.api_calls} "
                f"teams_mapped={report.teams_mapped} "
                f"team_logos_written={report.team_logos_written} "
                f"team_logos_skipped={report.team_logos_skipped_present} "
                f"teams_unmatched={report.teams_unmatched} "
                f"players_mapped={report.players_mapped} "
                f"player_photos_written={report.player_photos_written} "
                f"player_photos_skipped={report.player_photos_skipped_present} "
                f"players_unmatched={report.players_unmatched}"
            )
    finally:
        pool.close()


@cli.command("purge-inactive")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option(
    "--grace-days",
    type=int,
    default=30,
    help="Keep players added within this many days even if they have no box scores. Default 30. Pass 0 on first run since meta seed just timestamped everyone today.",
)
@click.option(
    "--dry-run",
    is_flag=True,
    default=False,
    help="Count what would be deleted without touching the DB.",
)
def purge_inactive(sport: str, grace_days: int, dry_run: bool) -> None:
    """Drop players we've never seen in event_box_scores.

    BDL's /players returns the all-time roster (thousands of historical
    entries). After event seeding completes we know which players have
    actually played — the rest are dead weight in the DB and noise in
    entity matching.

    Keeps:
      - Players with any event_box_scores row (any season)
      - Per-sport rookie exemption so first-year players don't get purged
        before they've logged a stat:
          * NBA  — meta.draft_year matches sports.current_season
          * NFL  — meta.experience starts with "Rookie" (BDL label)
          * FOOTBALL — none (handler doesn't tag rookies; falls back to
            grace_days only)
      - Players added within --grace-days (broad new-signing protection)

    Drops:
      - Everyone else

    Re-running is safe: future meta seed calls will re-introduce any player
    who reappears in BDL's list; fresh created_at keeps them in the grace
    window until they either play or age out.
    """
    cfg = config_mod.load()
    pool = create_pool(cfg)
    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        sport_upper = sport.upper()
        # Per-sport rookie clause inserted into the WHERE so we don't purge
        # first-year players who haven't logged a box score yet.
        if sport_upper == "NBA":
            rookie_clause = (
                "AND (p.meta->>'draft_year')::int IS DISTINCT FROM "
                "(SELECT current_season FROM sports WHERE id = 'NBA')"
            )
        elif sport_upper == "NFL":
            # %% escapes the literal % so psycopg doesn't treat it as a placeholder.
            rookie_clause = (
                "AND (p.meta->>'experience' IS NULL "
                "OR p.meta->>'experience' NOT ILIKE 'rookie%%')"
            )
        else:
            rookie_clause = ""

        purge_where = f"""
            WHERE p.sport = %s
              AND p.created_at < NOW() - (%s || ' days')::interval
              AND NOT EXISTS (
                  SELECT 1 FROM event_box_scores ebs
                  WHERE ebs.player_id = p.id AND ebs.sport = p.sport
              )
              {rookie_clause}
        """

        with get_conn(pool) as conn:
            # Always report the "would purge" count first.
            row = conn.execute(
                f"SELECT count(*) AS n FROM players p {purge_where}",
                (sport_upper, grace_days),
            ).fetchone()
            would_purge = row["n"] if row else 0

            total_row = conn.execute(
                "SELECT count(*) AS n FROM players WHERE sport = %s",
                (sport_upper,),
            ).fetchone()
            total = total_row["n"] if total_row else 0

            if dry_run:
                click.echo(
                    f"[dry-run] sport={sport_upper} total={total} "
                    f"would_purge={would_purge} would_keep={total - would_purge} "
                    f"grace_days={grace_days}"
                )
                return

            cur = conn.execute(
                f"DELETE FROM players p {purge_where}",
                (sport_upper, grace_days),
            )
            purged = cur.rowcount
            kept = total - purged
            click.echo(
                f"Purge complete sport={sport_upper} purged={purged} "
                f"kept={kept} grace_days={grace_days}"
            )
    finally:
        pool.close()


if __name__ == "__main__":
    cli()
