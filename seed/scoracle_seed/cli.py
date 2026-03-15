"""Click CLI for the Scoracle seeder.

Commands:
  bootstrap-teams  — One-time team roster seed at season start
  load-fixtures    — Load fixture schedule from provider API (stub)
  process          — Process all ready fixtures (called by cron)
  seed-fixture     — Seed a single fixture by ID
  percentiles      — Recalculate percentiles (ad-hoc)
"""

from __future__ import annotations

import logging
import sys
import time
from collections import defaultdict

import click

from . import config as config_mod
from .db import check_connectivity, create_pool, get_conn

logger = logging.getLogger("scoracle_seed")


def _setup_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


@click.group()
def cli() -> None:
    """Scoracle Seed — thin data ingestion for sports statistics."""
    _setup_logging()


# -------------------------------------------------------------------------
# bootstrap-teams — one-time team roster seed
# -------------------------------------------------------------------------


@cli.command("bootstrap-teams")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, default=2025, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
def bootstrap_teams(sport: str, season: int, league: int) -> None:
    """Seed teams for a sport (one-time, at season start)."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    sport_upper = sport.upper()

    with get_conn(pool) as conn:
        if sport_upper == "NBA":
            from .bdl_nba import NBAHandler

            handler = NBAHandler(cfg.bdl_api_key)
            try:
                teams = handler.get_teams()
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "NBA", team)
                click.echo(f"Upserted {len(teams)} NBA teams")
            finally:
                handler.close()

        elif sport_upper == "NFL":
            from .bdl_nfl import NFLHandler

            handler = NFLHandler(cfg.bdl_api_key)
            try:
                teams = handler.get_teams()
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "NFL", team)
                click.echo(f"Upserted {len(teams)} NFL teams")
            finally:
                handler.close()

        elif sport_upper == "FOOTBALL":
            from .seed_football import resolve_provider_season_id, resolve_sm_league_id
            from .sportmonks_football import FootballHandler

            if not league:
                click.echo("--league is required for football", err=True)
                sys.exit(1)

            handler = FootballHandler(cfg.sportmonks_api_token)
            try:
                sm_season_id = resolve_provider_season_id(conn, league, season)
                if not sm_season_id:
                    click.echo(
                        f"No provider season found for league={league} season={season}",
                        err=True,
                    )
                    sys.exit(1)

                teams = handler.get_teams(sm_season_id)
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "FOOTBALL", team)
                click.echo(f"Upserted {len(teams)} Football teams")
            finally:
                handler.close()

    pool.close()


# -------------------------------------------------------------------------
# load-fixtures — load fixture schedule from provider API
# -------------------------------------------------------------------------


@cli.command("load-fixtures")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, default=2025, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
def load_fixtures(sport: str, season: int, league: int) -> None:
    """Load fixture schedule from provider API into the fixtures table.

    This is NEW functionality — provider fixture/schedule endpoints need
    to be explored and implemented per provider.
    """
    click.echo(
        f"load-fixtures for {sport.upper()} season={season} league={league}\n"
        "NOTE: Fixture loading from provider APIs is not yet implemented.\n"
        "Provider schedule endpoints need to be explored."
    )
    sys.exit(1)


# -------------------------------------------------------------------------
# process — process all ready fixtures
# -------------------------------------------------------------------------


@cli.command()
@click.option(
    "--sport", type=str, default=None, help="Filter by sport (NBA, NFL, FOOTBALL)"
)
@click.option(
    "--max", "max_fixtures", type=int, default=50, help="Max fixtures to process"
)
def process(sport: str | None, max_fixtures: int) -> None:
    """Process all ready fixtures (called by cron every 30 min)."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    start = time.monotonic()

    with get_conn(pool) as conn:
        from .fixtures import get_pending

        pending = get_pending(conn, sport=sport, limit=max_fixtures)

        if not pending:
            click.echo("No pending fixtures to seed")
            pool.close()
            return

        click.echo(f"Found {len(pending)} pending fixtures")

        # Group by (sport, season, league_id) to deduplicate API calls
        groups: dict[tuple[str, int, int], list] = defaultdict(list)
        for f in pending:
            key = (f.sport, f.season, f.league_id or 0)
            groups[key].append(f)

        succeeded = 0
        failed = 0

        for (grp_sport, grp_season, grp_league_id), fixtures in groups.items():
            representative = fixtures[0]
            click.echo(
                f"Seeding group: sport={grp_sport} season={grp_season} "
                f"league={grp_league_id} fixtures={len(fixtures)}"
            )

            try:
                result = _seed_fixture_group(
                    conn, cfg, representative, grp_sport, grp_season, grp_league_id
                )

                if result.errors:
                    # Record failure for all fixtures in group
                    from .fixtures import record_failure

                    for f in fixtures:
                        record_failure(conn, f.id, result.errors[0])
                    failed += len(fixtures)
                    click.echo(f"  FAILED: {result.errors[0]}")
                else:
                    # Finalize each fixture
                    from .upsert import finalize_fixture

                    for f in fixtures:
                        try:
                            finalize_fixture(conn, f.id)
                        except Exception as exc:
                            logger.warning("finalize_fixture %d failed: %s", f.id, exc)
                    succeeded += len(fixtures)
                    click.echo(f"  OK: {result.summary()}")

            except Exception as exc:
                from .fixtures import record_failure

                for f in fixtures:
                    record_failure(conn, f.id, str(exc))
                failed += len(fixtures)
                click.echo(f"  ERROR: {exc}")

    elapsed = time.monotonic() - start
    click.echo(
        f"Process complete: succeeded={succeeded} failed={failed} "
        f"duration={elapsed:.1f}s"
    )
    pool.close()


