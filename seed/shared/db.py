"""Database connection pool using psycopg v3."""

from __future__ import annotations

import logging
from contextlib import contextmanager
from typing import TYPE_CHECKING, Generator

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import ConnectionPool

if TYPE_CHECKING:
    from .config import Config

logger = logging.getLogger(__name__)


def create_pool(cfg: "Config") -> ConnectionPool:
    """Create a psycopg connection pool."""
    return ConnectionPool(
        cfg.database_url,
        min_size=cfg.db_pool_min,
        max_size=cfg.db_pool_max,
        kwargs={"row_factory": dict_row},
    )


@contextmanager
def get_conn(pool: ConnectionPool) -> Generator[psycopg.Connection, None, None]:
    """Get a connection from the pool with auto-commit on success."""
    with pool.connection() as conn:
        yield conn


def check_connectivity(pool: ConnectionPool) -> bool:
    """Verify database connectivity with a simple query."""
    try:
        with get_conn(pool) as conn:
            conn.execute("SELECT 1")
        return True
    except Exception as exc:
        logger.error("Database connectivity check failed: %s", exc)
        return False


# ------------------------------------------------------------------
# Provider resolvers (football / SportMonks)
# ------------------------------------------------------------------


def resolve_provider_season_id(
    conn: psycopg.Connection, league_id: int, season_year: int
) -> int | None:
    """Look up SportMonks season ID from the provider_seasons table."""
    row = conn.execute(
        "SELECT resolve_provider_season_id(%s, %s)", (league_id, season_year)
    ).fetchone()
    if row:
        val = list(row.values())[0]
        return val
    return None


def resolve_sm_league_id(
    conn: psycopg.Connection, league_id: int
) -> tuple[int | None, str]:
    """Look up SportMonks league ID and name from leagues table."""
    row = conn.execute(
        "SELECT sportmonks_id, name FROM leagues WHERE id = %s", (league_id,)
    ).fetchone()
    if row:
        return row.get("sportmonks_id"), row.get("name", "")
    return None, ""


def get_football_league_ids(
    conn: psycopg.Connection, season_year: int, provider: str = "sportmonks"
) -> list[int]:
    """Return every football league_id with a provider_seasons row for the season."""
    rows = conn.execute(
        """
        SELECT ps.league_id
        FROM provider_seasons ps
        JOIN leagues l ON l.id = ps.league_id
        WHERE ps.season_year = %s
          AND ps.provider = %s
          AND l.sport = 'FOOTBALL'
        ORDER BY ps.league_id
        """,
        (season_year, provider),
    ).fetchall()
    return [r["league_id"] for r in rows]
