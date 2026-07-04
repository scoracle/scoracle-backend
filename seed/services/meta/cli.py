"""Metadata seeding CLI commands.

Commands:
  seed             — Enrich roster-scoped team and player profiles

SEEDING LAYER RULE:
  Roster membership owns player discovery. Metadata enriches players already
  present in team_rosters for the requested season. This is especially important
  for BDL: its player-list endpoints expose historical league-wide payloads, so
  meta seed must never use those lists as the canonical player universe.
"""

from __future__ import annotations

import logging
import sys
from datetime import datetime, timezone
from typing import Any

import click
import psycopg

from shared import config as config_mod
from shared.api_errors import RateLimitExhausted
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


def _load_roster_player_ids(
    conn: psycopg.Connection,
    sport_upper: str,
    season: int,
    *,
    team_ids: list[int] | None = None,
    max_players: int | None = None,
) -> list[int]:
    """Return the season-scoped player universe for metadata enrichment.

    This is the metadata guardrail. For BDL sports, do not page through
    /players here: BDL returns historical league-wide player payloads. The
    roster service is the only place that decides who is in-scope for a season.
    """
    rows = _load_roster_player_rows(
        conn,
        sport_upper,
        season,
        team_ids=team_ids,
        max_players=max_players,
    )
    return [r["player_id"] for r in rows]


def _load_roster_player_rows(
    conn: psycopg.Connection,
    sport_upper: str,
    season: int,
    *,
    team_ids: list[int] | None = None,
    max_players: int | None = None,
) -> list[dict[str, Any]]:
    """Return active roster rows that define the metadata enrichment universe."""
    params: list[Any] = [sport_upper, season]
    team_filter = ""
    if team_ids is not None:
        team_filter = "AND team_id = ANY(%s)"
        params.append(team_ids)
    params.append(max_players or 1000000000)

    rows = conn.execute(
        f"""
        SELECT player_id, team_id, jersey_number
        FROM (
            SELECT DISTINCT ON (player_id)
                player_id,
                team_id,
                jersey_number,
                last_seen
            FROM team_rosters
            WHERE sport = %s
              AND season = %s
              AND is_active
              {team_filter}
            ORDER BY player_id, last_seen DESC, team_id
        ) roster_scope
        ORDER BY player_id
        LIMIT %s
        """,
        params,
    ).fetchall()
    return rows


def _commit_if_supported(conn: psycopg.Connection) -> None:
    commit = getattr(conn, "commit", None)
    if callable(commit):
        commit()


def _football_profile_hydrated_player_ids(
    conn: psycopg.Connection, player_ids: list[int]
) -> set[int]:
    if not player_ids:
        return set()

    rows = conn.execute(
        """
        SELECT id
        FROM players
        WHERE sport = 'FOOTBALL'
          AND id = ANY(%s)
          AND meta ->> 'profile_source' = 'sportmonks_player_profile'
        """,
        (player_ids,),
    ).fetchall()
    return {row["id"] for row in rows}


