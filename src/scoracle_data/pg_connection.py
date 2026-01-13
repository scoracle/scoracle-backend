"""
PostgreSQL connection manager for Neon.

Provides a unified interface matching StatsDB but using PostgreSQL via psycopg3.
Supports connection pooling for production workloads.
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from typing import Any, Iterator, Optional

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import ConnectionPool


class PostgresDB:
    """
    PostgreSQL database connection manager for Neon.

    Provides the same interface as StatsDB for drop-in compatibility,
    with connection pooling for production performance.
    """

    def __init__(
        self,
        connection_string: Optional[str] = None,
        min_pool_size: int = 2,
        max_pool_size: Optional[int] = None,
    ):
        """
        Initialize the PostgreSQL connection manager.

        Args:
            connection_string: PostgreSQL connection URL. Defaults to DATABASE_URL env var.
            min_pool_size: Minimum connections to keep in pool.
            max_pool_size: Maximum connections in pool. Defaults to DATABASE_POOL_SIZE env var or 10.
        """
        self.connection_string = connection_string or os.environ.get("DATABASE_URL") or os.environ.get("NEON_DATABASE_URL")
        if not self.connection_string:
            raise ValueError(
                "DATABASE_URL or NEON_DATABASE_URL environment variable required or connection_string must be provided"
            )

        self._max_pool_size = max_pool_size or int(os.environ.get("DATABASE_POOL_SIZE", 10))
        self._min_pool_size = min_pool_size

        # Initialize connection pool
        self._pool = ConnectionPool(
            self.connection_string,
            min_size=self._min_pool_size,
            max_size=self._max_pool_size,
            kwargs={"row_factory": dict_row},
        )

    def open(self) -> None:
        """
        Explicitly open the connection pool.

        This establishes the minimum number of connections immediately,
        rather than waiting for the first request (lazy initialization).
        Call this at application startup to ensure the pool is ready.
        """
        self._pool.open()

    @contextmanager
    def get_connection(self) -> Iterator[psycopg.Connection]:
        """Get a connection from the pool."""
        with self._pool.connection() as conn:
            yield conn

    @contextmanager
    def cursor(self) -> Iterator[psycopg.Cursor]:
        """Get a cursor for executing queries."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                yield cur

    @contextmanager
    def transaction(self) -> Iterator[psycopg.Connection]:
        """
        Execute queries within a transaction.

        Automatically commits on success, rolls back on failure.
        """
        with self.get_connection() as conn:
            try:
                yield conn
                conn.commit()
            except Exception:
                conn.rollback()
                raise

    def execute(self, query: str, params: tuple = ()) -> None:
        """Execute a single query without returning results."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
            conn.commit()

    def executemany(self, query: str, params_list: list[tuple]) -> None:
        """Execute a query with multiple parameter sets."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.executemany(query, params_list)
            conn.commit()

    def executescript(self, sql: str) -> None:
        """Execute a SQL script (multiple statements)."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(sql)
            conn.commit()

    def fetchone(self, query: str, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                row = cur.fetchone()
                return dict(row) if row else None

    def fetchall(self, query: str, params: tuple = ()) -> list[dict[str, Any]]:
        """Execute a query and fetch all results as dicts."""
        with self.get_connection() as conn:
            with conn.cursor() as cur:
                cur.execute(query, params)
                return [dict(row) for row in cur.fetchall()]

    def close(self) -> None:
        """Close the connection pool."""
        self._pool.close()

    def is_initialized(self) -> bool:
        """Check if the database has been initialized with schema."""
        try:
            result = self.fetchone(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'meta') as exists"
            )
            return result["exists"] if result else False
        except Exception:
            return False

    # =========================================================================
    # Query Methods (matching StatsDB interface)
    # =========================================================================

    def get_season_id(self, sport_id: str, season_year: int) -> Optional[int]:
        """Get the season ID for a sport and year."""
        result = self.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year),
        )
        return result["id"] if result else None

    def get_current_season(self, sport_id: str) -> Optional[dict[str, Any]]:
        """Get the current season for a sport."""
        return self.fetchone(
            "SELECT * FROM seasons WHERE sport_id = %s AND is_current = true",
            (sport_id,),
        )

    def get_player(self, player_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get player info by ID."""
        return self.fetchone(
            "SELECT * FROM players WHERE id = %s AND sport_id = %s",
            (player_id, sport_id),
        )

    def get_team(self, team_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get team info by ID."""
        return self.fetchone(
            "SELECT * FROM teams WHERE id = %s AND sport_id = %s",
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

        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",  # Unified table in PostgreSQL
            "FOOTBALL": "football_player_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return None

        return self.fetchone(
            f"SELECT * FROM {table} WHERE player_id = %s AND season_id = %s",
            (player_id, season_id),
        )

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
            f"SELECT * FROM {table} WHERE team_id = %s AND season_id = %s",
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
            WHERE entity_type = %s AND entity_id = %s AND sport_id = %s AND season_id = %s
            ORDER BY stat_category
            """,
            (entity_type, entity_id, sport_id, season_id),
        )

    # =========================================================================
    # Optimized Query Methods (for API performance)
    # =========================================================================

    def get_team_profile_optimized(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete team profile in a single optimized query.

        Combines team info, stats, and percentiles using JOINs for better performance.

        Args:
            team_id: Team ID
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            Dict with team, stats, and percentiles, or None if not found
        """
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        # Determine stats table based on sport
        table_map = {
            "NBA": "nba_team_stats",
            "NFL": "nfl_team_stats",
            "FOOTBALL": "football_team_stats",
        }
        stats_table = table_map.get(sport_id)
        if not stats_table:
            return None

        # Single query with JOINs and aggregation
        query = f"""
            SELECT
                json_build_object(
                    'id', t.id,
                    'sport_id', t.sport_id,
                    'league_id', t.league_id,
                    'name', t.name,
                    'abbreviation', t.abbreviation,
                    'logo_url', t.logo_url,
                    'conference', t.conference,
                    'division', t.division,
                    'country', t.country,
                    'city', t.city,
                    'founded', t.founded,
                    'venue_name', t.venue_name,
                    'venue_city', t.venue_city,
                    'venue_capacity', t.venue_capacity,
                    'venue_surface', t.venue_surface,
                    'venue_image', t.venue_image,
                    'profile_fetched_at', t.profile_fetched_at,
                    'is_active', t.is_active
                ) as team,
                row_to_json(s.*) as stats,
                COALESCE(
                    json_agg(
                        json_build_object(
                            'stat_category', p.stat_category,
                            'stat_value', p.stat_value,
                            'percentile', p.percentile,
                            'rank', p.rank,
                            'sample_size', p.sample_size,
                            'comparison_group', p.comparison_group
                        )
                        ORDER BY p.stat_category
                    ) FILTER (WHERE p.id IS NOT NULL),
                    '[]'::json
                ) as percentiles
            FROM teams t
            LEFT JOIN {stats_table} s ON s.team_id = t.id AND s.season_id = %s
            LEFT JOIN percentile_cache p ON p.entity_type = 'team'
                AND p.entity_id = t.id
                AND p.sport_id = t.sport_id
                AND p.season_id = %s
            WHERE t.id = %s AND t.sport_id = %s
            GROUP BY t.id, s.*
        """

        result = self.fetchone(query, (season_id, season_id, team_id, sport_id))

        if not result:
            return None

        # Parse JSON fields back to dicts/lists
        import json

        return {
            "team": json.loads(result["team"]) if isinstance(result["team"], str) else result["team"],
            "stats": json.loads(result["stats"]) if isinstance(result["stats"], str) else result["stats"],
            "percentiles": json.loads(result["percentiles"]) if isinstance(result["percentiles"], str) else result["percentiles"],
        }

    def get_player_profile_optimized(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete player profile in a single optimized query.

        Combines player info, team info, stats, and percentiles using JOINs for better performance.

        Args:
            player_id: Player ID
            sport_id: Sport identifier
            season_year: Season year

        Returns:
            Dict with player, team, stats, and percentiles, or None if not found
        """
        season_id = self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        # Determine stats table based on sport
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",
            "FOOTBALL": "football_player_stats",
        }
        stats_table = table_map.get(sport_id)
        if not stats_table:
            return None

        # Single query with JOINs and aggregation
        query = f"""
            SELECT
                json_build_object(
                    'id', pl.id,
                    'sport_id', pl.sport_id,
                    'first_name', pl.first_name,
                    'last_name', pl.last_name,
                    'full_name', pl.full_name,
                    'position', pl.position,
                    'position_group', pl.position_group,
                    'nationality', pl.nationality,
                    'birth_date', pl.birth_date,
                    'birth_place', pl.birth_place,
                    'height_inches', pl.height_inches,
                    'weight_lbs', pl.weight_lbs,
                    'photo_url', pl.photo_url,
                    'current_team_id', pl.current_team_id,
                    'current_league_id', pl.current_league_id,
                    'jersey_number', pl.jersey_number,
                    'college', pl.college,
                    'experience_years', pl.experience_years,
                    'profile_fetched_at', pl.profile_fetched_at,
                    'is_active', pl.is_active
                ) as player,
                CASE WHEN t.id IS NOT NULL THEN
                    json_build_object(
                        'id', t.id,
                        'sport_id', t.sport_id,
                        'league_id', t.league_id,
                        'name', t.name,
                        'abbreviation', t.abbreviation,
                        'logo_url', t.logo_url,
                        'conference', t.conference,
                        'division', t.division,
                        'country', t.country,
                        'city', t.city,
                        'founded', t.founded,
                        'venue_name', t.venue_name,
                        'venue_city', t.venue_city,
                        'venue_capacity', t.venue_capacity,
                        'venue_surface', t.venue_surface,
                        'venue_image', t.venue_image,
                        'profile_fetched_at', t.profile_fetched_at,
                        'is_active', t.is_active
                    )
                ELSE NULL END as team,
                row_to_json(s.*) as stats,
                COALESCE(
                    json_agg(
                        json_build_object(
                            'stat_category', p.stat_category,
                            'stat_value', p.stat_value,
                            'percentile', p.percentile,
                            'rank', p.rank,
                            'sample_size', p.sample_size,
                            'comparison_group', p.comparison_group
                        )
                        ORDER BY p.stat_category
                    ) FILTER (WHERE p.id IS NOT NULL),
                    '[]'::json
                ) as percentiles
            FROM players pl
            LEFT JOIN teams t ON pl.current_team_id = t.id AND t.sport_id = pl.sport_id
            LEFT JOIN {stats_table} s ON s.player_id = pl.id AND s.season_id = %s
            LEFT JOIN percentile_cache p ON p.entity_type = 'player'
                AND p.entity_id = pl.id
                AND p.sport_id = pl.sport_id
                AND p.season_id = %s
            WHERE pl.id = %s AND pl.sport_id = %s
            GROUP BY pl.id, t.id, s.*
        """

        result = self.fetchone(query, (season_id, season_id, player_id, sport_id))

        if not result:
            return None

        # Parse JSON fields back to dicts/lists
        import json

        return {
            "player": json.loads(result["player"]) if isinstance(result["player"], str) else result["player"],
            "team": json.loads(result["team"]) if isinstance(result["team"], str) else result["team"] if result["team"] is not None else None,
            "stats": json.loads(result["stats"]) if isinstance(result["stats"], str) else result["stats"],
            "percentiles": json.loads(result["percentiles"]) if isinstance(result["percentiles"], str) else result["percentiles"],
        }

    def get_meta(self, key: str) -> Optional[str]:
        """Get a metadata value."""
        result = self.fetchone("SELECT value FROM meta WHERE key = %s", (key,))
        return result["value"] if result else None

    def set_meta(self, key: str, value: str) -> None:
        """Set a metadata value."""
        self.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES (%s, %s, NOW())
            ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            """,
            (key, value),
        )


# Global instance
_postgres_db: Optional[PostgresDB] = None


def get_postgres_db() -> PostgresDB:
    """
    Get the global PostgreSQL database instance.

    Returns:
        PostgresDB instance
    """
    global _postgres_db

    if _postgres_db is None:
        _postgres_db = PostgresDB()

    return _postgres_db


def close_postgres_db() -> None:
    """Close the global PostgreSQL database connection."""
    global _postgres_db
    if _postgres_db is not None:
        _postgres_db.close()
        _postgres_db = None


# =========================================================================
# Database Factory - Choose between SQLite and PostgreSQL
# =========================================================================


def get_db(use_postgres: Optional[bool] = None) -> PostgresDB:
    """
    Get a database instance based on configuration.

    Args:
        use_postgres: Force PostgreSQL if True, SQLite if False.
                     If None, uses USE_POSTGRESQL env var (defaults to True).

    Returns:
        Database instance (PostgresDB for Neon)
    """
    if use_postgres is None:
        use_postgres = os.environ.get("USE_POSTGRESQL", "true").lower() in ("true", "1", "yes")

    if use_postgres:
        return get_postgres_db()
    else:
        # Fall back to SQLite for local development
        from .connection import get_stats_db
        return get_stats_db(read_only=False)
