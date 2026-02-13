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
    python -m scoracle_data.cli export-profiles
    python -m scoracle_data.cli export-profiles --sport FOOTBALL --season 2025

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

from typing import TYPE_CHECKING

from .core.types import (
    Sport,
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
)

if TYPE_CHECKING:
    from .seeders.common import SeedResult

ALL_SPORTS = [s.value for s in Sport]


def get_db():
    """Get the singleton database connection."""
    from .pg_connection import get_db as _get_db

    return _get_db()


def _parse_seasons(seasons_str: str) -> list[int]:
    """Parse a season range or comma-separated list.

    Accepts:
        "2023-2025"  -> [2023, 2024, 2025]
        "2023,2024"  -> [2023, 2024]
        "2025"       -> [2025]
    """
    if "-" in seasons_str and "," not in seasons_str:
        parts = seasons_str.split("-")
        start, end = int(parts[0]), int(parts[1])
        return list(range(start, end + 1))
    elif "," in seasons_str:
        return [int(s.strip()) for s in seasons_str.split(",")]
    else:
        return [int(seasons_str)]


async def _discover_and_store_seasons(
    db, client, league_id: int, sportmonks_league_id: int, target_years: list[int]
) -> int:
    """Discover SportMonks season IDs for a league and store in provider_seasons.

    Returns the number of new mappings stored.
    """
    discovered = await client.discover_season_ids(sportmonks_league_id, target_years)

    stored = 0
    for year, sm_season_id in discovered.items():
        result = db.fetchone(
            """
            INSERT INTO provider_seasons (league_id, season_year, provider, provider_season_id)
            VALUES (%s, %s, 'sportmonks', %s)
            ON CONFLICT (league_id, season_year, provider) DO NOTHING
            RETURNING id
            """,
            (league_id, year, sm_season_id),
        )
        if result:
            stored += 1
            logger.info(
                "Stored provider season: league %d, year %d -> sportmonks season %d",
                league_id,
                year,
                sm_season_id,
            )

    return stored


def _get_handler(sport_id: str):
    """Create an API handler for the given sport.

    Handlers extend BaseApiClient and normalize API responses to
    canonical format. Reads API keys from environment variables:
    - BALLDONTLIE_API_KEY for NBA/NFL
    - SPORTMONKS_API_TOKEN for FOOTBALL
    """
    if sport_id in ("NBA", "NFL"):
        api_key = os.environ.get("BALLDONTLIE_API_KEY")
        if not api_key:
            raise ValueError("BALLDONTLIE_API_KEY environment variable required")
        if sport_id == "NBA":
            from .handlers import BDLNBAHandler

            return BDLNBAHandler(api_key=api_key)
        else:
            from .handlers import BDLNFLHandler

            return BDLNFLHandler(api_key=api_key)
    elif sport_id == "FOOTBALL":
        api_token = os.environ.get("SPORTMONKS_API_TOKEN")
        if not api_token:
            raise ValueError("SPORTMONKS_API_TOKEN environment variable required")
        from .handlers import SportMonksHandler

        return SportMonksHandler(api_token=api_token)
    else:
        raise ValueError(f"Unknown sport: {sport_id}")


def _get_seed_runner(sport_id: str, db, handler):
    """Create a seed runner for the given sport.

    NBA/NFL use BaseSeedRunner directly (identical orchestration).
    Football uses FootballSeedRunner for per-league iteration.
    """
    from .seeders import BaseSeedRunner, FootballSeedRunner

    if sport_id == "FOOTBALL":
        return FootballSeedRunner(db, handler)
    elif sport_id in ("NBA", "NFL"):
        return BaseSeedRunner(db, handler, sport=sport_id)
    else:
        raise ValueError(f"Unknown sport: {sport_id}")


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
            player_count = players["count"] if players else 0
            team_count = teams["count"] if teams else 0
            print(f"  {sport['id']}: {player_count:,} players, {team_count:,} teams")

        return 0

    except Exception as e:
        logger.error("Failed to get status: %s", e)
        return 1
    finally:
        db.close()


