"""
Database schema management for the stats database.

Handles initialization, migrations, and schema version tracking.
"""

from __future__ import annotations

import logging
import time
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .connection import StatsDB

logger = logging.getLogger(__name__)

MIGRATIONS_DIR = Path(__file__).parent / "migrations"


def get_migration_files() -> list[Path]:
    """Get all SQL migration files in order."""
    if not MIGRATIONS_DIR.exists():
        return []

    files = sorted(MIGRATIONS_DIR.glob("*.sql"))
    return files


def run_migrations(db: "StatsDB", force: bool = False) -> int:
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
                "SELECT value FROM meta WHERE key = ?",
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
            db.connection.executescript(sql)

            # Record migration as applied
            db.execute(
                """
                INSERT OR REPLACE INTO meta (key, value, updated_at)
                VALUES (?, ?, ?)
                """,
                (f"migration_{migration_name}", "applied", int(time.time())),
            )

            applied += 1
            logger.info("Successfully applied migration: %s", migration_name)

        except Exception as e:
            logger.error("Failed to apply migration %s: %s", migration_name, e)
            raise

    return applied


def init_database(db: "StatsDB") -> None:
    """
    Initialize the database with the full schema.

    This runs all migrations in order to set up the complete schema.

    Args:
        db: Database connection (must be in write mode)
    """
    if db.read_only:
        raise RuntimeError("Cannot initialize database in read-only mode")

    logger.info("Initializing stats database at %s", db.db_path)

    # Run all migrations
    applied = run_migrations(db)

    logger.info("Database initialized with %d migrations", applied)


def get_schema_version(db: "StatsDB") -> str:
    """Get the current schema version."""
    if not db.is_initialized():
        return "0.0"

    result = db.get_meta("schema_version")
    return result or "unknown"


def get_table_info(db: "StatsDB", table_name: str) -> list[dict]:
    """Get column information for a table."""
    return db.fetchall(f"PRAGMA table_info({table_name})")


def list_tables(db: "StatsDB") -> list[str]:
    """List all tables in the database."""
    rows = db.fetchall(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    )
    return [row["name"] for row in rows]


def get_table_counts(db: "StatsDB") -> dict[str, int]:
    """Get row counts for all tables."""
    tables = list_tables(db)
    counts = {}

    for table in tables:
        if table.startswith("sqlite_"):
            continue
        result = db.fetchone(f"SELECT COUNT(*) as count FROM {table}")
        counts[table] = result["count"] if result else 0

    return counts
