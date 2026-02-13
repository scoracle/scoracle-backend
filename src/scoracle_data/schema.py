"""
Database schema management for the stats database.

Applies a single consolidated schema.sql and tracks the version in the meta table.
PostgreSQL-only implementation.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .pg_connection import PostgresDB

logger = logging.getLogger(__name__)

SCHEMA_FILE = Path(__file__).parent / "schema.sql"

# Must match the version comment at the top of schema.sql
SCHEMA_VERSION = "6.0"


def run_migrations(db: "PostgresDB", force: bool = False) -> int:
    """
    Apply the consolidated schema if not already at the current version.

    Args:
        db: Database connection
        force: If True, re-apply even if version matches

    Returns:
        1 if schema was applied, 0 if already up to date
    """
    if not SCHEMA_FILE.exists():
        logger.error("Schema file not found: %s", SCHEMA_FILE)
        return 0

    # Check current version
    if not force and db.is_initialized():
        existing = db.fetchone(
            "SELECT value FROM meta WHERE key = %s",
            ("schema_version",),
        )
        if existing and existing["value"] == SCHEMA_VERSION:
            logger.info("Schema already at v%s — nothing to apply", SCHEMA_VERSION)
            return 0

    logger.info("Applying schema v%s from %s", SCHEMA_VERSION, SCHEMA_FILE.name)

    sql = SCHEMA_FILE.read_text()

    try:
        # Escape literal '%' so psycopg3 doesn't treat them as placeholders
        # (e.g., 'Field Goal %' in stat definition names).
        db.execute(sql.replace("%", "%%"))

        # Record the version
        db.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES (%s, %s, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            """,
            ("schema_version", SCHEMA_VERSION),
        )

        logger.info("Schema v%s applied successfully", SCHEMA_VERSION)
        return 1

    except Exception as e:
        logger.error("Failed to apply schema v%s: %s", SCHEMA_VERSION, e)
        raise


def init_database(db: "PostgresDB") -> None:
    """
    Initialize the database with the full schema.

    Args:
        db: Database connection
    """
    logger.info("Initializing stats database...")
    applied = run_migrations(db)
    if applied:
        logger.info("Database initialized with schema v%s", SCHEMA_VERSION)
    else:
        logger.info("Database already initialized at schema v%s", SCHEMA_VERSION)


def get_schema_version(db: "PostgresDB") -> str:
    """Get the current schema version."""
    if not db.is_initialized():
        return "0.0"

    result = db.get_meta("schema_version")
    return result or "unknown"


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