async def _seed_sport_season(
    db,
    sport_id: str,
    season: int,
    handler=None,
    leagues: list[dict] | None = None,
) -> "SeedResult":
    """Seed a single sport for a single season. Returns SeedResult.

    For FOOTBALL, iterates the provided leagues list (or falls back to
    Premier League only). For NBA/NFL, runs seed_all().
    """
    from .seeders.common import SeedResult

    result = SeedResult()

    if handler is None:
        handler = _get_handler(sport_id)

    runner = _get_seed_runner(sport_id, db, handler)

    async with handler:
        if sport_id == "FOOTBALL":
            from .seeders.football import FootballSeedRunner as _FB

            assert isinstance(runner, _FB)
            if leagues is None:
                leagues = [{"id": 8, "name": "Premier League"}]

            for league in leagues:
                league_id = league["id"]
                league_name = league["name"]

                # Resolve provider season ID from DB
                result_row = db.fetchone(
                    "SELECT resolve_provider_season_id(%s, %s) AS sid",
                    (league_id, season),
                )
                sm_season_id = result_row["sid"] if result_row else None
                if not sm_season_id:
                    logger.warning(
                        "No provider season ID for %s (league %d) year %d, skipping",
                        league_name,
                        league_id,
                        season,
                    )
                    continue

                logger.info(
                    "Seeding %s %d (season_id=%d)...",
                    league_name,
                    season,
                    sm_season_id,
                )
                league_result = await runner.seed_season(
                    sm_season_id,
                    league_id,
                    season,
                )
                result = result + league_result
        else:
            result = await runner.seed_all(season)

    return result


def _recalculate_percentiles(db, sport_id: str, season: int) -> dict[str, int]:
    """Recalculate percentiles for a sport/season. Returns counts dict."""
    from .percentiles.python_calculator import PythonPercentileCalculator

    calculator = PythonPercentileCalculator(db)
    return calculator.recalculate_all_percentiles(sport_id, season)


async def _cmd_discover_seasons(args: argparse.Namespace, db) -> int:
    """Discover and store SportMonks season IDs for benchmark football leagues.

    Queries the SportMonks API for each benchmark league, resolves season IDs
    for the requested years, and stores them in the provider_seasons table.

    Usage: seed --discover-seasons --seasons 2023-2025
    """
    seasons_str = getattr(args, "seasons", None)
    if not seasons_str:
        logger.error("--discover-seasons requires --seasons (e.g., --seasons 2023-2025)")
        db.close()
        return 1

    target_years = _parse_seasons(seasons_str)

    leagues = db.fetchall(
        "SELECT id, name, sportmonks_id FROM leagues "
        "WHERE sport = 'FOOTBALL' AND is_benchmark = true AND is_active = true "
        "ORDER BY id"
    )

    if not leagues:
        logger.error("No benchmark football leagues found in database")
        db.close()
        return 1

    total_stored = 0
    try:
        handler = _get_handler("FOOTBALL")
        async with handler:
            for league in leagues:
                sm_league_id = league.get("sportmonks_id")
                if not sm_league_id:
                    logger.warning("No sportmonks_id for %s, skipping", league["name"])
                    continue

                logger.info(
                    "Discovering seasons for %s (sportmonks_id=%d)...",
                    league["name"],
                    sm_league_id,
                )

                stored = await _discover_and_store_seasons(
                    db, handler, league["id"], sm_league_id, target_years
                )
                total_stored += stored

    except Exception as e:
        logger.error("Failed to discover seasons: %s", e)
        import traceback

        traceback.print_exc()
        db.close()
        return 1

    # Show current state of provider_seasons
    rows = db.fetchall(
        "SELECT ps.league_id, l.name, ps.season_year, ps.provider_season_id "
        "FROM provider_seasons ps "
        "JOIN leagues l ON l.id = ps.league_id "
        "ORDER BY l.id, ps.season_year"
    )

    print(f"\nProvider Season Mappings")
    print("=" * 60)
    print(f"  {'League':<20} {'Year':<8} {'SportMonks ID':<15}")
    print("-" * 60)
    for row in rows:
        print(f"  {row['name']:<20} {row['season_year']:<8} {row['provider_season_id']:<15}")
    print(f"\nNew mappings stored: {total_stored}")

    db.close()
    return 0


