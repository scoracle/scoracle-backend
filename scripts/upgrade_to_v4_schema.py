#!/usr/bin/env python3
"""
Upgrade database from v3.0 schema to v4.0 sport-specific schema.

This script:
1. Creates the new sport-specific profile tables (nba_player_profiles, etc.)
2. Migrates data from unified players/teams tables to sport-specific tables
3. Updates foreign key references in stats tables

Usage:
    # Set environment variable for database
    export NEON_DATABASE_URL_V2=postgresql://...

    # Run upgrade
    python scripts/upgrade_to_v4_schema.py

    # Dry run (show what would be done)
    python scripts/upgrade_to_v4_schema.py --dry-run
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path
from datetime import datetime

# Add src to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))


def get_connection_string() -> str:
    """Get database connection string from environment."""
    return (
        os.environ.get("NEON_DATABASE_URL_V2")
        or os.environ.get("DATABASE_URL")
        or os.environ.get("NEON_DATABASE_URL")
    )


def get_connection(connection_string: str):
    """Create PostgreSQL connection."""
    try:
        import psycopg
        from psycopg.rows import dict_row
        return psycopg.connect(connection_string, row_factory=dict_row)
    except ImportError:
        print("Error: psycopg not installed. Run: pip install 'psycopg[binary]'")
        sys.exit(1)


def check_schema_version(conn) -> str:
    """Check current schema version."""
    with conn.cursor() as cur:
        cur.execute("SELECT value FROM meta WHERE key = 'schema_version'")
        row = cur.fetchone()
        return row["value"] if row else "unknown"


def table_exists(conn, table_name: str) -> bool:
    """Check if a table exists."""
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT EXISTS(
                SELECT 1 FROM information_schema.tables
                WHERE table_name = %s
            )
            """,
            (table_name,)
        )
        return cur.fetchone()["exists"]


def run_schema_upgrade(conn, dry_run: bool = False) -> None:
    """Run the v4.0 schema migration."""
    schema_path = Path(__file__).parent.parent / "src" / "scoracle_data" / "migrations" / "001_sport_specific_schema.sql"

    if not schema_path.exists():
        print(f"Error: Schema file not found: {schema_path}")
        sys.exit(1)

    schema_sql = schema_path.read_text()

    if dry_run:
        print("Would execute schema from:", schema_path)
        print(f"Schema size: {len(schema_sql)} characters")
        return

    print(f"Executing schema from {schema_path}...")
    with conn.cursor() as cur:
        cur.execute(schema_sql)
    conn.commit()
    print("Schema applied successfully")


def migrate_player_data(conn, sport_id: str, target_table: str, dry_run: bool = False) -> int:
    """Migrate player data from unified table to sport-specific table."""
    # Check if source table exists
    if not table_exists(conn, "players"):
        print(f"  Source table 'players' does not exist, skipping {sport_id} migration")
        return 0

    # Build column mappings based on sport
    if sport_id == "FOOTBALL":
        columns = """
            id, first_name, last_name, full_name, position, position_group,
            nationality, birth_date, birth_place, birth_country,
            height_inches, weight_lbs, photo_url, current_team_id,
            current_league_id, jersey_number, profile_fetched_at,
            is_active, created_at, updated_at
        """
    else:  # NBA, NFL
        columns = """
            id, first_name, last_name, full_name, position, position_group,
            nationality, birth_date, birth_place, birth_country,
            height_inches, weight_lbs, photo_url, current_team_id,
            jersey_number, college, experience_years, profile_fetched_at,
            is_active, created_at, updated_at
        """

    # Count source rows
    with conn.cursor() as cur:
        cur.execute(
            "SELECT COUNT(*) as count FROM players WHERE sport_id = %s",
            (sport_id,)
        )
        count = cur.fetchone()["count"]

    if count == 0:
        print(f"  {sport_id}: No players to migrate")
        return 0

    if dry_run:
        print(f"  {sport_id}: Would migrate {count} players to {target_table}")
        return count

    # Migrate data
    insert_sql = f"""
        INSERT INTO {target_table} ({columns})
        SELECT {columns} FROM players WHERE sport_id = %s
        ON CONFLICT (id) DO NOTHING
    """

    with conn.cursor() as cur:
        cur.execute(insert_sql, (sport_id,))
        migrated = cur.rowcount
    conn.commit()

    print(f"  {sport_id}: Migrated {migrated} players to {target_table}")
    return migrated


