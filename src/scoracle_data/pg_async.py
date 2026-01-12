"""
Async PostgreSQL connection manager for high-performance API serving.

Uses psycopg3's native async support for non-blocking database operations,
allowing FastAPI to handle more concurrent requests.
"""

from __future__ import annotations

import json
import os
from contextlib import asynccontextmanager
from typing import Any, AsyncIterator, Optional

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import AsyncConnectionPool


class AsyncPostgresDB:
    """
    Async PostgreSQL database connection manager.

    Provides non-blocking database operations using psycopg3's async API
    with connection pooling for optimal performance.
    """

    def __init__(
        self,
        connection_string: Optional[str] = None,
        min_pool_size: int = 2,
        max_pool_size: Optional[int] = None,
    ):
        """
        Initialize the async PostgreSQL connection manager.

        Args:
            connection_string: PostgreSQL connection URL. Defaults to DATABASE_URL env var.
            min_pool_size: Minimum connections to keep in pool.
            max_pool_size: Maximum connections in pool.
        """
        self.connection_string = connection_string or os.environ.get("DATABASE_URL") or os.environ.get("NEON_DATABASE_URL")
        if not self.connection_string:
            raise ValueError(
                "DATABASE_URL or NEON_DATABASE_URL environment variable required or connection_string must be provided"
            )

        self._max_pool_size = max_pool_size or int(os.environ.get("DATABASE_POOL_SIZE", 10))
        self._min_pool_size = min_pool_size
        self._pool: Optional[AsyncConnectionPool] = None

    async def initialize(self) -> None:
        """Initialize the connection pool. Call this at app startup."""
        if self._pool is None:
            self._pool = AsyncConnectionPool(
                self.connection_string,
                min_size=self._min_pool_size,
                max_size=self._max_pool_size,
                kwargs={"row_factory": dict_row},
            )
            await self._pool.open()

    async def close(self) -> None:
        """Close the connection pool. Call this at app shutdown."""
        if self._pool is not None:
            await self._pool.close()
            self._pool = None

    @asynccontextmanager
    async def get_connection(self) -> AsyncIterator[psycopg.AsyncConnection]:
        """Get a connection from the pool."""
        if self._pool is None:
            await self.initialize()
        async with self._pool.connection() as conn:
            yield conn

    @asynccontextmanager
    async def cursor(self) -> AsyncIterator[psycopg.AsyncCursor]:
        """Get a cursor for executing queries."""
        async with self.get_connection() as conn:
            async with conn.cursor() as cur:
                yield cur

    async def execute(self, query: str, params: tuple = ()) -> None:
        """Execute a single query without returning results."""
        async with self.get_connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
            await conn.commit()

    async def fetchone(self, query: str, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Execute a query and fetch one result as a dict."""
        async with self.get_connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                row = await cur.fetchone()
                return dict(row) if row else None

    async def fetchall(self, query: str, params: tuple = ()) -> list[dict[str, Any]]:
        """Execute a query and fetch all results as dicts."""
        async with self.get_connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                rows = await cur.fetchall()
                return [dict(row) for row in rows]

    # =========================================================================
    # Async Query Methods
    # =========================================================================

    async def get_season_id(self, sport_id: str, season_year: int) -> Optional[int]:
        """Get the season ID for a sport and year."""
        result = await self.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year),
        )
        return result["id"] if result else None

    async def get_current_season(self, sport_id: str) -> Optional[dict[str, Any]]:
        """Get the current season for a sport."""
        return await self.fetchone(
            "SELECT * FROM seasons WHERE sport_id = %s AND is_current = true",
            (sport_id,),
        )

    async def get_player(self, player_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get player info by ID."""
        return await self.fetchone(
            "SELECT * FROM players WHERE id = %s AND sport_id = %s",
            (player_id, sport_id),
        )

    async def get_team(self, team_id: int, sport_id: str) -> Optional[dict[str, Any]]:
        """Get team info by ID."""
        return await self.fetchone(
            "SELECT * FROM teams WHERE id = %s AND sport_id = %s",
            (team_id, sport_id),
        )

    async def get_player_stats(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """Get player statistics for a given season."""
        season_id = await self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",
            "FOOTBALL": "football_player_stats",
        }

        table = table_map.get(sport_id)
        if not table:
            return None

        return await self.fetchone(
            f"SELECT * FROM {table} WHERE player_id = %s AND season_id = %s",
            (player_id, season_id),
        )

    async def get_team_stats(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """Get team statistics for a given season."""
        season_id = await self.get_season_id(sport_id, season_year)
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

        return await self.fetchone(
            f"SELECT * FROM {table} WHERE team_id = %s AND season_id = %s",
            (team_id, season_id),
        )

    async def get_percentiles(
        self,
        entity_type: str,
        entity_id: int,
        sport_id: str,
        season_year: int,
    ) -> list[dict[str, Any]]:
        """Get cached percentiles for an entity."""
        season_id = await self.get_season_id(sport_id, season_year)
        if not season_id:
            return []

        return await self.fetchall(
            """
            SELECT stat_category, stat_value, percentile, rank, sample_size, comparison_group
            FROM percentile_cache
            WHERE entity_type = %s AND entity_id = %s AND sport_id = %s AND season_id = %s
            ORDER BY stat_category
            """,
            (entity_type, entity_id, sport_id, season_id),
        )

    # =========================================================================
    # Optimized Async Query Methods (single-query JOINs)
    # =========================================================================

    async def get_team_profile_optimized(
        self,
        team_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete team profile in a single optimized query.

        Combines team info, stats, and percentiles using JOINs.
        """
        season_id = await self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_team_stats",
            "NFL": "nfl_team_stats",
            "FOOTBALL": "football_team_stats",
        }
        stats_table = table_map.get(sport_id)
        if not stats_table:
            return None

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

        result = await self.fetchone(query, (season_id, season_id, team_id, sport_id))

        if not result:
            return None

        return {
            "team": json.loads(result["team"]) if isinstance(result["team"], str) else result["team"],
            "stats": json.loads(result["stats"]) if isinstance(result["stats"], str) else result["stats"],
            "percentiles": json.loads(result["percentiles"]) if isinstance(result["percentiles"], str) else result["percentiles"],
        }

    async def get_player_profile_optimized(
        self,
        player_id: int,
        sport_id: str,
        season_year: int,
    ) -> Optional[dict[str, Any]]:
        """
        Get complete player profile in a single optimized query.

        Combines player info, team info, stats, and percentiles using JOINs.
        """
        season_id = await self.get_season_id(sport_id, season_year)
        if not season_id:
            return None

        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",
            "FOOTBALL": "football_player_stats",
        }
        stats_table = table_map.get(sport_id)
        if not stats_table:
            return None

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

        result = await self.fetchone(query, (season_id, season_id, player_id, sport_id))

        if not result:
            return None

        return {
            "player": json.loads(result["player"]) if isinstance(result["player"], str) else result["player"],
            "team": json.loads(result["team"]) if isinstance(result["team"], str) else result["team"] if result["team"] is not None else None,
            "stats": json.loads(result["stats"]) if isinstance(result["stats"], str) else result["stats"],
            "percentiles": json.loads(result["percentiles"]) if isinstance(result["percentiles"], str) else result["percentiles"],
        }

    # =========================================================================
    # Bulk Query Methods (for cache warming)
    # =========================================================================

    async def get_popular_entities(
        self,
        sport_id: str,
        entity_type: str = "player",
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        """
        Get most popular entities for cache warming.

        For players: returns those with most games played
        For teams: returns all active teams
        """
        if entity_type == "player":
            # Get players with stats (indicates they're active/relevant)
            table_map = {
                "NBA": "nba_player_stats",
                "NFL": "nfl_player_stats",
                "FOOTBALL": "football_player_stats",
            }
            stats_table = table_map.get(sport_id)
            if not stats_table:
                return []

            return await self.fetchall(
                f"""
                SELECT DISTINCT p.id, p.sport_id, p.full_name
                FROM players p
                JOIN {stats_table} s ON s.player_id = p.id
                WHERE p.sport_id = %s AND p.is_active = true
                ORDER BY p.id
                LIMIT %s
                """,
                (sport_id, limit),
            )
        else:
            # Get all active teams
            return await self.fetchall(
                """
                SELECT id, sport_id, name
                FROM teams
                WHERE sport_id = %s AND is_active = true
                ORDER BY id
                LIMIT %s
                """,
                (sport_id, limit),
            )

    async def get_all_seasons(self, sport_id: str) -> list[dict[str, Any]]:
        """Get all seasons for a sport."""
        return await self.fetchall(
            "SELECT * FROM seasons WHERE sport_id = %s ORDER BY season_year DESC",
            (sport_id,),
        )


# Global async instance
_async_db: Optional[AsyncPostgresDB] = None


async def get_async_db() -> AsyncPostgresDB:
    """
    Get the global async PostgreSQL database instance.

    Returns:
        AsyncPostgresDB instance
    """
    global _async_db

    if _async_db is None:
        _async_db = AsyncPostgresDB()
        await _async_db.initialize()

    return _async_db


async def close_async_db() -> None:
    """Close the global async PostgreSQL database connection."""
    global _async_db
    if _async_db is not None:
        await _async_db.close()
        _async_db = None
