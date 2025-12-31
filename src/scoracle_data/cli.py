#!/usr/bin/env python3
"""
Command-line interface for stats database management.

Usage:
    python -m backend.app.statsdb.cli init           # Initialize database
    python -m backend.app.statsdb.cli seed --sport NBA --season 2025
    python -m backend.app.statsdb.cli seed --all
    python -m backend.app.statsdb.cli seed-debug --sport NBA --season 2025  # Debug: 5 teams, 5 players
    python -m backend.app.statsdb.cli seed-2phase --sport FOOTBALL --season 2025
    python -m backend.app.statsdb.cli diff --sport FOOTBALL --season 2025
    python -m backend.app.statsdb.cli percentiles --sport NBA --season 2025
    python -m backend.app.statsdb.cli status
    python -m backend.app.statsdb.cli export --format json
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("statsdb.cli")


def get_db():
    """Get database connection in write mode."""
    from .connection import StatsDB, DEFAULT_DB_PATH

    return StatsDB(db_path=DEFAULT_DB_PATH, read_only=False)


async def get_api_service():
    """Get API-Sports service instance.

    Uses the api_client abstraction which:
    - In Scoracle: Uses app.services.apisports
    - Standalone: Uses built-in httpx client
    """
    from .api_client import get_api_client

    return get_api_client()


def cmd_seed_small(args: argparse.Namespace) -> int:
    """Seed a tiny JSON fixture (one team/player per sport) for quick DB validation."""
    from .seeders.small_dataset_seeder import seed_small_dataset

    result = seed_small_dataset(fixture_path=args.fixture)
    summary = result.get("summary", {})

    # Best-effort backend reporting
    backend = "postgres" if (os.getenv("DATABASE_URL") or os.getenv("NEON_DATABASE_URL")) else "sqlite"

    print("\nSmall dataset seeding complete")
    print("=" * 50)
    print(f"Backend: {backend}")
    if backend == "sqlite":
        from .connection import DEFAULT_DB_PATH
        print(f"SQLite path: {DEFAULT_DB_PATH}")
    else:
        print("Postgres URL: DATABASE_URL/NEON_DATABASE_URL")
    print(f"Teams upserted: {summary.get('teams', 0)}")
    print(f"Players upserted: {summary.get('players', 0)}")

    return 0


def cmd_init(args: argparse.Namespace) -> int:
    """Initialize the database with schema."""
    from .schema import init_database

    db = get_db()

    try:
        logger.info("Initializing stats database...")
        init_database(db)
        logger.info("Database initialized successfully at %s", db.db_path)
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
        if not db.exists():
            logger.info("Database does not exist at %s", db.db_path)
            return 1

        if not db.is_initialized():
            logger.info("Database exists but is not initialized")
            return 1

        version = get_schema_version(db)
        counts = get_table_counts(db)

        print(f"\nStats Database Status")
        print(f"=" * 50)
        print(f"Location: {db.db_path}")
        print(f"Schema Version: {version}")
        print(f"Last Full Sync: {db.get_meta('last_full_sync') or 'Never'}")
        print(f"Last Incremental: {db.get_meta('last_incremental_sync') or 'Never'}")
        print()
        print("Table Counts:")
        for table, count in sorted(counts.items()):
            if not table.startswith("sqlite_"):
                print(f"  {table}: {count:,}")

        # Show sports summary
        print()
        print("Sports Summary:")
        sports = db.fetchall("SELECT * FROM sports WHERE is_active = 1")
        for sport in sports:
            players = db.fetchone(
                "SELECT COUNT(*) as count FROM players WHERE sport_id = ?",
                (sport["id"],),
            )
            teams = db.fetchone(
                "SELECT COUNT(*) as count FROM teams WHERE sport_id = ?",
                (sport["id"],),
            )
            print(f"  {sport['id']}: {players['count']:,} players, {teams['count']:,} teams")

        return 0

    except Exception as e:
        logger.error("Failed to get status: %s", e)
        return 1
    finally:
        db.close()


async def cmd_seed_async(args: argparse.Namespace) -> int:
    """Seed data from API-Sports."""
    from .seeders import NBASeeder, NFLSeeder, FootballSeeder

    db = get_db()
    api = await get_api_service()

    # Initialize database if needed
    if not db.is_initialized():
        from .schema import init_database
        logger.info("Database not initialized, running initialization...")
        init_database(db)

    sports_to_seed = []
    if args.all:
        sports_to_seed = ["NBA", "NFL", "FOOTBALL"]
    elif args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        logger.error("Must specify --sport or --all")
        return 1

    # Determine seasons to seed
    seasons = [args.season] if args.season else [2025]
    current_season = args.current_season or seasons[-1]

    seeder_map = {
        "NBA": NBASeeder,
        "NFL": NFLSeeder,
        "FOOTBALL": FootballSeeder,
    }

    total_summary = {"teams": 0, "players": 0, "player_stats": 0, "team_stats": 0}

    for sport_id in sports_to_seed:
        seeder_class = seeder_map.get(sport_id)
        if not seeder_class:
            logger.warning("Unknown sport: %s", sport_id)
            continue

        logger.info("Seeding %s data for seasons %s...", sport_id, seasons)
        seeder = seeder_class(db, api)

        try:
            summary = await seeder.seed_all(seasons, current_season)
            for key in total_summary:
                total_summary[key] += summary.get(key, 0)
            logger.info(
                "%s seeding complete: %d teams, %d players, %d player stats, %d team stats",
                sport_id,
                summary["teams"],
                summary["players"],
                summary["player_stats"],
                summary["team_stats"],
            )
        except Exception as e:
            logger.error("Failed to seed %s: %s", sport_id, e)

    # Update metadata
    db.set_meta("last_full_sync", datetime.utcnow().isoformat())

    logger.info(
        "Total seeded: %d teams, %d players, %d player stats, %d team stats",
        total_summary["teams"],
        total_summary["players"],
        total_summary["player_stats"],
        total_summary["team_stats"],
    )

    db.close()
    return 0


def cmd_seed(args: argparse.Namespace) -> int:
    """Wrapper to run async seed command."""
    return asyncio.run(cmd_seed_async(args))


async def cmd_seed_2phase_async(args: argparse.Namespace) -> int:
    """Run two-phase seeding (discovery -> profile fetch -> stats)."""
    from .seeders import NBASeeder, NFLSeeder, FootballSeeder

    db = get_db()
    api = await get_api_service()

    # Initialize database if needed
    if not db.is_initialized():
        from .schema import init_database
        logger.info("Database not initialized, running initialization...")
        init_database(db)

    sports_to_seed = []
    if args.all:
        sports_to_seed = ["NBA", "NFL", "FOOTBALL"]
    elif args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        logger.error("Must specify --sport or --all")
        return 1

    season = args.season or 2025

    seeder_map = {
        "NBA": NBASeeder,
        "NFL": NFLSeeder,
        "FOOTBALL": FootballSeeder,
    }

    for sport_id in sports_to_seed:
        seeder_class = seeder_map.get(sport_id)
        if not seeder_class:
            logger.warning("Unknown sport: %s", sport_id)
            continue

        logger.info("Running two-phase seeding for %s season %d...", sport_id, season)
        seeder = seeder_class(db, api)

        try:
            if sport_id == "FOOTBALL":
                # For Football, seed each priority league
                from .seeders.football_seeder import PRIORITY_LEAGUES

                for league in PRIORITY_LEAGUES:
                    logger.info("Seeding Football league: %s (ID: %d)", league["name"], league["id"])
                    result = await seeder.seed_two_phase(
                        season,
                        league_id=league["id"],
                        skip_profiles=args.skip_profiles,
                    )
                    _log_two_phase_result(sport_id, league["id"], result)
            else:
                result = await seeder.seed_two_phase(
                    season,
                    skip_profiles=args.skip_profiles,
                )
                _log_two_phase_result(sport_id, None, result)

        except Exception as e:
            logger.error("Failed to seed %s: %s", sport_id, e)
            import traceback
            traceback.print_exc()

    # Update metadata
    db.set_meta("last_full_sync", datetime.utcnow().isoformat())
    db.close()
    return 0


def _log_two_phase_result(sport_id: str, league_id: Optional[int], result: dict) -> None:
    """Log two-phase seeding result."""
    discovery = result.get("discovery", {})
    league_str = f" (league {league_id})" if league_id else ""
    logger.info(
        "%s%s: Discovered %d teams (%d new), %d players (%d new, %d transfers)",
        sport_id,
        league_str,
        discovery.get("teams_discovered", 0),
        discovery.get("teams_new", 0),
        discovery.get("players_discovered", 0),
        discovery.get("players_new", 0),
        discovery.get("players_transferred", 0),
    )
    logger.info(
        "%s%s: Fetched %d profiles, seeded %d player stats, %d team stats",
        sport_id,
        league_str,
        result.get("profiles_fetched", 0),
        result.get("player_stats", 0),
        result.get("team_stats", 0),
    )


def cmd_seed_2phase(args: argparse.Namespace) -> int:
    """Wrapper to run async two-phase seed command."""
    return asyncio.run(cmd_seed_2phase_async(args))


async def cmd_seed_debug_async(args: argparse.Namespace) -> int:
    """Seed limited data for debugging (5 teams, 5 players per sport)."""
    from .seeders import NBASeeder, NFLSeeder, FootballSeeder
    from .seeders.football_seeder import PRIORITY_LEAGUES

    db = get_db()
    api = await get_api_service()

    # Initialize database if needed
    if not db.is_initialized():
        from .schema import init_database
        logger.info("Database not initialized, running initialization...")
        init_database(db)

    sports_to_seed = []
    if args.all:
        sports_to_seed = ["NBA", "NFL", "FOOTBALL"]
    elif args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        logger.error("Must specify --sport or --all")
        return 1

    season = args.season or 2025
    num_teams = args.teams or 5
    num_players = args.players or 5

    seeder_map = {
        "NBA": NBASeeder,
        "NFL": NFLSeeder,
        "FOOTBALL": FootballSeeder,
    }

    for sport_id in sports_to_seed:
        seeder_class = seeder_map.get(sport_id)
        if not seeder_class:
            logger.warning("Unknown sport: %s", sport_id)
            continue

        logger.info("Debug seeding %s: %d teams, %d players...", sport_id, num_teams, num_players)
        seeder = seeder_class(db, api)

        try:
            # Get league_id for football
            league_id = None
            if sport_id == "FOOTBALL":
                league_id = PRIORITY_LEAGUES[0]["id"]  # Premier League
                logger.info("Using league: %s (ID: %d)", PRIORITY_LEAGUES[0]["name"], league_id)

            # Fetch teams (limited)
            all_teams = await seeder.fetch_teams(season, league_id)
            teams = all_teams[:num_teams]
            logger.info("Fetched %d/%d teams", len(teams), len(all_teams))

            # Seed teams
            season_id = seeder.ensure_season(season)
            for team in teams:
                seeder.upsert_team(team)

            # Fetch players from first few teams only
            all_players = []
            for team in teams[:3]:  # Only fetch from first 3 teams
                if sport_id == "NBA":
                    players = await seeder.api.list_players("NBA", season=str(season), team_id=team["id"])
                elif sport_id == "NFL":
                    players = await seeder.api.list_players("NFL", season=str(season), team_id=team["id"])
                else:
                    players = await seeder.api.list_players("FOOTBALL", season=str(season), page=1, league=league_id)
                all_players.extend(players[:5])  # Take 5 from each team

            players = all_players[:num_players]
            logger.info("Fetched %d players", len(players))

            # Seed players
            for player in players:
                player_data = {
                    "id": player.get("id"),
                    "first_name": player.get("first_name") or player.get("firstname"),
                    "last_name": player.get("last_name") or player.get("lastname"),
                    "full_name": player.get("name") or f"{player.get('first_name', '')} {player.get('last_name', '')}".strip(),
                    "position": player.get("position"),
                    "current_team_id": teams[0]["id"] if teams else None,
                }
                seeder.upsert_player(player_data, season)

            # Fetch and seed player stats
            player_stats_count = 0
            for player in players:
                # Football requires league_id for player stats
                if sport_id == "FOOTBALL":
                    stats = await seeder.fetch_player_stats(player["id"], season, league_id=league_id)
                else:
                    stats = await seeder.fetch_player_stats(player["id"], season)
                if stats:
                    try:
                        transformed = seeder.transform_player_stats(
                            stats, player["id"], season_id, teams[0]["id"] if teams else None
                        )
                        seeder.upsert_player_stats(transformed)
                        player_stats_count += 1
                        logger.info("  Player %d: %s - stats seeded", player["id"], player.get("name", "Unknown"))
                    except Exception as e:
                        logger.warning("  Player %d: failed to transform stats: %s", player["id"], e)

            # Fetch and seed team stats
            team_stats_count = 0
            for team in teams:
                # Football requires league_id for team stats
                # NFL also requires league_id=1 for standings endpoint
                if sport_id == "FOOTBALL":
                    stats = await seeder.fetch_team_stats(team["id"], season, league_id=league_id)
                elif sport_id == "NFL":
                    stats = await seeder.fetch_team_stats(team["id"], season, league_id=1)
                else:
                    stats = await seeder.fetch_team_stats(team["id"], season)
                if stats:
                    try:
                        transformed = seeder.transform_team_stats(stats, team["id"], season_id)
                        seeder.upsert_team_stats(transformed)
                        team_stats_count += 1
                        logger.info("  Team %d: %s - stats seeded", team["id"], team.get("name", "Unknown"))
                    except Exception as e:
                        logger.warning("  Team %d: failed to transform stats: %s", team["id"], e)

            logger.info(
                "%s debug complete: %d teams, %d players, %d player stats, %d team stats",
                sport_id, len(teams), len(players), player_stats_count, team_stats_count
            )

        except Exception as e:
            logger.error("Failed to seed %s: %s", sport_id, e)
            import traceback
            traceback.print_exc()

    db.set_meta("last_debug_sync", datetime.utcnow().isoformat())
    db.close()
    return 0


def cmd_seed_debug(args: argparse.Namespace) -> int:
    """Wrapper to run async debug seed command."""
    return asyncio.run(cmd_seed_debug_async(args))


async def cmd_diff_async(args: argparse.Namespace) -> int:
    """Run roster diff to detect trades/transfers."""
    from .roster_diff import RosterDiffEngine

    db = get_db()
    api = await get_api_service()

    if not db.is_initialized():
        logger.error("Database not initialized. Run 'init' first.")
        return 1

    engine = RosterDiffEngine(db, api)
    season = args.season or 2025

    try:
        if args.all:
            # Run all priority diffs
            results = await engine.run_all_priority_diffs(season)
            total_new = sum(len(r.new_players) for r in results)
            total_transfers = sum(len(r.transferred_players) for r in results)
            logger.info(
                "All diffs complete: %d new players, %d transfers across %d leagues/sports",
                total_new,
                total_transfers,
                len(results),
            )
        else:
            # Single sport/league diff
            if not args.sport:
                logger.error("Must specify --sport or --all")
                return 1

            sport_id = args.sport.upper()
            league_id = args.league

            if sport_id == "FOOTBALL" and not league_id:
                logger.error("FOOTBALL requires --league parameter")
                return 1

            result = await engine.run_diff(sport_id, season, league_id)
            logger.info("Diff result: %s", result.to_dict())

            if result.new_players:
                logger.info("New players needing profile fetch: %s", result.new_players[:10])
                if len(result.new_players) > 10:
                    logger.info("... and %d more", len(result.new_players) - 10)

            if result.transferred_players:
                for player_id, from_team, to_team in result.transferred_players[:10]:
                    logger.info("Transfer: player %d from team %d to team %d", player_id, from_team, to_team)

    except Exception as e:
        logger.error("Diff failed: %s", e)
        import traceback
        traceback.print_exc()
        return 1
    finally:
        db.close()

    return 0


def cmd_diff(args: argparse.Namespace) -> int:
    """Wrapper to run async diff command."""
    return asyncio.run(cmd_diff_async(args))


def cmd_percentiles(args: argparse.Namespace) -> int:
    """Recalculate percentiles."""
    from .percentiles import PercentileCalculator

    db = get_db()

    if not db.is_initialized():
        logger.error("Database not initialized. Run 'init' first.")
        return 1

    calculator = PercentileCalculator(db)

    sports = [args.sport.upper()] if args.sport else ["NBA", "NFL", "FOOTBALL"]
    season = args.season or 2025

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

    sports = [sport] if sport else ["NBA", "NFL", "FOOTBALL"]

    for sport_id in sports:
        season_id = db.get_season_id(sport_id, season)
        if not season_id:
            logger.warning("No data for %s %d", sport_id, season)
            continue

        # Export players with stats
        players = db.fetchall(
            "SELECT * FROM players WHERE sport_id = ?",
            (sport_id,),
        )

        player_data = []
        for player in players:
            stats = db.get_player_stats(player["id"], sport_id, season)
            percentiles = db.get_percentiles("player", player["id"], sport_id, season)
            player_data.append({
                "player": dict(player),
                "stats": stats,
                "percentiles": percentiles,
            })

        # Export teams with stats
        teams = db.fetchall(
            "SELECT * FROM teams WHERE sport_id = ?",
            (sport_id,),
        )

        team_data = []
        for team in teams:
            stats = db.get_team_stats(team["id"], sport_id, season)
            percentiles = db.get_percentiles("team", team["id"], sport_id, season)
            team_data.append({
                "team": dict(team),
                "stats": stats,
                "percentiles": percentiles,
            })

        export_data = {
            "sport": sport_id,
            "season": season,
            "exported_at": datetime.utcnow().isoformat(),
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
            print(f"{r['rank']:3}. {r['full_name']:<25} {r['stat_value']:>10.1f}  ({r['team_name'] or 'N/A'})")

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
                print(f"{r['rank']:3}. {r['name']:<25} {r['points']:3} pts  ({r['wins']}-{r['draws']}-{r['losses']})")
            else:
                print(f"{r['rank']:3}. {r['name']:<25} {r['win_pct']:.3f}  ({r['wins']}-{r['losses']})")

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

    # seed command (legacy full sync)
    seed_parser = subparsers.add_parser("seed", help="Seed data from API-Sports (full sync)")
    seed_parser.add_argument("--sport", help="Sport to seed (NBA, NFL, FOOTBALL)")
    seed_parser.add_argument("--all", action="store_true", help="Seed all sports")
    seed_parser.add_argument("--season", type=int, help="Season year to seed")
    seed_parser.add_argument("--current-season", type=int, help="Mark this season as current")

    # seed-2phase command (recommended)
    seed2_parser = subparsers.add_parser(
        "seed-2phase",
        help="Two-phase seeding (discovery -> profiles -> stats)",
    )
    seed2_parser.add_argument("--sport", help="Sport to seed (NBA, NFL, FOOTBALL)")
    seed2_parser.add_argument("--all", action="store_true", help="Seed all sports")
    seed2_parser.add_argument("--season", type=int, help="Season year to seed")
    seed2_parser.add_argument(
        "--skip-profiles",
        action="store_true",
        help="Skip profile fetch phase (use if profiles already fetched)",
    )

    # seed-debug command (for development)
    seed_debug_parser = subparsers.add_parser(
        "seed-debug",
        help="Debug seeding with limited entities (5 teams, 5 players per sport)",
    )
    seed_debug_parser.add_argument("--sport", help="Sport to seed (NBA, NFL, FOOTBALL)")
    seed_debug_parser.add_argument("--all", action="store_true", help="Seed all sports")
    seed_debug_parser.add_argument("--season", type=int, help="Season year to seed")
    seed_debug_parser.add_argument("--teams", type=int, default=5, help="Number of teams to seed (default: 5)")
    seed_debug_parser.add_argument("--players", type=int, default=5, help="Number of players to seed (default: 5)")

    # seed-small command (fixture seeding)
    seed_small_parser = subparsers.add_parser(
        "seed-small",
        help="Seed the small dataset JSON fixture (no external API calls)",
    )
    seed_small_parser.add_argument(
        "--fixture",
        help="Optional path to fixture JSON (default: tests/fixtures/small_dataset.json)",
    )

    # diff command
    diff_parser = subparsers.add_parser("diff", help="Run roster diff (detect trades/transfers)")
    diff_parser.add_argument("--sport", help="Sport (NBA, NFL, FOOTBALL)")
    diff_parser.add_argument("--all", action="store_true", help="Run diff for all priority leagues")
    diff_parser.add_argument("--season", type=int, help="Season year")
    diff_parser.add_argument("--league", type=int, help="League ID (required for FOOTBALL)")

    # percentiles command
    pct_parser = subparsers.add_parser("percentiles", help="Recalculate percentiles")
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

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return 0

    commands = {
        "init": cmd_init,
        "status": cmd_status,
        "seed": cmd_seed,
        "seed-2phase": cmd_seed_2phase,
        "seed-debug": cmd_seed_debug,
        "seed-small": cmd_seed_small,
        "diff": cmd_diff,
        "percentiles": cmd_percentiles,
        "export": cmd_export,
        "query": cmd_query,
    }

    cmd_func = commands.get(args.command)
    if cmd_func:
        return cmd_func(args)
    else:
        parser.print_help()
        return 1


if __name__ == "__main__":
    sys.exit(main())