# -------------------------------------------------------------------------
# seed-fixture — seed a single fixture by ID
# -------------------------------------------------------------------------


@cli.command("seed-fixture")
@click.option("--id", "fixture_id", type=int, required=True, help="Fixture ID")
def seed_fixture_cmd(fixture_id: int) -> None:
    """Seed a single fixture by ID."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    with get_conn(pool) as conn:
        from .fixtures import get_by_id

        f = get_by_id(conn, fixture_id)
        if not f:
            click.echo(f"Fixture {fixture_id} not found", err=True)
            sys.exit(1)

        click.echo(
            f"Seeding fixture {f.id}: sport={f.sport} season={f.season} "
            f"home={f.home_team_id} away={f.away_team_id}"
        )

        result = _seed_fixture_group(conn, cfg, f, f.sport, f.season, f.league_id or 0)

        if result.errors:
            from .fixtures import record_failure

            record_failure(conn, f.id, result.errors[0])
            click.echo(f"FAILED: {result.errors[0]}", err=True)
            sys.exit(1)

        from .upsert import finalize_fixture

        players, teams = finalize_fixture(conn, f.id)
        click.echo(
            f"OK: {result.summary()} | percentiles: players={players} teams={teams}"
        )

    pool.close()


# -------------------------------------------------------------------------
# percentiles — ad-hoc recalculation
# -------------------------------------------------------------------------


@cli.command()
@click.option("--sport", type=str, required=True, help="Sport (NBA, NFL, FOOTBALL)")
@click.option("--season", type=int, required=True, help="Season year")
def percentiles(sport: str, season: int) -> None:
    """Recalculate percentiles for a sport/season (ad-hoc)."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    with get_conn(pool) as conn:
        row = conn.execute(
            "SELECT * FROM recalculate_percentiles(%s, %s)",
            (sport.upper(), season),
        ).fetchone()

        if row:
            click.echo(
                f"Percentiles recalculated: players={row['players_updated']} "
                f"teams={row['teams_updated']}"
            )
        else:
            click.echo("No results returned")

    pool.close()


# -------------------------------------------------------------------------
# Internal helpers
# -------------------------------------------------------------------------


def _seed_fixture_group(conn, cfg, fixture, sport, season, league_id):
    """Seed stats for a fixture group. Returns SeedResult."""
    from .models import SeedResult

    if sport == "NBA":
        from .bdl_nba import NBAHandler
        from .seed_nba import seed_nba

        handler = NBAHandler(cfg.bdl_api_key)
        try:
            return seed_nba(conn, handler, season)
        finally:
            handler.close()

    elif sport == "NFL":
        from .bdl_nfl import NFLHandler
        from .seed_nfl import seed_nfl

        handler = NFLHandler(cfg.bdl_api_key)
        try:
            return seed_nfl(conn, handler, season)
        finally:
            handler.close()

    elif sport == "FOOTBALL":
        from .seed_football import (
            resolve_provider_season_id,
            resolve_sm_league_id,
            seed_football_season,
        )
        from .sportmonks_football import FootballHandler

        handler = FootballHandler(cfg.sportmonks_api_token)
        try:
            sm_season_id = resolve_provider_season_id(conn, league_id, season)
            if not sm_season_id:
                result = SeedResult()
                result.add_error(
                    f"no provider season for league={league_id} season={season}"
                )
                return result

            sm_league_id, _ = resolve_sm_league_id(conn, league_id)
            if not sm_league_id:
                result = SeedResult()
                result.add_error(f"no sportmonks_id for league={league_id}")
                return result

            return seed_football_season(
                conn, handler, sm_season_id, league_id, season, sm_league_id
            )
        finally:
            handler.close()

    else:
        result = SeedResult()
        result.add_error(f"unknown sport: {sport}")
        return result
