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
