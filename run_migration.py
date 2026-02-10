#!/usr/bin/env python3
"""Run database migrations."""

import asyncio
import os
import sys
from pathlib import Path

import asyncpg
from dotenv import load_dotenv

load_dotenv()


async def run_migration():
    """Run the initial schema migration."""
    database_url = (
        os.environ.get("NEON_DATABASE_URL_V2")
        or os.environ.get("DATABASE_URL")
        or os.environ.get("NEON_DATABASE_URL")
    )

    if not database_url:
        print(
            "ERROR: No database URL found. Set NEON_DATABASE_URL_V2, DATABASE_URL, or NEON_DATABASE_URL."
        )
        sys.exit(1)

    migration_file = (
        Path(__file__).parent
        / "src"
        / "scoracle_data"
        / "migrations"
        / "001_schema.sql"
    )

    if not migration_file.exists():
        print(f"ERROR: Migration file not found: {migration_file}")
        sys.exit(1)

    print(f"Reading migration: {migration_file}")
    sql = migration_file.read_text()

    print("Connecting to database...")
    conn = await asyncpg.connect(database_url)

    try:
        print("Running migration...")
        await conn.execute(sql)
        print("Migration completed successfully!")

        # Verify tables were created
        result = await conn.fetch("""
            SELECT table_name 
            FROM information_schema.tables 
            WHERE table_schema = 'public' 
            ORDER BY table_name
        """)

        print(f"\nCreated {len(result)} tables:")
        for row in result:
            print(f"  - {row['table_name']}")

    finally:
        await conn.close()


if __name__ == "__main__":
    asyncio.run(run_migration())
