"""
Database connection manager for the stats database.

Provides a unified interface for database operations with support for
both read-only (production) and read-write (development) modes.
"""

from __future__ import annotations

import os
import sqlite3
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator, Optional

# Database file location
# Path: backend/app/statsdb/connection.py → Scoracle/ → statsdb/
DEFAULT_DB_DIR = Path(__file__).parent.parent.parent.parent / "statsdb"
DEFAULT_DB_PATH = DEFAULT_DB_DIR / "stats.sqlite"


class StatsDB:
    """
    Stats database connection manager.

    Provides context-managed access to the stats SQLite database with
    automatic connection pooling and read-only mode support for production.
    """

    def __init__(
        self,
        db_path: Optional[Path] = None,
        read_only: bool = False,
    ):
        """
        Initialize the database connection manager.

        Args:
            db_path: Path to the SQLite database file. Defaults to instance/statsdb/stats.sqlite
            read_only: If True, opens database in read-only mode (for production)
        """
        self.db_path = db_path or DEFAULT_DB_PATH
        self.read_only = read_only
        self._connection: Optional[sqlite3.Connection] = None

    def _get_connection_uri(self) -> str:
        """Build the SQLite connection URI."""
        uri = f"file:{self.db_path}"
        if self.read_only:
            uri += "?mode=ro"
        return uri

    def _create_connection(self) -> sqlite3.Connection:
        """Create a new database connection."""
        # Ensure directory exists for write mode
        if not self.read_only:
            self.db_path.parent.mkdir(parents=True, exist_ok=True)

        conn = sqlite3.connect(
            self._get_connection_uri(),
            uri=True,
            check_same_thread=False,
            isolation_level=None,  # Autocommit mode
        )

        # Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON")

        # Optimize for read performance
        conn.execute("PRAGMA journal_mode = WAL")
        conn.execute("PRAGMA synchronous = NORMAL")
        conn.execute("PRAGMA cache_size = -64000")  # 64MB cache

        # Return dicts instead of tuples
        conn.row_factory = sqlite3.Row

        return conn

    @property
    def connection(self) -> sqlite3.Connection:
        """Get or create the database connection."""
        if self._connection is None:
            self._connection = self._create_connection()
        return self._connection

    def close(self) -> None:
        """Close the database connection."""
        if self._connection is not None:
            self._connection.close()
            self._connection = None

    @contextmanager
    def cursor(self) -> Iterator[sqlite3.Cursor]:
        """Get a cursor for executing queries."""
        cur = self.connection.cursor()
        try:
            yield cur
        finally:
            cur.close()

    @contextmanager
    def transaction(self) -> Iterator[sqlite3.Cursor]:
        """
        Execute queries within a transaction.

        Automatically commits on success, rolls back on failure.
        """
        conn = self.connection
        cur = conn.cursor()
        try:
            cur.execute("BEGIN")
            yield cur
            conn.commit()
        except Exception:
            conn.rollback()
            raise
        finally:
            cur.close()

    def execute(self, query: str, params: tuple = ()) -> sqlite3.Cursor:
        """Execute a single query."""
        return self.connection.execute(query, params)

    def executemany(self, query: str, params_list: list[tuple]) -> sqlite3.Cursor:
        """Execute a query with multiple parameter sets."""
        return self.connection.executemany(query, params_list)

    def fetchone(self, query: str, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        cur = self.execute(query, params)
        row = cur.fetchone()
        return dict(row) if row else None

    def fetchall(self, query: str, params: tuple = ()) -> list[dict[str, Any]]:
        """Execute a query and fetch all results as dicts."""
        cur = self.execute(query, params)
        return [dict(row) for row in cur.fetchall()]

    def exists(self) -> bool:
        """Check if the database file exists."""
        return self.db_path.exists()

    def is_initialized(self) -> bool:
        """Check if the database has been initialized with schema."""
        if not self.exists():
            return False

        try:
            result = self.fetchone(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='meta'"
            )
            return result is not None
        except Exception:
            return False

    # =========================================================================
    # Query Methods
    # =========================================================================

    def get_season_id(self, sport_id: str, season_year: int) -> Optional[int]:
        """Get the season ID for a sport and year."""
        result = self.fetchone(
            "SELECT id FROM seasons WHERE sport_id = ? AND season_year = ?",
            (sport_id, season_year),
        )
        return result["id"] if result else None

    def get_current_season(self, sport_id: str) -> Optional[dict[str, Any]]:
        """Get the current season for a sport."""
        return self.fetchone(
            "SELECT * FROM seasons WHERE sport_id = ? AND is_current = 1",
            (sport_id,),
        )

    def get_player(self, player_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get player info by ID."""
        return self.fetchone(
            "SELECT * FROM players WHERE id = ? AND sport_id = ?",
            (player_id, sport_id),
        )

    def get_team(self, team_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get team info by ID."""
        return self.fetchone(
            "SELECT * FROM teams WHERE id = ? AND sport_id = ?",
            (team_id, sport_id),
        )

    def get_player_stats(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get player statistics for a given season.

        Args:
            player_id: API-Sports player ID
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season_year: Season year

        Returns:
            Dict of stats or None if not found
        """
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        # Sport-specific table mapping
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": None,  # NFL has multiple tables per position
            "FOOTBALL": "football_player_stats",
        }

        if sport_id == "NFL":
            # For NFL, try to get stats from all position tables
            return self._get_nfl_player_stats(player_id, season_id)

        table = table_map.get(sport_id)
        if not table:
            return None

        return self.fetchone(
            f"SELECT * FROM {table} WHERE player_id = ? AND season_id = ?",
            (player_id, season_id),
        )

    def _get_nfl_player_stats(
        self,
        player_id: int,
        season_id: int,
    ) -> dict[str, Any]:
        """Get NFL player stats from all position-specific tables."""
        result: dict[str, Any] = {"player_id": player_id, "season_id": season_id}

        tables = [
            "nfl_player_passing",
            "nfl_player_rushing",
            "nfl_player_receiving",
            "nfl_player_defense",
            "nfl_player_kicking",
        ]

        for table in tables:
            stats = self.fetchone(
                f"SELECT * FROM {table} WHERE player_id = ? AND season_id = ?",
                (player_id, season_id),
            )
            if stats:
                # Add stats with table prefix to avoid collisions
                prefix = table.replace("nfl_player_", "")
                for key, value in stats.items():
                    if key not in ("id", "player_id", "season_id", "team_id", "updated_at"):
                        result[f"{prefix}_{key}"] = value

        return result if len(result) > 2 else None

    def get_team_stats(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """Get team statistics for a given season."""
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_team_stats",
            "NFL": "nfl_team_stats",
            "FOOTBALL": "football_team_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return None

        return self.fetchone(
            f"SELECT * FROM {table} WHERE team_id = ? AND season_id = ?",
            (team_id, season_id),
        )

    def get_percentiles(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        season_year: int,
    ) -> list[dict[str, Any]]:
        """Get cached percentiles for an entity."""
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        return self.fetchall(
            """
            SELECT stat_category, stat_value, percentile, rank, sample_size, comparison_group
            FROM percentile_cache
            WHERE entity_type = ? AND entity_id = ? AND sport_id = ? AND season_id = ?
            ORDER BY stat_category
            """,
            (entity_type, entity_id, sport_id, season_id),
        )

    def get_meta(self, key: str) -> Optional[str]:
        """Get a metadata value."""
        result = self.fetchone("SELECT value FROM meta WHERE key = ?", (key,))
        return result["value"] if result else None

    def set_meta(self, key: str, value: str) -> None:
        """Set a metadata value."""
        import time

        self.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
            """,
            (key, value, int(time.time())),
        )


# Global instance
_stats_db: Optional[StatsDB] = None


def get_stats_db(read_only: bool = True) -> StatsDB:
    """
    Get the global stats database instance.

    Args:
        read_only: If True (default), opens in read-only mode for production.

    Returns:
        StatsDB instance
    """
    global _stats_db

    # Determine read-only mode from environment if not explicitly set
    env_read_only = os.getenv("STATSDB_READ_ONLY", "true").lower() in ("true", "1", "yes")

    if _stats_db is None:
        _stats_db = StatsDB(read_only=env_read_only if read_only else False)

    return _stats_db


def close_stats_db() -> None:
    """Close the global stats database connection."""
    global _stats_db
    if _stats_db is not None:
        _stats_db.close()
        _stats_db = None
