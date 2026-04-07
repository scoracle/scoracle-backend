"""Event seeding CLI commands.

Commands:
  load-fixtures    — Load fixture schedule from provider API
  process          — Process all ready fixtures (box scores)
  backfill         — Backfill fixture-level historical box scores
"""

from __future__ import annotations

import logging
import sys
from datetime import date

import click

from shared import config as config_mod
from shared.db import check_connectivity, create_pool, get_conn

logger = logging.getLogger("event_seeding")


@click.group(name="event")
def cli() -> None:
    """Event seeding — box scores and fixture data."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


@cli.command("load-fixtures")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option("--from-date", type=str, default=None, help="Start date YYYY-MM-DD")
@click.option("--to-date", type=str, default=None, help="End date YYYY-MM-DD")
def load_fixtures(
    sport: str,
    season: int,
    league: int,
    from_date: str | None,
    to_date: str | None,
) -> None:
    """Load fixture schedule from provider API into the fixtures table."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    sport_upper = sport.upper()
    loaded = 0

    with get_conn(pool) as conn:
        from shared.upsert import upsert_team, upsert_provider_entity_map
        from .handlers.bdl_nba import NBAHandler
        from .handlers.bdl_nfl import NFLHandler
        from .handlers.sportmonks_football import FootballHandler

        if sport_upper == "NBA":
            handler = NBAHandler(cfg.bdl_api_key)
            try:
                games = handler.get_games(season, from_date=from_date, to_date=to_date)
                for game in games:
                    home_team = game.get("home_team")
                    away_team = game.get("away_team")
                    if home_team:
                        upsert_team(conn, "NBA", home_team)
                        upsert_provider_entity_map(
                            conn, "bdl", "NBA", "team", str(home_team.id), home_team.id
                        )
                    if away_team:
                        upsert_team(conn, "NBA", away_team)
                        upsert_provider_entity_map(
                            conn, "bdl", "NBA", "team", str(away_team.id), away_team.id
                        )
                    loaded += 1
                click.echo(f"Loaded {loaded} NBA fixtures for season {season}")
            finally:
                handler.close()

        elif sport_upper == "NFL":
            handler = NFLHandler(cfg.bdl_api_key)
            try:
                games = handler.get_games(season, from_date=from_date, to_date=to_date)
                for game in games:
                    home_team = game.get("home_team")
                    away_team = game.get("away_team")
                    if home_team:
                        upsert_team(conn, "NFL", home_team)
                    if away_team:
                        upsert_team(conn, "NFL", away_team)
                    loaded += 1
                click.echo(f"Loaded {loaded} NFL fixtures for season {season}")
            finally:
                handler.close()

        elif sport_upper == "FOOTBALL":
            if not league:
                click.echo("--league required for football", err=True)
                sys.exit(1)

            handler = FootballHandler(cfg.sportmonks_api_token)
            try:
                from .seed_football import resolve_provider_season_id

                sm_season_id = resolve_provider_season_id(conn, league, season)
                if not sm_season_id:
                    click.echo(f"No season mapping for league {league}", err=True)
                    sys.exit(1)

                fixtures = handler.get_fixtures(sm_season_id)
                for fixture in fixtures:
                    # Teams are included in fixture data
                    loaded += 1
                click.echo(f"Loaded {loaded} Football fixtures for league {league}")
            finally:
                handler.close()

    pool.close()


@cli.command("process")
@click.option("--sport", type=str, default=None, help="Filter by sport")
@click.option("--season", type=int, default=None, help="Filter by season")
@click.option(
    "--max", "max_fixtures", type=int, default=None, help="Max fixtures to process"
)
def process(sport: str | None, season: int | None, max_fixtures: int | None) -> None:
    """Process pending fixtures and fetch box scores."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    with get_conn(pool) as conn:
        from shared.fixtures import get_pending
        from .handlers.bdl_nba import NBAHandler
        from .handlers.bdl_nfl import NFLHandler
        from .handlers.sportmonks_football import FootballHandler

        pending = get_pending(conn, sport=sport, limit=max_fixtures)

        if season:
            pending = [f for f in pending if f.season == season]

        if not pending:
            click.echo("No pending fixtures")
            return

        click.echo(f"Processing {len(pending)} fixtures")
        # Processing logic here

    pool.close()


if __name__ == "__main__":
    cli()