async def cmd_seed_async(args: argparse.Namespace) -> int:
    """Seed data using API handlers (BallDontLie / SportMonks).

    Supports two modes:
      Single:  seed --sport NBA --season 2025
      Batch:   seed --batch --seasons 2023-2025

    In batch mode, seeds are organized season-first (complete one season
    across all sports before moving to the next). Percentile recalculation
    runs automatically after each sport/season unless --skip-percentiles.
    """
    import time

    db = get_db()

    # Initialize database if needed
    if not db.is_initialized():
        from .schema import init_database

        logger.info("Database not initialized, running initialization...")
        init_database(db)

    # Handle --discover-seasons subcommand
    if getattr(args, "discover_seasons", False):
        return await _cmd_discover_seasons(args, db)

    # Determine batch vs single mode
    is_batch = getattr(args, "batch", False)
    skip_pct = getattr(args, "skip_percentiles", False)

    # Batch mode: percentiles on by default; single mode: only if explicit
    if is_batch:
        with_percentiles = not skip_pct
    else:
        with_percentiles = getattr(args, "with_percentiles", False) and not skip_pct

    from .seeders.common import SeedResult, BatchSeedResult

    if is_batch:
        return await _cmd_batch_seed(args, db, with_percentiles)

    # =========================================================================
    # SINGLE-SEASON SEED (original behavior + optional auto-percentiles)
    # =========================================================================
    sports_to_seed = []
    if args.all:
        sports_to_seed = ALL_SPORTS
    elif args.sport:
        sports_to_seed = [args.sport.upper()]
    else:
        logger.error("Must specify --sport or --all")
        return 1

    season = args.season or 2025
    total = SeedResult()

    for sport_id in sports_to_seed:
        try:
            handler = _get_handler(sport_id)
        except ValueError as e:
            logger.error("%s", e)
            return 1

        logger.info("Seeding %s for season %d...", sport_id, season)

        try:
            result = await _seed_sport_season(db, sport_id, season, handler)
            total = total + result
        except Exception as e:
            logger.error("Failed to seed %s: %s", sport_id, e)
            import traceback

            traceback.print_exc()

        # Auto-percentiles after each sport (if enabled)
        if with_percentiles and total.player_stats_upserted > 0:
            try:
                pct = _recalculate_percentiles(db, sport_id, season)
                logger.info(
                    "Percentiles recalculated for %s %d: %d players, %d teams",
                    sport_id,
                    season,
                    pct["players"],
                    pct["teams"],
                )
            except Exception as e:
                logger.error(
                    "Failed to recalculate percentiles for %s %d: %s",
                    sport_id,
                    season,
                    e,
                )

    # Update metadata
    db.set_meta("last_full_sync", datetime.now(tz=timezone.utc).isoformat())

    logger.info(
        "Total seeded: %d teams, %d player stats, %d team stats, %d errors",
        total.teams_upserted,
        total.player_stats_upserted,
        total.team_stats_upserted,
        len(total.errors),
    )

    db.close()
    return 0