def _seed_nba_metadata(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    max_teams: int | None,
    max_players: int | None,
    *,
    purge_statless: bool = False,
) -> tuple[int, int, int, int]:
    """Seed NBA metadata via the BDL provider.

    BDL's /v1/players returns historical league-wide rows. Metadata enrichment
    therefore reads player IDs only from team_rosters for the requested season.
    Run `scoracle-seed roster seed nba --season <year>` before this command.
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
        team_ids = [team.id for team in teams]
        for team in teams:
            upsert_team(conn, "NBA", team)
            upsert_provider_entity_map(
                conn, "bdl", "NBA", "team", str(team.id), team.id
            )
            teams_seeded += 1

        player_ids = _load_roster_player_ids(
            conn,
            "NBA",
            season,
            team_ids=team_ids,
            max_players=max_players,
        )
        if not player_ids:
            raise RuntimeError(
                "No active NBA team_rosters rows found for season="
                f"{season}. Run `scoracle-seed roster seed nba --season {season}` "
                "before metadata enrichment."
            )

        click.echo(f"Seeding {len(player_ids)} roster-scoped NBA player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            profile = handler.get_player(player_id)
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
        raise RuntimeError(
            "meta seed no longer purges statless players. Roster seed defines "
            "the season-scoped universe; use `meta purge-inactive` only as an "
            "explicit operator cleanup."
        )

    return teams_seeded, players_seeded, failed, purged


def _seed_nfl_metadata(
    conn: psycopg.Connection,
    api_key: str,
    season: int,
    max_teams: int | None,
    max_players: int | None,
    *,
    purge_statless: bool = False,
) -> tuple[int, int, int, int]:
    """Seed NFL metadata via the BDL provider.

    BDL's /nfl/v1/players returns historical league-wide rows. Metadata
    enrichment therefore reads player IDs only from team_rosters for the
    requested season. Run `scoracle-seed roster seed nfl --season <year>`
    before this command.
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
        team_ids = [team.id for team in teams]
        for team in teams:
            upsert_team(conn, "NFL", team)
            upsert_provider_entity_map(
                conn, "bdl", "NFL", "team", str(team.id), team.id
            )
            teams_seeded += 1

        player_ids = _load_roster_player_ids(
            conn,
            "NFL",
            season,
            team_ids=team_ids,
            max_players=max_players,
        )
        if not player_ids:
            raise RuntimeError(
                "No active NFL team_rosters rows found for season="
                f"{season}. Run `scoracle-seed roster seed nfl --season {season}` "
                "before metadata enrichment."
            )

        click.echo(f"Seeding {len(player_ids)} roster-scoped NFL player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            profile = handler.get_player(player_id)
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
        raise RuntimeError(
            "meta seed no longer purges statless players. Roster seed defines "
            "the season-scoped universe; use `meta purge-inactive` only as an "
            "explicit operator cleanup."
        )

    return teams_seeded, players_seeded, failed, purged


def _seed_football_metadata(
    conn: psycopg.Connection,
    api_token: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
) -> tuple[int, int, int, bool]:
    teams_seeded = 0
    players_seeded = 0
    failed = 0
    paused_for_rate_limit = False

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
        team_ids = [team.id for team in teams]
        for team in teams:
            team.league_id = league
            upsert_team(conn, "FOOTBALL", team)
            upsert_provider_entity_map(
                conn, "sportmonks", "FOOTBALL", "team", str(team.id), team.id
            )
            teams_seeded += 1
        _commit_if_supported(conn)

        roster_rows = _load_roster_player_rows(
            conn,
            "FOOTBALL",
            season,
            team_ids=team_ids,
            max_players=max_players,
        )
        if not roster_rows:
            raise RuntimeError(
                "No active FOOTBALL team_rosters rows found for season="
                f"{season}. Run `scoracle-seed roster seed football --season {season}` "
                "before metadata enrichment."
            )

        roster_player_ids = [row["player_id"] for row in roster_rows]
        hydrated_ids = _football_profile_hydrated_player_ids(conn, roster_player_ids)
        if hydrated_ids:
            roster_rows = [
                row for row in roster_rows if row["player_id"] not in hydrated_ids
            ]
            click.echo(
                "Skipping "
                f"{len(hydrated_ids)} already-hydrated Football player profiles"
            )

        player_ids = [row["player_id"] for row in roster_rows]
        player_team = {row["player_id"]: row["team_id"] for row in roster_rows}
        player_jersey = {
            row["player_id"]: row["jersey_number"]
            for row in roster_rows
            if row.get("jersey_number") is not None
        }

        click.echo(f"Seeding {len(player_ids)} roster-scoped Football player profiles")
        for idx, player_id in enumerate(player_ids, start=1):
            try:
                profile = handler.get_player_profile(player_id)
            except RateLimitExhausted:
                paused_for_rate_limit = True
                remaining = len(player_ids) - idx + 1
                click.echo(
                    "SportMonks rate limit exhausted; committed hydrated profiles "
                    f"and paused with {remaining} player profiles remaining.",
                    err=True,
                )
                _commit_if_supported(conn)
                break

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
            player.meta["profile_source"] = "sportmonks_player_profile"
            player.meta["profile_hydrated_at"] = datetime.now(timezone.utc).isoformat()

            upsert_player(conn, "FOOTBALL", player)
            upsert_provider_entity_map(
                conn,
                "sportmonks",
                "FOOTBALL",
                "player",
                str(player_id),
                player.id,
            )
            _commit_if_supported(conn)
            players_seeded += 1

            if idx % 100 == 0:
                click.echo(f"Football profile progress: {idx}/{len(player_ids)}")
    finally:
        handler.close()

    return teams_seeded, players_seeded, failed, paused_for_rate_limit


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
    default=False,
    help="Deprecated guardrail. Meta seed no longer purges players; roster "
    "seed defines the season-scoped player universe. Passing --purge-statless "
    "now fails closed. Default: off.",
)
def seed(
    sport: str,
    season: int,
    league: int,
    max_teams: int | None,
    max_players: int | None,
    purge_statless: bool,
) -> None:
    """Enrich team/player metadata for the season-scoped roster universe.

    Run `scoracle-seed roster seed <sport> --season <year>` first. Metadata
    seeding is deliberately not a discovery pass for NBA/NFL because BDL's
    player-list payloads include historical league-wide entities.
    """
    if max_teams is not None and max_teams <= 0:
        click.echo("--max-teams must be greater than zero", err=True)
        sys.exit(1)
    if max_players is not None and max_players <= 0:
        click.echo("--max-players must be greater than zero", err=True)
        sys.exit(1)
    if purge_statless:
        click.echo(
            "--purge-statless is no longer supported on meta seed. "
            "Run roster seed first; use meta purge-inactive only as an explicit cleanup.",
            err=True,
        )
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
            paused_for_rate_limit = False
            if sport_upper == "NBA":
                if not cfg.bdl_api_key:
                    click.echo(
                        "BALLDONTLIE_API_KEY is required for NBA meta seed", err=True
                    )
                    sys.exit(1)
                teams_seeded, players_seeded, failed, purged = _seed_nba_metadata(
                    conn,
                    cfg.bdl_api_key,
                    season,
                    max_teams,
                    max_players,
                    purge_statless=purge_statless,
                )
            elif sport_upper == "NFL":
                if not cfg.bdl_api_key:
                    click.echo(
                        "BALLDONTLIE_API_KEY is required for NFL meta seed", err=True
                    )
                    sys.exit(1)
                teams_seeded, players_seeded, failed, purged = _seed_nfl_metadata(
                    conn,
                    cfg.bdl_api_key,
                    season,
                    max_teams,
                    max_players,
                    purge_statless=purge_statless,
                )
            elif sport_upper == "FOOTBALL":
                if not cfg.sportmonks_api_token:
                    click.echo(
                        "SPORTMONKS_API_TOKEN is required for football meta seed",
                        err=True,
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
                    t, p, f, paused = _seed_football_metadata(
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
                    if paused:
                        paused_for_rate_limit = True
                        break
            else:
                click.echo(f"Unsupported sport: {sport}", err=True)
                sys.exit(1)

            # No purge here. The roster service owns player discovery and
            # season-scoped membership; metadata only enriches that universe.
            status = "paused" if sport_upper == "FOOTBALL" and paused_for_rate_limit else "complete"
            click.echo(
                f"Meta seed {status} sport={sport_upper} "
                f"teams={teams_seeded} players={players_seeded} "
                f"failed={failed} purged={purged}"
            )
    finally:
        pool.close()


def _purge_off_roster(conn: psycopg.Connection, sport_upper: str, season: int) -> int:
    """Drop players for `sport_upper` that are off-roster and statless.

    A player is deleted only when BOTH are true:
      1) no event_box_scores rows for the sport
      2) no active team_rosters row for the given season

    Keeps current-season rookies (NBA draft year / NFL experience label).
    Returns rowcount.

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
          AND NOT EXISTS (
              SELECT 1 FROM team_rosters tr
              WHERE tr.sport = p.sport
                AND tr.player_id = p.id
                AND tr.season = %s
                AND tr.is_active
          )
          {rookie_clause}
        """,
        (sport_upper, season),
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
                    conn,
                    cfg.api_sports_key,
                    season,
                    dry_run=dry_run,
                )
            elif sport_upper == "NFL":
                report = seed_nfl_images(
                    conn,
                    cfg.api_sports_key,
                    season,
                    dry_run=dry_run,
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
    help="Keep players added within this many days even if they have no box scores. Default 30.",
)
@click.option(
    "--dry-run",
    is_flag=True,
    default=False,
    help="Count what would be deleted without touching the DB.",
)
def purge_inactive(sport: str, grace_days: int, dry_run: bool) -> None:
    """Explicit operator cleanup for old off-roster players.

    Normal metadata seeding no longer needs this command: roster seed defines
    the season-scoped player universe, and meta seed only enriches that
    universe. Use this manually when cleaning legacy historical BDL bloat.

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

    Also preserves players present on an active current-season team_rosters row,
    so top-down roster seeding cannot be undone by this cleanup pass.

    Re-running is safe for rostered players because active team_rosters rows are
    preserved. Players removed by this command come back only if a future roster
    seed or event seed creates them again.
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
              AND NOT EXISTS (
                  SELECT 1 FROM team_rosters tr
                  WHERE tr.sport = p.sport
                    AND tr.player_id = p.id
                    AND tr.season = %s
                    AND tr.is_active
              )
              {rookie_clause}
        """

        with get_conn(pool) as conn:
            season_row = conn.execute(
                "SELECT current_season FROM sports WHERE id = %s",
                (sport_upper,),
            ).fetchone()
            current_season = season_row["current_season"] if season_row else 0

            # Always report the "would purge" count first.
            row = conn.execute(
                f"SELECT count(*) AS n FROM players p {purge_where}",
                (sport_upper, grace_days, current_season),
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
                (sport_upper, grace_days, current_season),
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
