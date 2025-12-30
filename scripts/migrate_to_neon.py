#!/usr/bin/env python3
"""
Migrate data from SQLite to Neon PostgreSQL.

Usage:
    # Set environment variables
    export SQLITE_DB_PATH=/path/to/stats.sqlite
    export DATABASE_URL=postgresql://user:password@ep-xxx.region.aws.neon.tech/scoracle_data

    # Run migration
    python scripts/migrate_to_neon.py

    # With options
    python scripts/migrate_to_neon.py --dry-run
    python scripts/migrate_to_neon.py --tables players,teams
"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional

# Add src to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))


def get_sqlite_connection(db_path: str) -> sqlite3.Connection:
    """Create SQLite connection."""
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    return conn


def get_postgres_connection(connection_string: str):
    """Create PostgreSQL connection."""
    try:
        import psycopg
        from psycopg.rows import dict_row
        return psycopg.connect(connection_string, row_factory=dict_row)
    except ImportError:
        print("Error: psycopg not installed. Run: pip install 'psycopg[binary]'")
        sys.exit(1)


def get_table_columns(sqlite_conn: sqlite3.Connection, table: str) -> list[str]:
    """Get column names for a table."""
    cursor = sqlite_conn.cursor()
    cursor.execute(f"PRAGMA table_info({table})")
    return [row[1] for row in cursor.fetchall()]


def get_row_count(sqlite_conn: sqlite3.Connection, table: str) -> int:
    """Get row count for a table."""
    cursor = sqlite_conn.cursor()
    cursor.execute(f"SELECT COUNT(*) FROM {table}")
    return cursor.fetchone()[0]


def migrate_table(
    sqlite_conn: sqlite3.Connection,
    pg_conn,
    table: str,
    batch_size: int = 1000,
    dry_run: bool = False,
) -> int:
    """
    Migrate a single table from SQLite to PostgreSQL.

    Args:
        sqlite_conn: SQLite connection
        pg_conn: PostgreSQL connection
        table: Table name to migrate
        batch_size: Number of rows per batch
        dry_run: If True, don't actually insert data

    Returns:
        Number of rows migrated
    """
    # Get row count
    total_rows = get_row_count(sqlite_conn, table)
    if total_rows == 0:
        print(f"  {table}: 0 rows (skipping)")
        return 0

    # Get column names
    columns = get_table_columns(sqlite_conn, table)

    if dry_run:
        print(f"  {table}: {total_rows} rows (dry run)")
        return total_rows

    # Prepare PostgreSQL insert
    placeholders = ", ".join(["%s"] * len(columns))
    column_list = ", ".join(columns)
    insert_query = f"INSERT INTO {table} ({column_list}) VALUES ({placeholders}) ON CONFLICT DO NOTHING"

    # Migrate in batches
    cursor = sqlite_conn.cursor()
    cursor.execute(f"SELECT * FROM {table}")

    migrated = 0
    while True:
        rows = cursor.fetchmany(batch_size)
        if not rows:
            break

        # Convert to list of tuples
        data = [tuple(row) for row in rows]

        with pg_conn.cursor() as pg_cursor:
            pg_cursor.executemany(insert_query, data)
        pg_conn.commit()

        migrated += len(data)
        print(f"  {table}: {migrated}/{total_rows} rows migrated", end="\r")

    print(f"  {table}: {migrated} rows migrated          ")
    return migrated


def run_schema(pg_conn, schema_path: Path) -> None:
    """Run the PostgreSQL schema migration."""
    if not schema_path.exists():
        print(f"Error: Schema file not found: {schema_path}")
        sys.exit(1)

    schema_sql = schema_path.read_text()

    with pg_conn.cursor() as cursor:
        cursor.execute(schema_sql)
    pg_conn.commit()
    print(f"Schema applied from {schema_path}")


def main():
    parser = argparse.ArgumentParser(description="Migrate SQLite to PostgreSQL/Neon")
    parser.add_argument(
        "--sqlite-path",
        default=os.environ.get("SQLITE_DB_PATH", "stats.sqlite"),
        help="Path to SQLite database",
    )
    parser.add_argument(
        "--pg-url",
        default=os.environ.get("DATABASE_URL"),
        help="PostgreSQL connection URL",
    )
    parser.add_argument(
        "--tables",
        help="Comma-separated list of tables to migrate (default: all)",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=1000,
        help="Batch size for inserts (default: 1000)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be migrated without actually migrating",
    )
    parser.add_argument(
        "--skip-schema",
        action="store_true",
        help="Skip running the schema migration",
    )
    parser.add_argument(
        "--recalculate-percentiles",
        action="store_true",
        help="Recalculate percentiles after migration using PostgreSQL engine",
    )

    args = parser.parse_args()

    if not args.pg_url:
        print("Error: DATABASE_URL environment variable or --pg-url required")
        sys.exit(1)

    if not Path(args.sqlite_path).exists():
        print(f"Error: SQLite database not found: {args.sqlite_path}")
        sys.exit(1)

    # Tables in dependency order
    all_tables = [
        "sports",
        "seasons",
        "leagues",
        "teams",
        "players",
        "player_teams",
        "nba_player_stats",
        "nba_team_stats",
        "nfl_player_stats",
        "nfl_team_stats",
        "football_player_stats",
        "football_team_stats",
        "football_standings_snapshot",
        "percentile_cache",
        "sync_log",
        "entities_minimal",
        "meta",
    ]

    tables = args.tables.split(",") if args.tables else all_tables

    print(f"{'[DRY RUN] ' if args.dry_run else ''}Migration: SQLite -> Neon PostgreSQL")
    print(f"SQLite: {args.sqlite_path}")
    print(f"PostgreSQL: {args.pg_url.split('@')[1] if '@' in args.pg_url else 'configured'}")
    print(f"Started: {datetime.now().isoformat()}")
    print()

    # Connect to databases
    sqlite_conn = get_sqlite_connection(args.sqlite_path)
    pg_conn = get_postgres_connection(args.pg_url)

    try:
        # Run schema if not skipped
        if not args.skip_schema and not args.dry_run:
            schema_path = Path(__file__).parent.parent / "src" / "scoracle_data" / "migrations" / "007_postgresql_schema.sql"
            print("Applying PostgreSQL schema...")
            run_schema(pg_conn, schema_path)
            print()

        # Migrate tables
        print("Migrating tables:")
        total_migrated = 0
        for table in tables:
            try:
                count = migrate_table(
                    sqlite_conn,
                    pg_conn,
                    table,
                    batch_size=args.batch_size,
                    dry_run=args.dry_run,
                )
                total_migrated += count
            except Exception as e:
                print(f"  {table}: ERROR - {e}")

        print()
        print(f"Total rows migrated: {total_migrated}")

        # Recalculate percentiles if requested
        if args.recalculate_percentiles and not args.dry_run:
            print()
            print("Recalculating percentiles with PostgreSQL engine...")
            from scoracle_data.pg_connection import PostgresDB
            from scoracle_data.percentiles.pg_calculator import PostgresPercentileCalculator

            db = PostgresDB(args.pg_url)
            calc = PostgresPercentileCalculator(db)

            for sport in ["NBA", "NFL", "FOOTBALL"]:
                current_season = db.get_current_season(sport)
                if current_season:
                    result = calc.recalculate_all_percentiles(sport, current_season["season_year"])
                    print(f"  {sport}: {result['players']} player records, {result['teams']} team records")

            db.close()

    finally:
        sqlite_conn.close()
        pg_conn.close()

    print()
    print(f"Finished: {datetime.now().isoformat()}")
    print("Migration complete!")


if __name__ == "__main__":
    main()
