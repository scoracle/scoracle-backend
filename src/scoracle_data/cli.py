#!/usr/bin/env python3
"""
Command-line interface for stats database management.

Usage:
    python -m scoracle_data.cli init
    python -m scoracle_data.cli seed --sport NBA --season 2025
    python -m scoracle_data.cli seed --all
    python -m scoracle_data.cli percentiles --sport NBA --season 2025
    python -m scoracle_data.cli status
    python -m scoracle_data.cli export --format json

    # Schedule-driven fixture commands
    python -m scoracle_data.cli fixtures load schedule.csv --sport FOOTBALL
    python -m scoracle_data.cli fixtures status
    python -m scoracle_data.cli fixtures pending
    python -m scoracle_data.cli fixtures run-scheduler
    python -m scoracle_data.cli fixtures seed 12345
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("statsdb.cli")

from .core.types import (
    Sport,
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
)

ALL_SPORTS = [s.value for s in Sport]


def get_db():
    """Get the singleton database connection."""
    from .pg_connection import get_db as _get_db

    return _get_db()


def _get_provider_client(sport_id: str):
    """Create a provider client for the given sport.

    Reads API keys from environment variables:
    - BALLDONTLIE_API_KEY for NBA/NFL
    - SPORTMONKS_API_TOKEN for FOOTBALL
    """
    if sport_id in ("NBA", "NFL"):
        api_key = os.environ.get("BALLDONTLIE_API_KEY")
        if not api_key:
            raise ValueError("BALLDONTLIE_API_KEY environment variable required")
        if sport_id == "NBA":
            from .providers.balldontlie_nba import BallDontLieNBA

            return BallDontLieNBA(api_key=api_key)
        else:
            from .providers.balldontlie_nfl import BallDontLieNFL

            return BallDontLieNFL(api_key=api_key)
    elif sport_id == "FOOTBALL":
        api_token = os.environ.get("SPORTMONKS_API_TOKEN")
        if not api_token:
            raise ValueError("SPORTMONKS_API_TOKEN environment variable required")
        from .providers.sportmonks import SportMonksClient

        return SportMonksClient(api_token=api_token)
    else:
        raise ValueError(f"Unknown sport: {sport_id}")


def _get_seed_runner(sport_id: str, db, client):
    """Create a seed runner for the given sport."""
    from .seeders import NBASeedRunner, NFLSeedRunner, FootballSeedRunner

    runners = {
        "NBA": NBASeedRunner,
        "NFL": NFLSeedRunner,
        "FOOTBALL": FootballSeedRunner,
    }
    runner_class = runners.get(sport_id)
    if not runner_class:
        raise ValueError(f"Unknown sport: {sport_id}")
    return runner_class(db, client)


def cmd_init(args: argparse.Namespace) -> int:
    """Initialize the database with schema."""
    from .schema import init_database

    db = get_db()

    try:
        logger.info("Initializing stats database...")
        init_database(db)
        logger.info("Database initialized successfully")
        return 0
    except Exception as e:
        logger.error("Failed to initialize database: %s", e)
        return 1
    finally:
        db.close()


def cmd_status(args: argparse.Namespace) -> int:
    """Show database status."""
    from .schema import get_schema_version, get_table_counts

    db = get_db()

    try:
        if not db.is_initialized():
            logger.info("Database is not initialized")
            return 1

        version = get_schema_version(db)
        counts = get_table_counts(db)

        print(f"\nStats Database Status")
        print(f"=" * 50)
        print(f"Backend: PostgreSQL")
        print(f"Schema Version: {version}")
        print(f"Last Full Sync: {db.get_meta('last_full_sync') or 'Never'}")
        print(f"Last Incremental: {db.get_meta('last_incremental_sync') or 'Never'}")
        print()
        print("Table Counts:")
        for table, count in sorted(counts.items()):
            print(f"  {table}: {count:,}")

        # Show sports summary using unified tables
        print()
        print("Sports Summary:")
        sports = db.fetchall("SELECT * FROM sports WHERE is_active = true")
        for sport in sports:
            players = db.fetchone(
                f"SELECT COUNT(*) as count FROM {PLAYERS_TABLE} WHERE sport = %s",
                (sport["id"],),
            )
            teams = db.fetchone(
                f"SELECT COUNT(*) as count FROM {TEAMS_TABLE} WHERE sport = %s",
                (sport["id"],),
            )
            print(
                f"  {sport['id']}: {players['count']:,} players, {teams['count']:,} teams"
            )

        return 0

    except Exception as e:
        logger.error("Failed to get status: %s", e)
        return 1
    finally:
        db.close()


async def cmd_seed_async(args: argparse.Namespace) -> int:
    """Seed data using canonical provider clients (BallDontLie / SportMonks).

    Uses the new seed runners which talk directly to the provider APIs
    and write to PostgreSQL via psycopg.
    """
    from .seeders.seed_football import LEAGUES, PREMIER_LEAGUE_SEASONS

    db = get_db()

    # Initialize database if needed
    if not db.is_initialized():
        from .schema import init_database

        logger.info("Database not initialized, running initialization...")
        init_database(db)

    sports_to_seed = []
    if args.all:
        sports_to_seed = ALL_SPORTS
    elif args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        logger.error("Must specify --sport or --all")
        return 1

    season = args.season or 2025

    from .seeders.common import SeedResult

    total = SeedResult()

    for sport_id in sports_to_seed:
        try:
            client = _get_provider_client(sport_id)
        except ValueError as e:
            logger.error("%s", e)
            return 1

        runner = _get_seed_runner(sport_id, db, client)
        logger.info("Seeding %s for season %d...", sport_id, season)

        try:
            async with client:
                if sport_id == "FOOTBALL":
                    # For now, only seed leagues where we have a season ID.
                    # PREMIER_LEAGUE_SEASONS maps year -> sportmonks_season_id.
                    # TODO: add season ID lookups for other leagues.
                    sm_season_id = PREMIER_LEAGUE_SEASONS.get(season)
                    if not sm_season_id:
                        logger.warning(
                            "No SportMonks season ID for year %d, skipping", season
                        )
                        continue

                    # Seed Premier League (internal league_id=1)
                    league_id = 1
                    league_info = LEAGUES[league_id]
                    logger.info(
                        "Seeding %s %d (season_id=%d)...",
                        league_info["name"],
                        season,
                        sm_season_id,
                    )
                    result = await runner.seed_season(
                        sm_season_id,
                        league_id,
                        season,
                    )
                    total = total + result
                else:
                    result = await runner.seed_all(season)
                    total = total + result
        except Exception as e:
            logger.error("Failed to seed %s: %s", sport_id, e)
            import traceback

            traceback.print_exc()

    # Update metadata
    db.set_meta("last_full_sync", datetime.now(tz=timezone.utc).isoformat())

    logger.info(
        "Total seeded: %d teams, %d players, %d player stats, %d team stats, %d errors",
        total.teams_upserted,
        total.players_upserted,
        total.player_stats_upserted,
        total.team_stats_upserted,
        len(total.errors),
    )

    db.close()
    return 0


def cmd_seed(args: argparse.Namespace) -> int:
    """Wrapper to run async seed command."""
    return asyncio.run(cmd_seed_async(args))


def cmd_percentiles(args: argparse.Namespace) -> int:
    """Recalculate or archive percentiles using pure Python calculator."""
    from .percentiles.python_calculator import PythonPercentileCalculator

    db = get_db()

    if not db.is_initialized():
        logger.error("Database not initialized. Run 'init' first.")
        return 1

    calculator = PythonPercentileCalculator(db)

    sports = [args.sport.upper()] if args.sport else ALL_SPORTS
    season = args.season or 2025

    # Handle archive subcommand
    if hasattr(args, "percentiles_command") and args.percentiles_command == "archive":
        for sport_id in sports:
            logger.info("Archiving percentiles for %s %d...", sport_id, season)
            try:
                count = calculator.archive_season(sport_id, season, is_final=args.final)
                logger.info(
                    "%s archived: %d records (final=%s)", sport_id, count, args.final
                )
            except Exception as e:
                logger.error("Failed to archive percentiles for %s: %s", sport_id, e)
                import traceback

                traceback.print_exc()
        db.close()
        return 0

    # Default: recalculate percentiles
    for sport_id in sports:
        logger.info("Calculating percentiles for %s %d...", sport_id, season)
        try:
            result = calculator.recalculate_all_percentiles(sport_id, season)
            logger.info(
                "%s percentiles calculated: %d players, %d teams",
                sport_id,
                result["players"],
                result["teams"],
            )
        except Exception as e:
            logger.error("Failed to calculate percentiles for %s: %s", sport_id, e)
            import traceback

            traceback.print_exc()

    db.close()
    return 0


def cmd_export(args: argparse.Namespace) -> int:
    """Export data to files."""
    db = get_db()

    if not db.is_initialized():
        logger.error("Database not initialized.")
        return 1

    output_dir = Path(args.output) if args.output else Path("./exports")
    output_dir.mkdir(parents=True, exist_ok=True)

    format_type = args.format or "json"
    sport = args.sport.upper() if args.sport else None
    season = args.season or 2025

    logger.info("Exporting data to %s in %s format...", output_dir, format_type)

    sports = [sport] if sport else ALL_SPORTS

    for sport_id in sports:
        # Export players with stats using unified tables
        players = db.fetchall(
            f"SELECT * FROM {PLAYERS_TABLE} WHERE sport = %s",
            (sport_id,),
        )

        player_data = []
        for player in players:
            stats_row = db.fetchone(
                f"SELECT stats, percentiles FROM {PLAYER_STATS_TABLE} "
                f"WHERE player_id = %s AND sport = %s AND season = %s",
                (player["id"], sport_id, season),
            )
            stats = dict(stats_row) if stats_row else None
            percentiles = stats_row["percentiles"] if stats_row else None
            player_data.append(
                {
                    "player": dict(player),
                    "stats": stats,
                    "percentiles": percentiles,
                }
            )

        # Export teams with stats using unified tables
        teams = db.fetchall(
            f"SELECT * FROM {TEAMS_TABLE} WHERE sport = %s",
            (sport_id,),
        )

        team_data = []
        for team in teams:
            stats_row = db.fetchone(
                f"SELECT stats, percentiles FROM {TEAM_STATS_TABLE} "
                f"WHERE team_id = %s AND sport = %s AND season = %s",
                (team["id"], sport_id, season),
            )
            t_stats = dict(stats_row) if stats_row else None
            t_percentiles = stats_row["percentiles"] if stats_row else None
            team_data.append(
                {
                    "team": dict(team),
                    "stats": t_stats,
                    "percentiles": t_percentiles,
                }
            )

        export_data = {
            "sport": sport_id,
            "season": season,
            "exported_at": datetime.now(tz=timezone.utc).isoformat(),
            "players": player_data,
            "teams": team_data,
        }

        output_file = output_dir / f"{sport_id.lower()}_{season}.{format_type}"

        if format_type == "json":
            with open(output_file, "w") as f:
                json.dump(export_data, f, indent=2, default=str)
        else:
            logger.error("Unsupported format: %s", format_type)
            continue

        logger.info(
            "Exported %s %d: %d players, %d teams -> %s",
            sport_id,
            season,
            len(player_data),
            len(team_data),
            output_file,
        )

    db.close()
    return 0


def cmd_query(args: argparse.Namespace) -> int:
    """Run a query against the database."""
    db = get_db()

    if not db.is_initialized():
        logger.error("Database not initialized.")
        return 1

    query_type = args.type
    sport = args.sport.upper() if args.sport else "NBA"
    season = args.season or 2025

    if query_type == "leaders":
        from .queries import PlayerQueries

        pq = PlayerQueries(db)
        stat = args.stat or "points_per_game"
        limit = args.limit or 25

        results = pq.get_stat_leaders(sport, season, stat, limit)

        print(f"\n{sport} {season} - Top {limit} {stat}")
        print("=" * 60)
        for r in results:
            print(
                f"{r['rank']:3}. {r['name']:<25} {r['stat_value']:>10.1f}  ({r['team_name'] or 'N/A'})"
            )

    elif query_type == "standings":
        from .queries import TeamQueries

        tq = TeamQueries(db)

        results = tq.get_standings(
            sport,
            season,
            league_id=args.league,
            conference=args.conference,
        )

        print(f"\n{sport} {season} Standings")
        print("=" * 60)
        for r in results:
            if sport == "FOOTBALL":
                print(
                    f"{r['rank']:3}. {r['name']:<25} {r['points']:3} pts  ({r['wins']}-{r['draws']}-{r['losses']})"
                )
            else:
                print(
                    f"{r['rank']:3}. {r['name']:<25} {r['win_pct']:.3f}  ({r['wins']}-{r['losses']})"
                )

    elif query_type == "profile":
        entity_type = args.entity_type or "player"
        entity_id = args.entity_id

        if not entity_id:
            logger.error("Must specify --entity-id for profile query")
            return 1

        if entity_type == "player":
            from .queries import PlayerQueries

            pq = PlayerQueries(db)
            result = pq.get_player_profile(entity_id, sport, season)
        else:
            from .queries import TeamQueries

            tq = TeamQueries(db)
            result = tq.get_team_profile(entity_id, sport, season)

        if result:
            print(json.dumps(result, indent=2, default=str))
        else:
            print("Not found")

    else:
        logger.error("Unknown query type: %s", query_type)
        return 1

    db.close()
    return 0


# =============================================================================
# FIXTURE MANAGEMENT COMMANDS
# =============================================================================


def get_pg_db():
    """Get PostgreSQL database connection for fixture/ML operations."""
    return get_db()


def cmd_fixtures(args: argparse.Namespace) -> int:
    """Route fixture subcommands."""
    subcommand = args.fixtures_command

    if subcommand == "load":
        return cmd_fixtures_load(args)
    elif subcommand == "status":
        return cmd_fixtures_status(args)
    elif subcommand == "pending":
        return cmd_fixtures_pending(args)
    elif subcommand == "upcoming":
        return cmd_fixtures_upcoming(args)
    elif subcommand == "failed":
        return cmd_fixtures_failed(args)
    elif subcommand == "recent":
        return cmd_fixtures_recent(args)
    elif subcommand == "seed":
        return asyncio.run(cmd_fixtures_seed_async(args))
    elif subcommand == "run-scheduler":
        return asyncio.run(cmd_fixtures_run_scheduler_async(args))
    elif subcommand == "reset":
        return cmd_fixtures_reset(args)
    else:
        logger.error("Unknown fixtures subcommand: %s", subcommand)
        return 1


def cmd_fixtures_load(args: argparse.Namespace) -> int:
    """Load fixtures from CSV or JSON file."""
    from .fixtures import FixtureLoader

    db = get_pg_db()

    try:
        loader = FixtureLoader(db)
        file_path = Path(args.file)

        if not file_path.exists():
            logger.error("File not found: %s", file_path)
            return 1

        # Determine format from extension
        if file_path.suffix.lower() == ".csv":
            result = loader.load_from_csv(
                file_path,
                sport=args.sport,
                season_year=args.season,
                seed_delay_hours=args.delay or 4,
                clear_existing=args.clear,
            )
        elif file_path.suffix.lower() == ".json":
            result = loader.load_from_json(
                file_path,
                sport=args.sport,
                season_year=args.season,
                seed_delay_hours=args.delay or 4,
                clear_existing=args.clear,
            )
        else:
            logger.error("Unsupported file format. Use .csv or .json")
            return 1

        print(f"\nFixtures Loaded")
        print("=" * 50)
        print(f"  Loaded:  {result['loaded']}")
        print(f"  Skipped: {result['skipped']}")
        print(f"  Errors:  {result['errors']}")

        return 0

    except Exception as e:
        logger.error("Failed to load fixtures: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_status(args: argparse.Namespace) -> int:
    """Show fixture status summary."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None)  # No API needed for status
        summary = scheduler.get_fixture_status_summary(args.sport)

        print(f"\nFixture Status Summary")
        if args.sport:
            print(f"Sport: {args.sport}")
        print("=" * 50)

        total = 0
        for status, count in sorted(summary.items()):
            print(f"  {status:<15} {count:>6,}")
            total += count

        print("-" * 50)
        print(f"  {'Total':<15} {total:>6,}")

        return 0

    except Exception as e:
        logger.error("Failed to get fixture status: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_pending(args: argparse.Namespace) -> int:
    """Show pending fixtures ready to seed."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None, max_fixtures_per_run=args.limit or 50)
        pending = scheduler._get_pending_fixtures(args.sport)

        print(f"\nPending Fixtures (ready to seed)")
        if args.sport:
            print(f"Sport: {args.sport}")
        print("=" * 70)

        if not pending:
            print("  No pending fixtures")
            return 0

        print(f"  {'ID':<8} {'Sport':<10} {'Start Time':<20} {'Attempts':<10}")
        print("-" * 70)

        for f in pending:
            start_time = (
                f["start_time"].strftime("%Y-%m-%d %H:%M") if f["start_time"] else "N/A"
            )
            print(
                f"  {f['id']:<8} {f['sport']:<10} {start_time:<20} {f['seed_attempts']:<10}"
            )

        print(f"\nTotal: {len(pending)} fixtures ready to seed")

        return 0

    except Exception as e:
        logger.error("Failed to get pending fixtures: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_upcoming(args: argparse.Namespace) -> int:
    """Show upcoming fixtures."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None)
        upcoming = scheduler.get_upcoming_fixtures(
            sport_id=args.sport,
            hours_ahead=args.hours or 24,
            limit=args.limit or 50,
        )

        print(f"\nUpcoming Fixtures (next {args.hours or 24} hours)")
        if args.sport:
            print(f"Sport: {args.sport}")
        print("=" * 80)

        if not upcoming:
            print("  No upcoming fixtures")
            return 0

        print(f"  {'ID':<8} {'Sport':<10} {'Start Time':<20} {'Home':<15} {'Away':<15}")
        print("-" * 80)

        for f in upcoming:
            start_time = (
                f["start_time"].strftime("%Y-%m-%d %H:%M") if f["start_time"] else "N/A"
            )
            home = f.get("home_team_name", "N/A")[:14]
            away = f.get("away_team_name", "N/A")[:14]
            print(
                f"  {f['id']:<8} {f['sport']:<10} {start_time:<20} {home:<15} {away:<15}"
            )

        print(f"\nTotal: {len(upcoming)} upcoming fixtures")

        return 0

    except Exception as e:
        logger.error("Failed to get upcoming fixtures: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_recent(args: argparse.Namespace) -> int:
    """Show recently seeded fixtures."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None)
        recent = scheduler.get_recent_seeds(
            sport_id=args.sport,
            hours_back=args.hours or 24,
            limit=args.limit or 50,
        )

        print(f"\nRecently Seeded Fixtures (past {args.hours or 24} hours)")
        if args.sport:
            print(f"Sport: {args.sport}")
        print("=" * 80)

        if not recent:
            print("  No recently seeded fixtures")
            return 0

        print(f"  {'ID':<8} {'Sport':<10} {'Seeded At':<20} {'Home':<15} {'Away':<15}")
        print("-" * 80)

        for f in recent:
            seeded_at = (
                f["seeded_at"].strftime("%Y-%m-%d %H:%M") if f["seeded_at"] else "N/A"
            )
            home = f.get("home_team_name", "N/A")[:14]
            away = f.get("away_team_name", "N/A")[:14]
            print(
                f"  {f['id']:<8} {f['sport']:<10} {seeded_at:<20} {home:<15} {away:<15}"
            )

        print(f"\nTotal: {len(recent)} recently seeded fixtures")

        return 0

    except Exception as e:
        logger.error("Failed to get recent fixtures: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_failed(args: argparse.Namespace) -> int:
    """Show fixtures that failed seeding."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None)
        failed = scheduler.get_failed_fixtures(
            sport_id=args.sport,
            limit=args.limit or 50,
        )

        print(f"\nFailed Fixtures")
        if args.sport:
            print(f"Sport: {args.sport}")
        print("=" * 90)

        if not failed:
            print("  No failed fixtures")
            return 0

        print(f"  {'ID':<8} {'Sport':<10} {'Attempts':<10} {'Error':<50}")
        print("-" * 90)

        for f in failed:
            error = (f.get("last_seed_error") or "Unknown")[:48]
            print(
                f"  {f['id']:<8} {f['sport']:<10} {f['seed_attempts']:<10} {error:<50}"
            )

        print(f"\nTotal: {len(failed)} failed fixtures")
        print("\nUse 'fixtures reset <fixture_id>' to reset a fixture for retry")

        return 0

    except Exception as e:
        logger.error("Failed to get failed fixtures: %s", e)
        return 1
    finally:
        db.close()


async def cmd_fixtures_seed_async(args: argparse.Namespace) -> int:
    """Manually seed a specific fixture."""
    from .fixtures import PostMatchSeeder

    db = get_pg_db()

    # Determine sport from fixture to create appropriate provider client
    fixture = db.fetchone(
        "SELECT sport FROM fixtures WHERE id = %s", (args.fixture_id,)
    )
    sport_id = fixture["sport"] if fixture else "NBA"
    client = _get_provider_client(sport_id)

    try:
        seeder = PostMatchSeeder(db, client)
        fixture_id = args.fixture_id

        logger.info("Seeding fixture %d...", fixture_id)
        result = await seeder.seed_fixture(
            fixture_id,
            recalculate_percentiles=not args.skip_percentiles,
        )

        print(f"\nFixture Seeding Result")
        print("=" * 50)
        print(f"  Fixture ID:    {result.fixture_id}")
        print(f"  Sport:         {result.sport}")
        print(f"  Success:       {result.success}")
        print(f"  Players:       {result.players_updated}")
        print(f"  Teams:         {result.teams_updated}")
        print(f"  Percentiles:   {'Yes' if result.percentiles_recalculated else 'No'}")
        print(f"  Duration:      {result.duration_seconds:.1f}s")

        if result.error:
            print(f"  Error:         {result.error}")

        return 0 if result.success else 1

    except Exception as e:
        logger.error("Failed to seed fixture: %s", e)
        return 1
    finally:
        db.close()


async def cmd_fixtures_run_scheduler_async(args: argparse.Namespace) -> int:
    """Run the scheduler to process pending fixtures."""
    from .fixtures import SchedulerService

    db = get_pg_db()
    # Scheduler creates per-fixture provider clients as needed
    sport_id = (args.sport or "NBA").upper()
    client = _get_provider_client(sport_id)

    try:
        scheduler = SchedulerService(
            db,
            client,
            max_fixtures_per_run=args.max or 10,
            max_retries=args.max_retries or 3,
        )

        logger.info("Running fixture scheduler...")
        result = await scheduler.process_pending_fixtures(
            sport_id=args.sport,
            recalculate_percentiles=not args.skip_percentiles,
        )

        print(f"\nScheduler Run Summary")
        print("=" * 50)
        print(f"  Fixtures Found:     {result.fixtures_found}")
        print(f"  Fixtures Processed: {result.fixtures_processed}")
        print(f"  Succeeded:          {result.fixtures_succeeded}")
        print(f"  Failed:             {result.fixtures_failed}")
        print(f"  Players Updated:    {result.total_players_updated}")
        print(f"  Teams Updated:      {result.total_teams_updated}")
        print(f"  Duration:           {result.total_duration_seconds:.1f}s")

        if result.errors:
            print(f"\nErrors:")
            for err in result.errors[:10]:
                print(f"  - {err}")
            if len(result.errors) > 10:
                print(f"  ... and {len(result.errors) - 10} more")

        return 0 if result.fixtures_failed == 0 else 1

    except Exception as e:
        logger.error("Scheduler run failed: %s", e)
        return 1
    finally:
        db.close()


def cmd_fixtures_reset(args: argparse.Namespace) -> int:
    """Reset a fixture to allow retry."""
    from .fixtures import SchedulerService

    db = get_pg_db()

    try:
        scheduler = SchedulerService(db, None)
        fixture_id = args.fixture_id

        if scheduler.reset_fixture_for_retry(fixture_id):
            print(f"Fixture {fixture_id} has been reset for retry")
            return 0
        else:
            print(f"Fixture {fixture_id} not found")
            return 1

    except Exception as e:
        logger.error("Failed to reset fixture: %s", e)
        return 1
    finally:
        db.close()


def main() -> int:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Stats Database CLI",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # init command
    init_parser = subparsers.add_parser("init", help="Initialize the database")

    # status command
    status_parser = subparsers.add_parser("status", help="Show database status")

    # seed command (uses BallDontLie / SportMonks providers)
    seed_parser = subparsers.add_parser(
        "seed",
        help="Seed data from provider APIs (BallDontLie for NBA/NFL, SportMonks for Football)",
    )
    seed_parser.add_argument("--sport", help="Sport to seed (NBA, NFL, FOOTBALL)")
    seed_parser.add_argument("--all", action="store_true", help="Seed all sports")
    seed_parser.add_argument("--season", type=int, help="Season year to seed")

    # percentiles command with subcommands
    pct_parser = subparsers.add_parser(
        "percentiles",
        help="Recalculate or archive percentiles",
    )
    pct_subparsers = pct_parser.add_subparsers(
        dest="percentiles_command",
        help="Percentile subcommands",
    )

    # percentiles recalc (default behavior)
    pct_recalc = pct_subparsers.add_parser(
        "recalc",
        help="Recalculate percentiles for a sport/season",
    )
    pct_recalc.add_argument("--sport", help="Sport (default: all)")
    pct_recalc.add_argument("--season", type=int, help="Season year")

    # percentiles archive (end of season)
    pct_archive = pct_subparsers.add_parser(
        "archive",
        help="Archive current percentiles for historical preservation",
    )
    pct_archive.add_argument("--sport", help="Sport (default: all)")
    pct_archive.add_argument("--season", type=int, help="Season year")
    pct_archive.add_argument(
        "--final",
        action="store_true",
        help="Mark as final end-of-season snapshot",
    )

    # Keep backwards compatibility: percentiles --sport --season
    pct_parser.add_argument("--sport", help="Sport (default: all)")
    pct_parser.add_argument("--season", type=int, help="Season year")

    # export command
    export_parser = subparsers.add_parser("export", help="Export data to files")
    export_parser.add_argument("--format", choices=["json"], default="json")
    export_parser.add_argument("--output", help="Output directory")
    export_parser.add_argument("--sport", help="Sport to export")
    export_parser.add_argument("--season", type=int, help="Season year")

    # query command
    query_parser = subparsers.add_parser("query", help="Run queries")
    query_parser.add_argument("type", choices=["leaders", "standings", "profile"])
    query_parser.add_argument("--sport", help="Sport")
    query_parser.add_argument("--season", type=int, help="Season year")
    query_parser.add_argument("--stat", help="Stat name for leaders query")
    query_parser.add_argument("--limit", type=int, help="Result limit")
    query_parser.add_argument("--league", type=int, help="League ID filter")
    query_parser.add_argument("--conference", help="Conference filter")
    query_parser.add_argument("--entity-type", choices=["player", "team"])
    query_parser.add_argument("--entity-id", type=int, help="Entity ID for profile")

    # fixtures command (schedule-driven seeding)
    fixtures_parser = subparsers.add_parser(
        "fixtures",
        help="Manage match fixtures and schedule-driven seeding",
    )
    fixtures_subparsers = fixtures_parser.add_subparsers(
        dest="fixtures_command",
        help="Fixture subcommands",
    )

    # fixtures load
    fixtures_load_parser = fixtures_subparsers.add_parser(
        "load",
        help="Load fixtures from CSV or JSON file",
    )
    fixtures_load_parser.add_argument("file", help="Path to CSV or JSON file")
    fixtures_load_parser.add_argument("--sport", help="Default sport ID if not in file")
    fixtures_load_parser.add_argument("--season", type=int, help="Season year")
    fixtures_load_parser.add_argument(
        "--delay", type=int, default=4, help="Hours after match to seed (default: 4)"
    )
    fixtures_load_parser.add_argument(
        "--clear", action="store_true", help="Clear existing fixtures first"
    )

    # fixtures status
    fixtures_status_parser = fixtures_subparsers.add_parser(
        "status",
        help="Show fixture status summary",
    )
    fixtures_status_parser.add_argument("--sport", help="Filter by sport")

    # fixtures pending
    fixtures_pending_parser = fixtures_subparsers.add_parser(
        "pending",
        help="Show fixtures ready to seed",
    )
    fixtures_pending_parser.add_argument("--sport", help="Filter by sport")
    fixtures_pending_parser.add_argument(
        "--limit", type=int, default=50, help="Max fixtures to show"
    )

    # fixtures upcoming
    fixtures_upcoming_parser = fixtures_subparsers.add_parser(
        "upcoming",
        help="Show upcoming fixtures",
    )
    fixtures_upcoming_parser.add_argument("--sport", help="Filter by sport")
    fixtures_upcoming_parser.add_argument(
        "--hours", type=int, default=24, help="Hours ahead to look"
    )
    fixtures_upcoming_parser.add_argument(
        "--limit", type=int, default=50, help="Max fixtures to show"
    )

    # fixtures recent
    fixtures_recent_parser = fixtures_subparsers.add_parser(
        "recent",
        help="Show recently seeded fixtures",
    )
    fixtures_recent_parser.add_argument("--sport", help="Filter by sport")
    fixtures_recent_parser.add_argument(
        "--hours", type=int, default=24, help="Hours back to look"
    )
    fixtures_recent_parser.add_argument(
        "--limit", type=int, default=50, help="Max fixtures to show"
    )

    # fixtures failed
    fixtures_failed_parser = fixtures_subparsers.add_parser(
        "failed",
        help="Show fixtures that failed seeding",
    )
    fixtures_failed_parser.add_argument("--sport", help="Filter by sport")
    fixtures_failed_parser.add_argument(
        "--limit", type=int, default=50, help="Max fixtures to show"
    )

    # fixtures seed
    fixtures_seed_parser = fixtures_subparsers.add_parser(
        "seed",
        help="Manually seed a specific fixture",
    )
    fixtures_seed_parser.add_argument("fixture_id", type=int, help="Fixture ID to seed")
    fixtures_seed_parser.add_argument(
        "--skip-percentiles",
        action="store_true",
        help="Skip percentile recalculation",
    )

    # fixtures run-scheduler
    fixtures_scheduler_parser = fixtures_subparsers.add_parser(
        "run-scheduler",
        help="Run scheduler to process pending fixtures",
    )
    fixtures_scheduler_parser.add_argument("--sport", help="Filter by sport")
    fixtures_scheduler_parser.add_argument(
        "--max", type=int, default=10, help="Max fixtures per run"
    )
    fixtures_scheduler_parser.add_argument(
        "--max-retries", type=int, default=3, help="Skip fixtures with more failures"
    )
    fixtures_scheduler_parser.add_argument(
        "--skip-percentiles",
        action="store_true",
        help="Skip percentile recalculation",
    )

    # fixtures reset
    fixtures_reset_parser = fixtures_subparsers.add_parser(
        "reset",
        help="Reset a fixture for retry",
    )
    fixtures_reset_parser.add_argument(
        "fixture_id", type=int, help="Fixture ID to reset"
    )

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return 0

    commands = {
        "init": cmd_init,
        "status": cmd_status,
        "seed": cmd_seed,
        "percentiles": cmd_percentiles,
        "export": cmd_export,
        "query": cmd_query,
        "fixtures": cmd_fixtures,
    }

    cmd_func = commands.get(args.command)
    if cmd_func:
        return cmd_func(args)
    else:
        parser.print_help()
        return 1


if __name__ == "__main__":
    sys.exit(main())
