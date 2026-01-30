"""Football CLI for seeding data."""

import asyncio
import logging
import os
import sys

import asyncpg
import click

from .client import SportMonksClient
from .seeder import FootballSeeder
from .tables import LEAGUES, PREMIER_LEAGUE_SEASONS

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger(__name__)


def get_database_url() -> str:
    """Get database URL from environment."""
    url = os.environ.get("DATABASE_URL")
    if not url:
        click.echo("ERROR: DATABASE_URL environment variable not set", err=True)
        sys.exit(1)
    return url


def get_api_token() -> str:
    """Get SportMonks API token from environment."""
    token = os.environ.get("SPORTMONKS_API_TOKEN")
    if not token:
        click.echo("ERROR: SPORTMONKS_API_TOKEN environment variable not set", err=True)
        sys.exit(1)
    return token


@click.group()
def cli():
    """Scoracle Football seeder CLI."""
    pass


@cli.command()
@click.option("--team-id", default=9, help="Team ID to test (default: 9 - Manchester City)")
def test_team(team_id: int):
    """Test seeding a single team."""
    asyncio.run(_test_team(team_id))


async def _test_team(team_id: int):
    db_url = get_database_url()
    api_token = get_api_token()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with SportMonksClient(api_token) as client:
            seeder = FootballSeeder(conn, client)
            result = await seeder.test_single_team(team_id)
            
            if result.errors:
                click.echo(f"FAILED: {result.errors}", err=True)
                sys.exit(1)
            else:
                click.echo(f"SUCCESS: Seeded {result.teams_upserted} team")
    finally:
        await conn.close()


@cli.command()
@click.option("--player-id", default=159665, help="Player ID to test (default: 159665 - Erling Haaland)")
def test_player(player_id: int):
    """Test seeding a single player."""
    asyncio.run(_test_player(player_id))


async def _test_player(player_id: int):
    db_url = get_database_url()
    api_token = get_api_token()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with SportMonksClient(api_token) as client:
            seeder = FootballSeeder(conn, client)
            result = await seeder.test_single_player(player_id)
            
            if result.errors:
                click.echo(f"FAILED: {result.errors}", err=True)
                sys.exit(1)
            else:
                click.echo(f"SUCCESS: Seeded {result.players_upserted} player")
    finally:
        await conn.close()


@cli.command()
@click.argument("season_id", type=int)
def seed_teams(season_id: int):
    """Seed teams for a season."""
    asyncio.run(_seed_teams(season_id))


async def _seed_teams(season_id: int):
    db_url = get_database_url()
    api_token = get_api_token()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with SportMonksClient(api_token) as client:
            seeder = FootballSeeder(conn, client)
            result = await seeder.seed_teams(season_id)
            
            click.echo(f"Seeded {result.teams_upserted} teams")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("season_id", type=int)
def seed_players(season_id: int):
    """Seed players for a season."""
    asyncio.run(_seed_players(season_id))


async def _seed_players(season_id: int):
    db_url = get_database_url()
    api_token = get_api_token()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with SportMonksClient(api_token) as client:
            seeder = FootballSeeder(conn, client)
            result = await seeder.seed_players(season_id)
            
            click.echo(f"Seeded {result.players_upserted} players")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("season_id", type=int)
@click.argument("league_id", type=int)
@click.argument("year", type=int)
def seed_season(season_id: int, league_id: int, year: int):
    """Seed all data for a season (teams, players, stats)."""
    asyncio.run(_seed_season(season_id, league_id, year))


async def _seed_season(season_id: int, league_id: int, year: int):
    db_url = get_database_url()
    api_token = get_api_token()
    
    click.echo(f"Seeding season {season_id} (League {league_id}, Year {year})")
    
    conn = await asyncpg.connect(db_url)
    try:
        async with SportMonksClient(api_token) as client:
            seeder = FootballSeeder(conn, client)
            result = await seeder.seed_season(season_id, league_id, year)
            
            click.echo("")
            click.echo("=" * 50)
            click.echo("SEED COMPLETE")
            click.echo("=" * 50)
            click.echo(f"  Teams:        {result.teams_upserted}")
            click.echo(f"  Players:      {result.players_upserted}")
            click.echo(f"  Team Stats:   {result.team_stats_upserted}")
            click.echo(f"  Player Stats: {result.player_stats_upserted}")
            click.echo(f"  Errors:       {len(result.errors)}")
            
            if result.errors:
                click.echo("")
                click.echo("First 10 errors:")
                for err in result.errors[:10]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
def seed_premier_league_2024():
    """Seed Premier League 2024-25 season."""
    # Premier League 2024-25 season ID: 23614
    asyncio.run(_seed_season(23614, 1, 2024))


@cli.command()
def list_leagues():
    """List available leagues and their IDs."""
    click.echo("Available Leagues:")
    click.echo("-" * 50)
    for league_id, info in LEAGUES.items():
        click.echo(f"  {league_id}: {info['name']} ({info['country']}) - SportMonks ID: {info['sportmonks_id']}")
    
    click.echo("")
    click.echo("Known Premier League Seasons:")
    click.echo("-" * 50)
    for year, season_id in sorted(PREMIER_LEAGUE_SEASONS.items()):
        click.echo(f"  {year}-{year+1:02d}: Season ID {season_id}")


if __name__ == "__main__":
    cli()
