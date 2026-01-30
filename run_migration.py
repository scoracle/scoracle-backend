#!/usr/bin/env python3
"""Run database migrations."""

import asyncio
import asyncpg
import sys
from pathlib import Path

DATABASE_URL = "postgresql://neondb_owner:npg_7x0jtQPmfWZq@ep-noisy-frost-aemyfaig-pooler.c-2.us-east-2.aws.neon.tech/neondb?sslmode=require"


async def run_migration():
    """Run the initial schema migration."""
    migration_file = Path(__file__).parent / "migrations" / "001_initial_schema.sql"
    
    if not migration_file.exists():
        print(f"ERROR: Migration file not found: {migration_file}")
        sys.exit(1)
    
    print(f"Reading migration: {migration_file}")
    sql = migration_file.read_text()
    
    print("Connecting to database...")
    conn = await asyncpg.connect(DATABASE_URL)
    
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