def migrate_team_data(conn, sport_id: str, target_table: str, dry_run: bool = False) -> int:
    """Migrate team data from unified table to sport-specific table."""
    # Check if source table exists
    if not table_exists(conn, "teams"):
        print(f"  Source table 'teams' does not exist, skipping {sport_id} migration")
        return 0

    # Build column mappings based on sport
    if sport_id == "FOOTBALL":
        columns = """
            id, name, abbreviation, country, city, league_id, logo_url,
            founded, is_national, venue_name, venue_address, venue_city,
            venue_capacity, venue_surface, venue_image, profile_fetched_at,
            is_active, created_at, updated_at
        """
    else:  # NBA, NFL
        columns = """
            id, name, abbreviation, conference, division, city, country,
            logo_url, founded, venue_name, venue_address, venue_city,
            venue_capacity, venue_surface, venue_image, profile_fetched_at,
            is_active, created_at, updated_at
        """

    # Count source rows
    with conn.cursor() as cur:
        cur.execute(
            "SELECT COUNT(*) as count FROM teams WHERE sport_id = %s",
            (sport_id,)
        )
        count = cur.fetchone()["count"]

    if count == 0:
        print(f"  {sport_id}: No teams to migrate")
        return 0

    if dry_run:
        print(f"  {sport_id}: Would migrate {count} teams to {target_table}")
        return count

    # Migrate data
    insert_sql = f"""
        INSERT INTO {target_table} ({columns})
        SELECT {columns} FROM teams WHERE sport_id = %s
        ON CONFLICT (id) DO NOTHING
    """

    with conn.cursor() as cur:
        cur.execute(insert_sql, (sport_id,))
        migrated = cur.rowcount
    conn.commit()

    print(f"  {sport_id}: Migrated {migrated} teams to {target_table}")
    return migrated


def update_schema_version(conn, version: str) -> None:
    """Update schema version in meta table."""
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES ('schema_version', %s, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            """,
            (version,)
        )
    conn.commit()


def main():
    parser = argparse.ArgumentParser(description="Upgrade database to v4.0 sport-specific schema")
    parser.add_argument(
        "--pg-url",
        default=get_connection_string(),
        help="PostgreSQL connection URL",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without making changes",
    )
    parser.add_argument(
        "--skip-schema",
        action="store_true",
        help="Skip schema creation (only migrate data)",
    )
    parser.add_argument(
        "--skip-data",
        action="store_true",
        help="Skip data migration (only create schema)",
    )

    args = parser.parse_args()

    if not args.pg_url:
        print("Error: No database URL found.")
        print("Set NEON_DATABASE_URL_V2, DATABASE_URL, or use --pg-url")
        sys.exit(1)

    # Mask password in output
    display_url = args.pg_url.split('@')[1] if '@' in args.pg_url else 'configured'

    print(f"{'[DRY RUN] ' if args.dry_run else ''}Database Schema Upgrade to v4.0")
    print(f"PostgreSQL: {display_url}")
    print(f"Started: {datetime.now().isoformat()}")
    print()

    conn = get_connection(args.pg_url)

    try:
        # Check current version
        current_version = check_schema_version(conn)
        print(f"Current schema version: {current_version}")

        # Check if already on v4.0
        if table_exists(conn, "nba_player_profiles") and not args.skip_schema:
            print("Sport-specific tables already exist (v4.0 schema detected)")
            print("Use --skip-schema if you only want to migrate data")

        # Run schema upgrade
        if not args.skip_schema:
            print()
            print("Creating sport-specific tables...")
            run_schema_upgrade(conn, args.dry_run)

        # Migrate data from unified tables
        if not args.skip_data:
            print()
            print("Migrating team data...")
            sport_team_tables = {
                "NBA": "nba_team_profiles",
                "NFL": "nfl_team_profiles",
                "FOOTBALL": "football_team_profiles",
            }
            for sport_id, table in sport_team_tables.items():
                migrate_team_data(conn, sport_id, table, args.dry_run)

            print()
            print("Migrating player data...")
            sport_player_tables = {
                "NBA": "nba_player_profiles",
                "NFL": "nfl_player_profiles",
                "FOOTBALL": "football_player_profiles",
            }
            for sport_id, table in sport_player_tables.items():
                migrate_player_data(conn, sport_id, table, args.dry_run)

        # Update schema version
        if not args.dry_run:
            print()
            print("Updating schema version to 4.0...")
            update_schema_version(conn, "4.0")

        print()
        print(f"Finished: {datetime.now().isoformat()}")
        print("Upgrade complete!" if not args.dry_run else "Dry run complete - no changes made")

    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        conn.close()


if __name__ == "__main__":
    main()
