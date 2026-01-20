"""
Database schema management for the stats database.

Handles initialization, migrations, and schema version tracking.
PostgreSQL-only implementation.
"""

from __future__ import annotations

import logging
import time
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .pg_connection import PostgresDB

logger = logging.getLogger(__name__)

MIGRATIONS_DIR = Path(__file__).parent / "migrations"


def get_migration_files() -> list[Path]:
    """Get all SQL migration files in order."""
    if not MIGRATIONS_DIR.exists():
        return []

    files = sorted(MIGRATIONS_DIR.glob("*.sql"))
    return files


def run_migrations(db: "PostgresDB", force: bool = False) -> int:
    """
    Run all pending migrations.

    Args:
        db: Database connection
        force: If True, run all migrations even if already applied

    Returns:
        Number of migrations applied
    """
    migration_files = get_migration_files()
    if not migration_files:
        logger.warning("No migration files found in %s", MIGRATIONS_DIR)
        return 0

    applied = 0

    for migration_file in migration_files:
        migration_name = migration_file.stem

        # Check if already applied (unless force)
        if not force and db.is_initialized():
            existing = db.fetchone(
                "SELECT value FROM meta WHERE key = %s",
                (f"migration_{migration_name}",),
            )
            if existing:
                logger.debug("Skipping already applied migration: %s", migration_name)
                continue

        logger.info("Applying migration: %s", migration_name)

        # Read and execute migration
        sql = migration_file.read_text()

        try:
            # Execute the entire migration script
            db.execute(sql)

            # Record migration as applied
            db.execute(
                """
                INSERT INTO meta (key, value, updated_at)
                VALUES (%s, %s, NOW())
                ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
                """,
                (f"migration_{migration_name}", "applied"),
            )

            applied += 1
            logger.info("Successfully applied migration: %s", migration_name)

        except Exception as e:
            logger.error("Failed to apply migration %s: %s", migration_name, e)
            raise

    return applied


def init_database(db: "PostgresDB") -> None:
    """
    Initialize the database with the full schema.

    This runs all migrations in order to set up the complete schema.

    Args:
        db: Database connection
    """
    logger.info("Initializing stats database...")

    # Run all migrations
    applied = run_migrations(db)

    logger.info("Database initialized with %d migrations", applied)


def get_schema_version(db: "PostgresDB") -> str:
    """Get the current schema version."""
    if not db.is_initialized():
        return "0.0"

    result = db.get_meta("schema_version")
    return result or "unknown"


def get_table_info(db: "PostgresDB", table_name: str) -> list[dict]:
    """Get column information for a table."""
    return db.fetchall(
        """
        SELECT column_name, data_type, is_nullable
        FROM information_schema.columns
        WHERE table_name = %s AND table_schema = 'public'
        ORDER BY ordinal_position
        """,
        (table_name,),
    )


def list_tables(db: "PostgresDB") -> list[str]:
    """List all tables in the database."""
    rows = db.fetchall(
        """
        SELECT table_name as name
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
        ORDER BY table_name
        """
    )
    return [row["name"] for row in rows]


def get_table_counts(db: "PostgresDB") -> dict[str, int]:
    """Get row counts for all tables."""
    tables = list_tables(db)
    counts = {}

    for table in tables:
        result = db.fetchone(f"SELECT COUNT(*) as count FROM {table}")
        counts[table] = result["count"] if result else 0

    return counts
