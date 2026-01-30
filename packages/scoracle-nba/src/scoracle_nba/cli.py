"""NBA CLI for seeding data."""

import asyncio
import logging
import os
import sys

import asyncpg
import click

from .client import BallDontLieNBA
from .seeder import NBASeeder

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


def get_api_key() -> str:
    """Get BallDontLie API key from environment."""
    key = os.environ.get("BALLDONTLIE_API_KEY")
    if not key:
        click.echo("ERROR: BALLDONTLIE_API_KEY environment variable not set", err=True)
        sys.exit(1)
    return key


@click.group()
def cli():
    """Scoracle NBA seeder CLI."""
    pass


@cli.command()
@click.option("--team-id", default=1, help="Team ID to test (default: 1 - Hawks)")
def test_team(team_id: int):
    """Test seeding a single team."""
    asyncio.run(_test_team(team_id))


async def _test_team(team_id: int):
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.test_single_team(team_id)
            
            if result.errors:
                click.echo(f"FAILED: {result.errors}", err=True)
                sys.exit(1)
            else:
                click.echo(f"SUCCESS: Seeded {result.teams_upserted} team")
    finally:
        await conn.close()


@cli.command()
@click.option("--player-id", default=115, help="Player ID to test (default: 115 - Stephen Curry)")
def test_player(player_id: int):
    """Test seeding a single player."""
    asyncio.run(_test_player(player_id))


async def _test_player(player_id: int):
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.test_single_player(player_id)
            
            if result.errors:
                click.echo(f"FAILED: {result.errors}", err=True)
                sys.exit(1)
            else:
                click.echo(f"SUCCESS: Seeded {result.players_upserted} player, {result.teams_upserted} team")
    finally:
        await conn.close()


@cli.command()
def seed_teams():
    """Seed all NBA teams."""
    asyncio.run(_seed_teams())


async def _seed_teams():
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.seed_teams()
            
            click.echo(f"Seeded {result.teams_upserted} teams")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
def seed_players():
    """Seed all NBA players."""
    asyncio.run(_seed_players())


async def _seed_players():
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.seed_players()
            
            click.echo(f"Seeded {result.players_upserted} players")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("season", type=int)
@click.option("--season-type", default="regular", help="Season type: regular, playoffs, playin, ist")
def seed_player_stats(season: int, season_type: str):
    """Seed player stats for a season."""
    asyncio.run(_seed_player_stats(season, season_type))


async def _seed_player_stats(season: int, season_type: str):
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.seed_player_stats(season, season_type)
            
            click.echo(f"Seeded {result.player_stats_upserted} player stats")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("season", type=int)
@click.option("--season-type", default="regular", help="Season type: regular, playoffs, playin, ist")
def seed_team_stats(season: int, season_type: str):
    """Seed team stats for a season."""
    asyncio.run(_seed_team_stats(season, season_type))


async def _seed_team_stats(season: int, season_type: str):
    db_url = get_database_url()
    api_key = get_api_key()
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.seed_team_stats(season, season_type)
            
            click.echo(f"Seeded {result.team_stats_upserted} team stats")
            if result.errors:
                click.echo(f"Errors: {len(result.errors)}", err=True)
                for err in result.errors[:5]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("season", type=int)
@click.option("--season-type", default="regular", help="Season type: regular, playoffs, playin, ist")
def seed_all(season: int, season_type: str):
    """Seed all NBA data for a season (teams, players, stats)."""
    asyncio.run(_seed_all(season, season_type))


async def _seed_all(season: int, season_type: str):
    db_url = get_database_url()
    api_key = get_api_key()
    
    click.echo(f"Starting full NBA seed for {season} ({season_type})")
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            result = await seeder.seed_all(season, season_type)
            
            click.echo("")
            click.echo("=" * 50)
            click.echo("SEED COMPLETE")
            click.echo("=" * 50)
            click.echo(f"  Teams:        {result.teams_upserted}")
            click.echo(f"  Players:      {result.players_upserted}")
            click.echo(f"  Player Stats: {result.player_stats_upserted}")
            click.echo(f"  Team Stats:   {result.team_stats_upserted}")
            click.echo(f"  Errors:       {len(result.errors)}")
            
            if result.errors:
                click.echo("")
                click.echo("First 10 errors:")
                for err in result.errors[:10]:
                    click.echo(f"  - {err}", err=True)
    finally:
        await conn.close()


@cli.command()
@click.argument("start_season", type=int)
@click.argument("end_season", type=int)
@click.option("--season-type", default="regular", help="Season type: regular, playoffs")
def backfill(start_season: int, end_season: int, season_type: str):
    """Backfill NBA data for multiple seasons."""
    asyncio.run(_backfill(start_season, end_season, season_type))


async def _backfill(start_season: int, end_season: int, season_type: str):
    db_url = get_database_url()
    api_key = get_api_key()
    
    click.echo(f"Starting NBA backfill from {start_season} to {end_season} ({season_type})")
    
    conn = await asyncpg.connect(db_url)
    try:
        async with BallDontLieNBA(api_key) as client:
            seeder = NBASeeder(conn, client)
            
            # Seed teams and players once (current roster)
            click.echo("Seeding teams...")
            teams_result = await seeder.seed_teams()
            click.echo(f"  -> {teams_result.teams_upserted} teams")
            
            click.echo("Seeding players...")
            players_result = await seeder.seed_players()
            click.echo(f"  -> {players_result.players_upserted} players")
            
            # Seed stats for each season
            total_player_stats = 0
            total_team_stats = 0
            total_errors = []
            
            for season in range(start_season, end_season + 1):
                click.echo(f"Seeding stats for {season}...")
                
                ps_result = await seeder.seed_player_stats(season, season_type)
                total_player_stats += ps_result.player_stats_upserted
                total_errors.extend(ps_result.errors)
                click.echo(f"  -> {ps_result.player_stats_upserted} player stats")
                
                ts_result = await seeder.seed_team_stats(season, season_type)
                total_team_stats += ts_result.team_stats_upserted
                total_errors.extend(ts_result.errors)
                click.echo(f"  -> {ts_result.team_stats_upserted} team stats")
            
            click.echo("")
            click.echo("=" * 50)
            click.echo("BACKFILL COMPLETE")
            click.echo("=" * 50)
            click.echo(f"  Seasons:      {start_season} - {end_season}")
            click.echo(f"  Teams:        {teams_result.teams_upserted}")
            click.echo(f"  Players:      {players_result.players_upserted}")
            click.echo(f"  Player Stats: {total_player_stats}")
            click.echo(f"  Team Stats:   {total_team_stats}")
            click.echo(f"  Errors:       {len(total_errors)}")
    finally:
        await conn.close()


if __name__ == "__main__":
    cli()