async def _cmd_batch_seed(
    args: argparse.Namespace, db, with_percentiles: bool
) -> int:
    """Batch seed: multiple seasons across all sports with auto-percentiles.

    Execution order (season-first):
        For each season in range:
            1. Seed NBA (teams, players, stats -> triggers compute per-36)
            2. Recalculate NBA percentiles
            3. Seed NFL (teams, players, stats -> triggers compute derived)
            4. Recalculate NFL percentiles
            5. Discover provider season IDs for FOOTBALL leagues (if needed)
            6. Seed FOOTBALL for each benchmark league (triggers compute per-90)
            7. Recalculate FOOTBALL percentiles (across all leagues for that season)
    """
    import time

    from .seeders.common import BatchSeedResult

    batch_start = time.time()
    batch = BatchSeedResult()

    # Parse season range
    seasons_str = getattr(args, "seasons", None)
    if not seasons_str:
        logger.error("--batch requires --seasons (e.g., --seasons 2023-2025)")
        db.close()
        return 1

    seasons = _parse_seasons(seasons_str)
    logger.info("Batch seed: seasons %s", seasons)

    # Determine sports to seed
    sports_to_seed = []
    if args.all or not args.sport:
        sports_to_seed = ALL_SPORTS
    else:
        sports_to_seed = [args.sport.upper()]

    # Pre-fetch benchmark football leagues if FOOTBALL is in the list
    football_leagues: list[dict] = []
    if "FOOTBALL" in sports_to_seed:
        football_leagues = db.fetchall(
            "SELECT id, name, sportmonks_id FROM leagues "
            "WHERE sport = 'FOOTBALL' AND is_benchmark = true AND is_active = true "
            "ORDER BY id"
        )
        logger.info(
            "Football benchmark leagues: %s",
            ", ".join(f"{l['name']} (id={l['id']})" for l in football_leagues),
        )

        # Discover and store provider season IDs for all football leagues
        logger.info("Discovering SportMonks season IDs for football leagues...")
        try:
            sm_handler = _get_handler("FOOTBALL")
            async with sm_handler:
                for league in football_leagues:
                    sm_league_id = league.get("sportmonks_id")
                    if not sm_league_id:
                        logger.warning(
                            "No sportmonks_id for %s, skipping season discovery",
                            league["name"],
                        )
                        continue

                    stored = await _discover_and_store_seasons(
                        db, sm_handler, league["id"], sm_league_id, seasons
                    )
                    batch.provider_seasons_discovered += stored

            logger.info(
                "Provider season discovery complete: %d new mappings stored",
                batch.provider_seasons_discovered,
            )
        except Exception as e:
            logger.error("Failed to discover provider seasons: %s", e)
            import traceback

            traceback.print_exc()
            # Continue anyway — some seasons may already be mapped

    # =========================================================================
    # MAIN BATCH LOOP: season-first ordering
    # =========================================================================
    for season in sorted(seasons):
        logger.info("=" * 60)
        logger.info("BATCH SEED: Season %d", season)
        logger.info("=" * 60)

        for sport_id in sports_to_seed:
            logger.info("-" * 40)
            logger.info("Seeding %s %d...", sport_id, season)
            logger.info("-" * 40)

            try:
                handler = _get_handler(sport_id)
                if sport_id == "FOOTBALL":
                    result = await _seed_sport_season(
                        db, sport_id, season, handler, football_leagues
                    )
                else:
                    result = await _seed_sport_season(
                        db, sport_id, season, handler
                    )

                label = f"{sport_id}/{season}"
                batch.add_seed(result, label)

                logger.info(
                    "%s %d seeded: %d teams, %d player stats, %d team stats, %d errors",
                    sport_id,
                    season,
                    result.teams_upserted,
                    result.player_stats_upserted,
                    result.team_stats_upserted,
                    len(result.errors),
                )

            except Exception as e:
                logger.error("Failed to seed %s %d: %s", sport_id, season, e)
                import traceback

                traceback.print_exc()
                batch.seed_result.errors.append(f"{sport_id}/{season}: {e}")
                continue

            # Recalculate percentiles after each sport/season
            if with_percentiles:
                try:
                    pct = _recalculate_percentiles(db, sport_id, season)
                    batch.add_percentile(f"{sport_id}/{season}")
                    logger.info(
                        "Percentiles for %s %d: %d players, %d teams",
                        sport_id,
                        season,
                        pct["players"],
                        pct["teams"],
                    )
                except Exception as e:
                    logger.error(
                        "Failed to recalculate percentiles for %s %d: %s",
                        sport_id,
                        season,
                        e,
                    )

    batch.total_duration_seconds = time.time() - batch_start

    # Update metadata
    db.set_meta("last_full_sync", datetime.now(tz=timezone.utc).isoformat())

    # =========================================================================
    # BATCH SUMMARY
    # =========================================================================
    sr = batch.seed_result
    print(f"\n{'=' * 60}")
    print(f"BATCH SEED COMPLETE")
    print(f"{'=' * 60}")
    print(f"  Duration:             {batch.total_duration_seconds:.1f}s")
    print(f"  Seasons completed:    {len(batch.seasons_completed)}")
    for label in batch.seasons_completed:
        print(f"    - {label}")
    print(f"  Teams upserted:       {sr.teams_upserted:,}")
    print(f"  Player stats:         {sr.player_stats_upserted:,}")
    print(f"  Team stats:           {sr.team_stats_upserted:,}")
    if batch.provider_seasons_discovered > 0:
        print(f"  Season IDs discovered: {batch.provider_seasons_discovered}")
    if batch.percentiles_computed:
        print(f"  Percentiles computed: {len(batch.percentiles_computed)}")
        for label in batch.percentiles_computed:
            print(f"    - {label}")
    if sr.errors:
        print(f"  Errors:               {len(sr.errors)}")
        for err in sr.errors[:20]:
            print(f"    - {err}")
        if len(sr.errors) > 20:
            print(f"    ... and {len(sr.errors) - 20} more")
    print(f"{'=' * 60}")

    db.close()
    return 0 if not sr.errors else 1


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
        # Export players with stats in a single JOIN query (not N+1)
        player_rows = db.fetchall(
            f"""
            SELECT p.*, ps.stats, ps.percentiles
            FROM {PLAYERS_TABLE} p
            LEFT JOIN {PLAYER_STATS_TABLE} ps
                ON ps.player_id = p.id AND ps.sport = p.sport AND ps.season = %s
            WHERE p.sport = %s
            """,
            (season, sport_id),
        )

        player_data = []
        for row in player_rows:
            stats = row.pop("stats", None)
            percentiles = row.pop("percentiles", None)
            player_data.append(
                {
                    "player": row,
                    "stats": stats,
                    "percentiles": percentiles,
                }
            )

        # Export teams with stats in a single JOIN query (not N+1)
        team_rows = db.fetchall(
            f"""
            SELECT t.*, ts.stats, ts.percentiles
            FROM {TEAMS_TABLE} t
            LEFT JOIN {TEAM_STATS_TABLE} ts
                ON ts.team_id = t.id AND ts.sport = t.sport AND ts.season = %s
            WHERE t.sport = %s
            """,
            (season, sport_id),
        )

        team_data = []
        for row in team_rows:
            t_stats = row.pop("stats", None)
            t_percentiles = row.pop("percentiles", None)
            team_data.append(
                {
                    "team": row,
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


def cmd_export_profiles(args: argparse.Namespace) -> int:
    """Export minimal entity profiles for frontend bootstrap/autocomplete.

    Generates per-sport JSON files (v2.0 format) containing players and teams
    with just enough data for fuzzy search and API lookups. Uses the current
    DB data with correct BallDontLie (NBA/NFL) and SportMonks (Football) IDs.

    Output: exports/{sport}_entities.json
    """
    import unicodedata

    db = get_db()

    if not db.is_initialized():
        logger.error("Database not initialized.")
        return 1

    output_dir = Path(args.output) if args.output else Path("./exports")
    output_dir.mkdir(parents=True, exist_ok=True)

    season = args.season or 2025
    sport_filter = args.sport.upper() if args.sport else None
    sports = [sport_filter] if sport_filter else ALL_SPORTS

    def _normalize_text(text: str) -> str:
        """Lowercase + strip accents for fuzzy search."""
        nfkd = unicodedata.normalize("NFKD", text)
        return "".join(c for c in nfkd if not unicodedata.combining(c)).lower()

    def _tokenize(name: str) -> list[str]:
        """Split name into search tokens, sorted alphabetically."""
        return sorted(name.lower().split())

    # NBA position -> position_group mapping
    NBA_POS_GROUP = {
        "G": "Guard", "PG": "Guard", "SG": "Guard",
        "F": "Forward", "SF": "Forward", "PF": "Forward",
        "C": "Center",
        "G-F": "Guard-Forward", "F-G": "Guard-Forward",
        "F-C": "Forward-Center", "C-F": "Forward-Center",
    }

    # NFL position -> position_group mapping
    NFL_POS_GROUP = {
        "QB": "Offense", "RB": "Offense - Skill", "FB": "Offense",
        "WR": "Offense - Skill", "TE": "Offense - Skill",
        "OT": "Offense - Line", "OG": "Offense - Line", "C": "Offense - Line",
        "OL": "Offense - Line", "T": "Offense - Line", "G": "Offense - Line",
        "DE": "Defense - Line", "DT": "Defense - Line", "DL": "Defense - Line",
        "NT": "Defense - Line",
        "LB": "Defense - Linebacker", "OLB": "Defense - Linebacker",
        "ILB": "Defense - Linebacker", "MLB": "Defense - Linebacker",
        "CB": "Defense - Secondary", "S": "Defense - Secondary",
        "SS": "Defense - Secondary", "FS": "Defense - Secondary",
        "DB": "Defense - Secondary",
        "K": "Special Teams", "P": "Special Teams", "LS": "Special Teams",
        "KR": "Special Teams", "PR": "Special Teams",
    }

    for sport_id in sports:
        entities: list[dict] = []

        logger.info("Exporting %s profiles for season %d...", sport_id, season)

        # =====================================================================
        # PLAYERS
        # =====================================================================
        if sport_id == "FOOTBALL":
            # Football: resolve league_id from player_stats (players.league_id may be NULL)
            player_rows = db.fetchall(
                f"""
                SELECT DISTINCT ON (p.id)
                    p.id, p.name, p.position, p.detailed_position, p.meta,
                    t.name AS team_name, t.short_code AS team_abbr,
                    ps.league_id,
                    l.name AS league_name
                FROM {PLAYERS_TABLE} p
                LEFT JOIN {TEAMS_TABLE} t
                    ON t.id = p.team_id AND t.sport = p.sport
                LEFT JOIN {PLAYER_STATS_TABLE} ps
                    ON ps.player_id = p.id AND ps.sport = p.sport AND ps.season = %s
                LEFT JOIN leagues l
                    ON l.id = ps.league_id
                WHERE p.sport = %s
                ORDER BY p.id, ps.league_id
                """,
                (season, sport_id),
            )
        else:
            # NBA/NFL: no league_id needed
            player_rows = db.fetchall(
                f"""
                SELECT p.id, p.name, p.position, p.meta,
                       t.short_code AS team_abbr
                FROM {PLAYERS_TABLE} p
                LEFT JOIN {TEAMS_TABLE} t
                    ON t.id = p.team_id AND t.sport = p.sport
                WHERE p.sport = %s
                """,
                (sport_id,),
            )

        for row in player_rows:
            if not row.get("name"):
                continue

            entity: dict = {
                "id": f"{sport_id.lower()}_player_{row['id']}",
                "entity_id": row["id"],
                "type": "player",
                "sport": sport_id,
                "name": row["name"],
                "normalized": _normalize_text(row["name"]),
                "tokens": _tokenize(row["name"]),
            }

            meta: dict = {}
            position = row.get("position")
            if position:
                meta["position"] = position

            if sport_id == "NBA" and position:
                pg = NBA_POS_GROUP.get(position, "")
                if pg:
                    meta["position_group"] = pg
            elif sport_id == "NFL" and position:
                pg = NFL_POS_GROUP.get(position, "")
                if pg:
                    meta["position_group"] = pg

            team_abbr = row.get("team_abbr")
            if team_abbr:
                meta["team"] = team_abbr
            elif sport_id == "FOOTBALL" and row.get("team_name"):
                meta["team"] = row["team_name"]

            if sport_id == "FOOTBALL":
                league_id = row.get("league_id")
                if league_id:
                    entity["league_id"] = league_id
                league_name = row.get("league_name")
                if league_name:
                    meta["league"] = league_name

            entity["meta"] = meta
            entities.append(entity)

        # =====================================================================
        # TEAMS
        # =====================================================================
        if sport_id == "FOOTBALL":
            team_rows = db.fetchall(
                f"""
                SELECT DISTINCT ON (t.id)
                    t.id, t.name, t.short_code, t.country,
                    ts.league_id,
                    l.name AS league_name
                FROM {TEAMS_TABLE} t
                LEFT JOIN {TEAM_STATS_TABLE} ts
                    ON ts.team_id = t.id AND ts.sport = t.sport AND ts.season = %s
                LEFT JOIN leagues l
                    ON l.id = ts.league_id
                WHERE t.sport = %s
                ORDER BY t.id, ts.league_id
                """,
                (season, sport_id),
            )
        else:
            team_rows = db.fetchall(
                f"""
                SELECT t.id, t.name, t.short_code, t.conference, t.division, t.country
                FROM {TEAMS_TABLE} t
                WHERE t.sport = %s
                """,
                (sport_id,),
            )

        for row in team_rows:
            if not row.get("name"):
                continue

            entity = {
                "id": f"{sport_id.lower()}_team_{row['id']}",
                "entity_id": row["id"],
                "type": "team",
                "sport": sport_id,
                "name": row["name"],
                "normalized": _normalize_text(row["name"]),
                "tokens": _tokenize(row["name"]),
            }

            meta = {}
            if row.get("short_code"):
                meta["abbreviation"] = row["short_code"]

            if sport_id in ("NBA", "NFL"):
                if row.get("conference"):
                    meta["conference"] = row["conference"]
                if row.get("division"):
                    meta["division"] = row["division"]
            elif sport_id == "FOOTBALL":
                if row.get("country"):
                    meta["country"] = row["country"]
                league_id = row.get("league_id")
                if league_id:
                    entity["league_id"] = league_id
                league_name = row.get("league_name")
                if league_name:
                    meta["league"] = league_name

            entity["meta"] = meta
            entities.append(entity)

        # =====================================================================
        # WRITE OUTPUT
        # =====================================================================
        export_data = {
            "version": "2.0",
            "generated_at": datetime.now(tz=timezone.utc).isoformat(),
            "sport": sport_id,
            "count": len(entities),
            "entities": entities,
        }

        output_file = output_dir / f"{sport_id.lower()}_entities.json"
        with open(output_file, "w") as f:
            json.dump(export_data, f, indent=2, default=str)

        # Count breakdown
        player_count = sum(1 for e in entities if e["type"] == "player")
        team_count = sum(1 for e in entities if e["type"] == "team")
        logger.info(
            "Exported %s: %d entities (%d players, %d teams) -> %s",
            sport_id,
            len(entities),
            player_count,
            team_count,
            output_file,
        )

    db.close()
    return 0


def cmd_query(args: argparse.Namespace) -> int:
    """Run a query against the database."""
    return asyncio.run(_cmd_query_async(args))


async def _cmd_query_async(args: argparse.Namespace) -> int:
    """Async implementation of query command (services are async)."""
    from .async_pg_connection import AsyncPostgresDB

    db = AsyncPostgresDB()
    await db.open()

    try:
        query_type = args.type
        sport = args.sport.upper() if args.sport else "NBA"
        season = args.season or 2025

        if query_type == "leaders":
            from .services.stats import get_stat_leaders

            stat = args.stat or "points_per_game"
            limit = args.limit or 25

            results = await get_stat_leaders(db, sport, season, stat, limit)

            print(f"\n{sport} {season} - Top {limit} {stat}")
            print("=" * 60)
            for r in results:
                print(
                    f"{r['rank']:3}. {r['name']:<25} {r['stat_value']:>10.1f}  ({r['team_name'] or 'N/A'})"
                )

        elif query_type == "standings":
            from .services.stats import get_standings

            results = await get_standings(
                db,
                sport,
                season,
                league_id=args.league or 0,
                conference=args.conference,
            )

            print(f"\n{sport} {season} Standings")
            print("=" * 60)
            for r in results:
                stats = r.get("stats", {}) or {}
                if sport == "FOOTBALL":
                    print(
                        f"{r['rank']:3}. {r['name']:<25} {stats.get('points', 0):3} pts  "
                        f"({stats.get('wins', 0)}-{stats.get('draws', 0)}-{stats.get('losses', 0)})"
                    )
                else:
                    win_pct = r.get("win_pct") or 0
                    wins = stats.get("wins", 0) or 0
                    losses = stats.get("losses", 0) or 0
                    print(
                        f"{r['rank']:3}. {r['name']:<25} {float(win_pct):.3f}  ({wins}-{losses})"
                    )

        elif query_type == "profile":
            entity_type = args.entity_type or "player"
            entity_id = args.entity_id

            if not entity_id:
                logger.error("Must specify --entity-id for profile query")
                return 1

            if entity_type == "player":
                from .services.profiles import get_player_profile

                result = await get_player_profile(db, entity_id, sport)
            else:
                from .services.profiles import get_team_profile

                result = await get_team_profile(db, entity_id, sport)

            if result:
                print(json.dumps(result, indent=2, default=str))
            else:
                print("Not found")

        else:
            logger.error("Unknown query type: %s", query_type)
            return 1

    finally:
        await db.close()

    return 0


# =============================================================================
# FIXTURE MANAGEMENT COMMANDS
# =============================================================================



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

    db = get_db()

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

    db = get_db()

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

    db = get_db()

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

    db = get_db()

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

    db = get_db()

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

    db = get_db()

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

    db = get_db()

    # Determine sport from fixture to create appropriate handler
    fixture = db.fetchone(
        "SELECT sport FROM fixtures WHERE id = %s", (args.fixture_id,)
    )
    sport_id = fixture["sport"] if fixture else "NBA"
    handler = _get_handler(sport_id)

    try:
        seeder = PostMatchSeeder(db, handler)
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

    db = get_db()
    # Scheduler creates per-fixture handlers as needed
    sport_id = (args.sport or "NBA").upper()
    handler = _get_handler(sport_id)

    try:
        scheduler = SchedulerService(
            db,
            handler,
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

    db = get_db()

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

    # seed command (uses BallDontLie / SportMonks handlers)
    seed_parser = subparsers.add_parser(
        "seed",
        help="Seed data from APIs (BallDontLie for NBA/NFL, SportMonks for Football)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
examples:
  seed --sport NBA --season 2025              # Single sport, single season
  seed --all --season 2025                    # All sports, single season
  seed --all --season 2025 --with-percentiles # With auto-percentile recalc
  seed --batch --seasons 2023-2025            # Batch: all sports, 3 seasons
  seed --batch --seasons 2023,2025 --sport NBA  # Batch: NBA only, 2 seasons
  seed --batch --seasons 2023-2025 --skip-percentiles  # Batch without percentiles

  # Discover SportMonks season IDs for football leagues
  seed --discover-seasons --seasons 2023-2025
        """,
    )
    seed_parser.add_argument("--sport", help="Sport to seed (NBA, NFL, FOOTBALL)")
    seed_parser.add_argument("--all", action="store_true", help="Seed all sports")
    seed_parser.add_argument("--season", type=int, help="Season year (single mode)")
    seed_parser.add_argument(
        "--batch",
        action="store_true",
        help="Batch mode: seed multiple seasons with auto-percentiles",
    )
    seed_parser.add_argument(
        "--seasons",
        help="Season range for batch mode (e.g., 2023-2025 or 2023,2024,2025)",
    )
    seed_parser.add_argument(
        "--with-percentiles",
        action="store_true",
        default=False,
        help="Auto-recalculate percentiles after seeding (default in batch mode)",
    )
    seed_parser.add_argument(
        "--skip-percentiles",
        action="store_true",
        default=False,
        help="Skip percentile recalculation (overrides --with-percentiles and batch default)",
    )
    seed_parser.add_argument(
        "--discover-seasons",
        action="store_true",
        help="Discover and store SportMonks season IDs for football leagues, then exit",
    )

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

    # export-profiles command (frontend bootstrap)
    export_profiles_parser = subparsers.add_parser(
        "export-profiles",
        help="Export minimal entity profiles for frontend autocomplete bootstrap",
    )
    export_profiles_parser.add_argument("--sport", help="Sport to export (default: all)")
    export_profiles_parser.add_argument("--season", type=int, help="Season year (default: 2025)")
    export_profiles_parser.add_argument("--output", help="Output directory (default: ./exports)")

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
        "export-profiles": cmd_export_profiles,
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
