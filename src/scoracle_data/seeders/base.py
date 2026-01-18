"""
Base seeder class for stats database population.

All sport-specific seeders inherit from this class and implement
the abstract methods for data transformation.

Two-Phase Seeding Architecture:
  Phase 1: DISCOVERY - Fetch rosters, identify new/changed entities
  Phase 2: PROFILE FETCH - Fetch full profiles for new entities only
  Phase 3: STATS UPDATE - Update statistics for all entities

Parallelization:
  Uses asyncio.gather with configurable batch sizes to parallelize
  API calls while respecting rate limits.

Sport-Specific Tables (v4.0):
  Player profiles are stored in sport-specific tables:
  - nba_player_profiles, nfl_player_profiles, football_player_profiles
  - nba_team_profiles, nfl_team_profiles, football_team_profiles

  This prevents cross-sport ID collisions (e.g., API-Sports reuses IDs across sports).
"""

from __future__ import annotations

import asyncio
import logging
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Callable, Coroutine, Optional, TypeVar

from ..api.types import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

if TYPE_CHECKING:
    from ..connection import StatsDB
    from ...services.apisports import ApiSportsService

logger = logging.getLogger(__name__)

T = TypeVar("T")


async def run_parallel_batches(
    items: list[T],
    async_fn: Callable[[T], Coroutine[Any, Any, Any]],
    batch_size: int = 10,
    delay_between_batches: float = 0.5,
) -> list[tuple[T, Any, Exception | None]]:
    """
    Execute async function on items in parallel batches.

    This respects API rate limits by:
    1. Processing items in batches of `batch_size`
    2. Adding a delay between batches

    Args:
        items: List of items to process
        async_fn: Async function to call for each item
        batch_size: Number of concurrent requests per batch
        delay_between_batches: Seconds to wait between batches

    Returns:
        List of (item, result, error) tuples. Error is None if successful.
    """
    results: list[tuple[T, Any, Exception | None]] = []

    for i in range(0, len(items), batch_size):
        batch = items[i : i + batch_size]

        async def safe_call(item: T) -> tuple[T, Any, Exception | None]:
            try:
                result = await async_fn(item)
                return (item, result, None)
            except Exception as e:
                logger.warning(f"Batch item failed: {e}")
                return (item, None, e)

        batch_results = await asyncio.gather(*[safe_call(item) for item in batch])
        results.extend(batch_results)

        # Add delay between batches to respect rate limits
        if i + batch_size < len(items):
            await asyncio.sleep(delay_between_batches)

    return results


@dataclass
class DiscoveryResult:
    """Result of the discovery phase."""

    teams_discovered: int = 0
    teams_new: int = 0
    teams_updated: int = 0
    players_discovered: int = 0
    players_new: int = 0
    players_updated: int = 0
    players_transferred: int = 0  # Players who changed teams
    entities_needing_profile: list[tuple[str, int]] = field(default_factory=list)  # [("player", 123), ("team", 456)]


class BaseSeeder(ABC):
    """
    Abstract base class for sport-specific seeders.

    Subclasses must implement:
    - sport_id: The sport identifier (NBA, NFL, FOOTBALL)
    - fetch_teams(): Fetch teams from API
    - fetch_players(): Fetch players from API
    - fetch_player_stats(): Fetch player statistics
    - fetch_team_stats(): Fetch team statistics
    - transform_player_stats(): Transform API response to DB schema
    - transform_team_stats(): Transform API response to DB schema
    """

    sport_id: str = ""

    def __init__(
        self,
        db: "StatsDB",
        api_service: "ApiSportsService",
    ):
        """
        Initialize the seeder.

        Args:
            db: Stats database connection
            api_service: API-Sports service instance
        """
        self.db = db
        self.api = api_service
        self._sync_id: Optional[int] = None

    # =========================================================================
    # Sync Tracking
    # =========================================================================

    def _start_sync(
        self,
        sync_type: str,
        entity_type: Optional[str] = None,
        season_id: Optional[int] = None,
    ) -> int:
        """Record the start of a sync operation."""
        result = self.db.fetchone(
            """
            INSERT INTO sync_log (sport_id, sync_type, entity_type, season_id, started_at, status)
            VALUES (%s, %s, %s, %s, NOW(), 'running')
            RETURNING id
            """,
            (self.sport_id, sync_type, entity_type, season_id),
        )
        return result["id"] if result else 0

    def _complete_sync(
        self,
        sync_id: int,
        records_processed: int,
        records_inserted: int,
        records_updated: int,
    ) -> None:
        """Record successful sync completion."""
        self.db.execute(
            """
            UPDATE sync_log
            SET status = 'completed',
                completed_at = NOW(),
                records_processed = %s,
                records_inserted = %s,
                records_updated = %s
            WHERE id = %s
            """,
            (
                records_processed,
                records_inserted,
                records_updated,
                sync_id,
            ),
        )

    def _fail_sync(self, sync_id: int, error_message: str) -> None:
        """Record sync failure."""
        self.db.execute(
            """
            UPDATE sync_log
            SET status = 'failed',
                completed_at = NOW(),
                error_message = %s
            WHERE id = %s
            """,
            (error_message, sync_id),
        )

    # =========================================================================
    # Cache Invalidation
    # =========================================================================

    def invalidate_percentile_cache(self, season_id: int) -> int:
        """
        Invalidate the percentile cache for this sport and season.

        Should be called after stats are updated to ensure percentiles
        are recalculated with fresh data.

        Args:
            season_id: Season ID to invalidate

        Returns:
            Number of records deleted
        """
        result = self.db.fetchone(
            """
            DELETE FROM percentile_cache
            WHERE sport_id = %s AND season_id = %s
            RETURNING COUNT(*) as count
            """,
            (self.sport_id, season_id),
        )

        # Also try to invalidate API cache if available
        try:
            from ..api.cache import get_cache
            cache = get_cache()
            cache.invalidate_stats_cache(self.sport_id)
        except Exception:
            # API cache may not be available during CLI seeding
            pass

        count = result["count"] if result else 0
        logger.info(f"Invalidated {count} percentile cache entries for {self.sport_id} season {season_id}")
        return count

    def recalculate_percentiles(self, season_year: int) -> dict[str, int]:
        """
        Recalculate all percentiles for this sport and season.

        Writes percentiles as JSONB directly to the stats tables.
        Uses the pure Python calculator for database-agnostic operation.

        Args:
            season_year: Season year

        Returns:
            Dict with player and team counts
        """
        try:
            from ..percentiles.python_calculator import PythonPercentileCalculator
            calculator = PythonPercentileCalculator(self.db)
            return calculator.recalculate_all_percentiles(self.sport_id, season_year)
        except ImportError:
            logger.warning("Percentile calculator not available")
            return {"players": 0, "teams": 0}

    # =========================================================================
    # Season Management
    # =========================================================================

    def ensure_season(self, season_year: int, is_current: bool = False) -> int:
        """
        Ensure a season exists and return its ID.

        Args:
            season_year: The season year (e.g., 2024)
            is_current: Whether this is the current season

        Returns:
            Season ID
        """
        existing = self.db.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (self.sport_id, season_year),
        )

        if existing:
            # Update is_current if needed
            if is_current:
                self.db.execute(
                    "UPDATE seasons SET is_current = false WHERE sport_id = %s",
                    (self.sport_id,),
                )
                self.db.execute(
                    "UPDATE seasons SET is_current = true WHERE id = %s",
                    (existing["id"],),
                )
            return existing["id"]

        # Create new season
        label = self._get_season_label(season_year)

        if is_current:
            # Clear current flag from other seasons
            self.db.execute(
                "UPDATE seasons SET is_current = false WHERE sport_id = %s",
                (self.sport_id,),
            )

        result = self.db.fetchone(
            """
            INSERT INTO seasons (sport_id, season_year, season_label, is_current)
            VALUES (%s, %s, %s, %s)
            RETURNING id
            """,
            (self.sport_id, season_year, label, is_current),
        )

        return result["id"] if result else 0

    def _get_season_label(self, season_year: int) -> str:
        """
        Get human-readable season label.

        Override in subclasses for sport-specific formatting.
        """
        return str(season_year)

    # =========================================================================
    # Team Management
    # =========================================================================

    def _get_team_table(self) -> str:
        """Get the sport-specific team profile table name."""
        return TEAM_PROFILE_TABLES.get(self.sport_id, f"{self.sport_id.lower()}_team_profiles")

    def _get_player_table(self) -> str:
        """Get the sport-specific player profile table name."""
        return PLAYER_PROFILE_TABLES.get(self.sport_id, f"{self.sport_id.lower()}_player_profiles")

    def upsert_team(self, team_data: dict[str, Any], mark_profile_fetched: bool = False) -> int:
        """
        Insert or update a team record in sport-specific table.

        Args:
            team_data: Team data matching sport-specific TeamProfile schema
            mark_profile_fetched: If True, set profile_fetched_at to now

        Returns:
            Team ID
        """
        table = self._get_team_table()
        profile_fetched_clause = "NOW()" if mark_profile_fetched else "%s"
        profile_fetched_param = [] if mark_profile_fetched else [team_data.get("profile_fetched_at")]

        # Build query based on sport (FOOTBALL has league_id FK, others have conference/division)
        if self.sport_id == "FOOTBALL":
            self.db.execute(
                f"""
                INSERT INTO {table} (
                    id, name, abbreviation, country, city, league_id,
                    logo_url, founded, is_national,
                    venue_name, venue_address, venue_city, venue_capacity, venue_surface, venue_image,
                    profile_fetched_at, updated_at
                )
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, {profile_fetched_clause}, NOW())
                ON CONFLICT(id) DO UPDATE SET
                    name = COALESCE(excluded.name, {table}.name),
                    abbreviation = COALESCE(excluded.abbreviation, {table}.abbreviation),
                    country = COALESCE(excluded.country, {table}.country),
                    city = COALESCE(excluded.city, {table}.city),
                    league_id = COALESCE(excluded.league_id, {table}.league_id),
                    logo_url = COALESCE(excluded.logo_url, {table}.logo_url),
                    founded = COALESCE(excluded.founded, {table}.founded),
                    is_national = COALESCE(excluded.is_national, {table}.is_national),
                    venue_name = COALESCE(excluded.venue_name, {table}.venue_name),
                    venue_address = COALESCE(excluded.venue_address, {table}.venue_address),
                    venue_city = COALESCE(excluded.venue_city, {table}.venue_city),
                    venue_capacity = COALESCE(excluded.venue_capacity, {table}.venue_capacity),
                    venue_surface = COALESCE(excluded.venue_surface, {table}.venue_surface),
                    venue_image = COALESCE(excluded.venue_image, {table}.venue_image),
                    profile_fetched_at = COALESCE(excluded.profile_fetched_at, {table}.profile_fetched_at),
                    updated_at = NOW()
                """,
                (
                    team_data["id"],
                    team_data["name"],
                    team_data.get("abbreviation"),
                    team_data.get("country"),
                    team_data.get("city"),
                    team_data.get("league_id"),
                    team_data.get("logo_url"),
                    team_data.get("founded"),
                    team_data.get("is_national", False),
                    team_data.get("venue_name"),
                    team_data.get("venue_address"),
                    team_data.get("venue_city"),
                    team_data.get("venue_capacity"),
                    team_data.get("venue_surface"),
                    team_data.get("venue_image"),
                    *profile_fetched_param,
                ),
            )
        else:
            # NBA/NFL have conference/division instead of league_id
            self.db.execute(
                f"""
                INSERT INTO {table} (
                    id, name, abbreviation, conference, division, city, country,
                    logo_url, founded,
                    venue_name, venue_address, venue_city, venue_capacity, venue_surface, venue_image,
                    profile_fetched_at, updated_at
                )
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, {profile_fetched_clause}, NOW())
                ON CONFLICT(id) DO UPDATE SET
                    name = COALESCE(excluded.name, {table}.name),
                    abbreviation = COALESCE(excluded.abbreviation, {table}.abbreviation),
                    conference = COALESCE(excluded.conference, {table}.conference),
                    division = COALESCE(excluded.division, {table}.division),
                    city = COALESCE(excluded.city, {table}.city),
                    country = COALESCE(excluded.country, {table}.country),
                    logo_url = COALESCE(excluded.logo_url, {table}.logo_url),
                    founded = COALESCE(excluded.founded, {table}.founded),
                    venue_name = COALESCE(excluded.venue_name, {table}.venue_name),
                    venue_address = COALESCE(excluded.venue_address, {table}.venue_address),
                    venue_city = COALESCE(excluded.venue_city, {table}.venue_city),
                    venue_capacity = COALESCE(excluded.venue_capacity, {table}.venue_capacity),
                    venue_surface = COALESCE(excluded.venue_surface, {table}.venue_surface),
                    venue_image = COALESCE(excluded.venue_image, {table}.venue_image),
                    profile_fetched_at = COALESCE(excluded.profile_fetched_at, {table}.profile_fetched_at),
                    updated_at = NOW()
                """,
                (
                    team_data["id"],
                    team_data["name"],
                    team_data.get("abbreviation"),
                    team_data.get("conference"),
                    team_data.get("division"),
                    team_data.get("city"),
                    team_data.get("country"),
                    team_data.get("logo_url"),
                    team_data.get("founded"),
                    team_data.get("venue_name"),
                    team_data.get("venue_address"),
                    team_data.get("venue_city"),
                    team_data.get("venue_capacity"),
                    team_data.get("venue_surface"),
                    team_data.get("venue_image"),
                    *profile_fetched_param,
                ),
            )
        return team_data["id"]

    # =========================================================================
    # Player Management
    # =========================================================================

    def upsert_player(self, player_data: dict[str, Any], mark_profile_fetched: bool = False) -> int:
        """
        Insert or update a player record in sport-specific table.

        Args:
            player_data: Player data matching sport-specific PlayerProfile schema
            mark_profile_fetched: If True, set profile_fetched_at to now

        Returns:
            Player ID
        """
        table = self._get_player_table()
        profile_fetched_clause = "NOW()" if mark_profile_fetched else "%s"
        profile_fetched_param = [] if mark_profile_fetched else [player_data.get("profile_fetched_at")]

        # Build query based on sport (FOOTBALL has current_league_id, NBA/NFL have college/experience)
        if self.sport_id == "FOOTBALL":
            self.db.execute(
                f"""
                INSERT INTO {table} (
                    id, first_name, last_name, full_name,
                    position, position_group, nationality, birth_date, birth_place, birth_country,
                    height_inches, weight_lbs, photo_url, current_team_id, current_league_id,
                    jersey_number, profile_fetched_at, updated_at
                )
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, {profile_fetched_clause}, NOW())
                ON CONFLICT(id) DO UPDATE SET
                    first_name = COALESCE(excluded.first_name, {table}.first_name),
                    last_name = COALESCE(excluded.last_name, {table}.last_name),
                    full_name = COALESCE(excluded.full_name, {table}.full_name),
                    position = COALESCE(excluded.position, {table}.position),
                    position_group = COALESCE(excluded.position_group, {table}.position_group),
                    nationality = COALESCE(excluded.nationality, {table}.nationality),
                    birth_date = COALESCE(excluded.birth_date, {table}.birth_date),
                    birth_place = COALESCE(excluded.birth_place, {table}.birth_place),
                    birth_country = COALESCE(excluded.birth_country, {table}.birth_country),
                    height_inches = COALESCE(excluded.height_inches, {table}.height_inches),
                    weight_lbs = COALESCE(excluded.weight_lbs, {table}.weight_lbs),
                    photo_url = COALESCE(excluded.photo_url, {table}.photo_url),
                    current_team_id = COALESCE(excluded.current_team_id, {table}.current_team_id),
                    current_league_id = COALESCE(excluded.current_league_id, {table}.current_league_id),
                    jersey_number = COALESCE(excluded.jersey_number, {table}.jersey_number),
                    profile_fetched_at = COALESCE(excluded.profile_fetched_at, {table}.profile_fetched_at),
                    updated_at = NOW()
                """,
                (
                    player_data["id"],
                    player_data.get("first_name"),
                    player_data.get("last_name"),
                    player_data["full_name"],
                    player_data.get("position"),
                    player_data.get("position_group"),
                    player_data.get("nationality"),
                    player_data.get("birth_date"),
                    player_data.get("birth_place"),
                    player_data.get("birth_country"),
                    player_data.get("height_inches"),
                    player_data.get("weight_lbs"),
                    player_data.get("photo_url"),
                    player_data.get("current_team_id"),
                    player_data.get("current_league_id"),
                    player_data.get("jersey_number"),
                    *profile_fetched_param,
                ),
            )
        else:
            # NBA/NFL have college and experience_years
            self.db.execute(
                f"""
                INSERT INTO {table} (
                    id, first_name, last_name, full_name,
                    position, position_group, nationality, birth_date, birth_place, birth_country,
                    height_inches, weight_lbs, photo_url, current_team_id,
                    jersey_number, college, experience_years, profile_fetched_at, updated_at
                )
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, {profile_fetched_clause}, NOW())
                ON CONFLICT(id) DO UPDATE SET
                    first_name = COALESCE(excluded.first_name, {table}.first_name),
                    last_name = COALESCE(excluded.last_name, {table}.last_name),
                    full_name = COALESCE(excluded.full_name, {table}.full_name),
                    position = COALESCE(excluded.position, {table}.position),
                    position_group = COALESCE(excluded.position_group, {table}.position_group),
                    nationality = COALESCE(excluded.nationality, {table}.nationality),
                    birth_date = COALESCE(excluded.birth_date, {table}.birth_date),
                    birth_place = COALESCE(excluded.birth_place, {table}.birth_place),
                    birth_country = COALESCE(excluded.birth_country, {table}.birth_country),
                    height_inches = COALESCE(excluded.height_inches, {table}.height_inches),
                    weight_lbs = COALESCE(excluded.weight_lbs, {table}.weight_lbs),
                    photo_url = COALESCE(excluded.photo_url, {table}.photo_url),
                    current_team_id = COALESCE(excluded.current_team_id, {table}.current_team_id),
                    jersey_number = COALESCE(excluded.jersey_number, {table}.jersey_number),
                    college = COALESCE(excluded.college, {table}.college),
                    experience_years = COALESCE(excluded.experience_years, {table}.experience_years),
                    profile_fetched_at = COALESCE(excluded.profile_fetched_at, {table}.profile_fetched_at),
                    updated_at = NOW()
                """,
                (
                    player_data["id"],
                    player_data.get("first_name"),
                    player_data.get("last_name"),
                    player_data["full_name"],
                    player_data.get("position"),
                    player_data.get("position_group"),
                    player_data.get("nationality"),
                    player_data.get("birth_date"),
                    player_data.get("birth_place"),
                    player_data.get("birth_country"),
                    player_data.get("height_inches"),
                    player_data.get("weight_lbs"),
                    player_data.get("photo_url"),
                    player_data.get("current_team_id"),
                    player_data.get("jersey_number"),
                    player_data.get("college"),
                    player_data.get("experience_years"),
                    *profile_fetched_param,
                ),
            )
        return player_data["id"]

    # =========================================================================
    # Batch Upsert Methods (Performance Optimized)
    # =========================================================================

    def batch_upsert_teams(self, teams: list[dict[str, Any]], batch_size: int = 100) -> int:
        """
        Batch insert/update teams using PostgreSQL multi-row INSERT.

        Reduces N database roundtrips to ceil(N/batch_size) roundtrips.

        Args:
            teams: List of team data dicts
            batch_size: Number of rows per batch (default 100)

        Returns:
            Number of teams upserted
        """
        if not teams:
            return 0

        total = 0
        for i in range(0, len(teams), batch_size):
            batch = teams[i : i + batch_size]
            total += self._batch_upsert_teams_chunk(batch)

        return total

    def _batch_upsert_teams_chunk(self, teams: list[dict[str, Any]]) -> int:
        """Execute a single batch upsert for teams into sport-specific table."""
        if not teams:
            return 0

        table = self._get_team_table()

        if self.sport_id == "FOOTBALL":
            # FOOTBALL: has league_id, no conference/division
            placeholders = []
            params = []
            for team in teams:
                placeholders.append(
                    "(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, NOW())"
                )
                params.extend([
                    team["id"],
                    team["name"],
                    team.get("abbreviation"),
                    team.get("country"),
                    team.get("city"),
                    team.get("league_id"),
                    team.get("logo_url"),
                    team.get("founded"),
                    team.get("is_national", False),
                    team.get("venue_name"),
                    team.get("venue_address"),
                    team.get("venue_city"),
                    team.get("venue_capacity"),
                    team.get("venue_surface"),
                ])

            query = f"""
                INSERT INTO {table} (
                    id, name, abbreviation, country, city, league_id,
                    logo_url, founded, is_national,
                    venue_name, venue_address, venue_city, venue_capacity, venue_surface,
                    updated_at
                )
                VALUES {", ".join(placeholders)}
                ON CONFLICT(id) DO UPDATE SET
                    name = COALESCE(excluded.name, {table}.name),
                    abbreviation = COALESCE(excluded.abbreviation, {table}.abbreviation),
                    country = COALESCE(excluded.country, {table}.country),
                    city = COALESCE(excluded.city, {table}.city),
                    league_id = COALESCE(excluded.league_id, {table}.league_id),
                    logo_url = COALESCE(excluded.logo_url, {table}.logo_url),
                    founded = COALESCE(excluded.founded, {table}.founded),
                    is_national = COALESCE(excluded.is_national, {table}.is_national),
                    venue_name = COALESCE(excluded.venue_name, {table}.venue_name),
                    venue_address = COALESCE(excluded.venue_address, {table}.venue_address),
                    venue_city = COALESCE(excluded.venue_city, {table}.venue_city),
                    venue_capacity = COALESCE(excluded.venue_capacity, {table}.venue_capacity),
                    venue_surface = COALESCE(excluded.venue_surface, {table}.venue_surface),
                    updated_at = NOW()
            """
        else:
            # NBA/NFL: have conference/division, no league_id
            placeholders = []
            params = []
            for team in teams:
                placeholders.append(
                    "(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, NOW())"
                )
                params.extend([
                    team["id"],
                    team["name"],
                    team.get("abbreviation"),
                    team.get("conference"),
                    team.get("division"),
                    team.get("city"),
                    team.get("country"),
                    team.get("logo_url"),
                    team.get("founded"),
                    team.get("venue_name"),
                    team.get("venue_address"),
                    team.get("venue_city"),
                    team.get("venue_capacity"),
                    team.get("venue_surface"),
                ])

            query = f"""
                INSERT INTO {table} (
                    id, name, abbreviation, conference, division, city, country,
                    logo_url, founded,
                    venue_name, venue_address, venue_city, venue_capacity, venue_surface,
                    updated_at
                )
                VALUES {", ".join(placeholders)}
                ON CONFLICT(id) DO UPDATE SET
                    name = COALESCE(excluded.name, {table}.name),
                    abbreviation = COALESCE(excluded.abbreviation, {table}.abbreviation),
                    conference = COALESCE(excluded.conference, {table}.conference),
                    division = COALESCE(excluded.division, {table}.division),
                    city = COALESCE(excluded.city, {table}.city),
                    country = COALESCE(excluded.country, {table}.country),
                    logo_url = COALESCE(excluded.logo_url, {table}.logo_url),
                    founded = COALESCE(excluded.founded, {table}.founded),
                    venue_name = COALESCE(excluded.venue_name, {table}.venue_name),
                    venue_address = COALESCE(excluded.venue_address, {table}.venue_address),
                    venue_city = COALESCE(excluded.venue_city, {table}.venue_city),
                    venue_capacity = COALESCE(excluded.venue_capacity, {table}.venue_capacity),
                    venue_surface = COALESCE(excluded.venue_surface, {table}.venue_surface),
                    updated_at = NOW()
            """

        self.db.execute(query, params)
        return len(teams)

    def batch_upsert_players(self, players: list[dict[str, Any]], batch_size: int = 100) -> int:
        """
        Batch insert/update players using PostgreSQL multi-row INSERT.

        Reduces N database roundtrips to ceil(N/batch_size) roundtrips.

        Args:
            players: List of player data dicts
            batch_size: Number of rows per batch (default 100)

        Returns:
            Number of players upserted
        """
        if not players:
            return 0

        total = 0
        for i in range(0, len(players), batch_size):
            batch = players[i : i + batch_size]
            total += self._batch_upsert_players_chunk(batch)

        return total

    def _batch_upsert_players_chunk(self, players: list[dict[str, Any]]) -> int:
        """Execute a single batch upsert for players into sport-specific table."""
        if not players:
            return 0

        table = self._get_player_table()

        if self.sport_id == "FOOTBALL":
            # FOOTBALL: has current_league_id, no college/experience
            placeholders = []
            params = []
            for player in players:
                placeholders.append(
                    "(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, NOW())"
                )
                params.extend([
                    player["id"],
                    player.get("first_name"),
                    player.get("last_name"),
                    player["full_name"],
                    player.get("position"),
                    player.get("position_group"),
                    player.get("nationality"),
                    player.get("birth_date"),
                    player.get("birth_place"),
                    player.get("birth_country"),
                    player.get("height_inches"),
                    player.get("weight_lbs"),
                    player.get("photo_url"),
                    player.get("current_team_id"),
                    player.get("current_league_id"),
                    player.get("jersey_number"),
                ])

            query = f"""
                INSERT INTO {table} (
                    id, first_name, last_name, full_name,
                    position, position_group, nationality, birth_date, birth_place, birth_country,
                    height_inches, weight_lbs, photo_url, current_team_id, current_league_id,
                    jersey_number, updated_at
                )
                VALUES {", ".join(placeholders)}
                ON CONFLICT(id) DO UPDATE SET
                    first_name = COALESCE(excluded.first_name, {table}.first_name),
                    last_name = COALESCE(excluded.last_name, {table}.last_name),
                    full_name = COALESCE(excluded.full_name, {table}.full_name),
                    position = COALESCE(excluded.position, {table}.position),
                    position_group = COALESCE(excluded.position_group, {table}.position_group),
                    nationality = COALESCE(excluded.nationality, {table}.nationality),
                    birth_date = COALESCE(excluded.birth_date, {table}.birth_date),
                    birth_place = COALESCE(excluded.birth_place, {table}.birth_place),
                    birth_country = COALESCE(excluded.birth_country, {table}.birth_country),
                    height_inches = COALESCE(excluded.height_inches, {table}.height_inches),
                    weight_lbs = COALESCE(excluded.weight_lbs, {table}.weight_lbs),
                    photo_url = COALESCE(excluded.photo_url, {table}.photo_url),
                    current_team_id = COALESCE(excluded.current_team_id, {table}.current_team_id),
                    current_league_id = COALESCE(excluded.current_league_id, {table}.current_league_id),
                    jersey_number = COALESCE(excluded.jersey_number, {table}.jersey_number),
                    updated_at = NOW()
            """
        else:
            # NBA/NFL: have college/experience_years, no current_league_id
            placeholders = []
            params = []
            for player in players:
                placeholders.append(
                    "(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, NOW())"
                )
                params.extend([
                    player["id"],
                    player.get("first_name"),
                    player.get("last_name"),
                    player["full_name"],
                    player.get("position"),
                    player.get("position_group"),
                    player.get("nationality"),
                    player.get("birth_date"),
                    player.get("birth_place"),
                    player.get("birth_country"),
                    player.get("height_inches"),
                    player.get("weight_lbs"),
                    player.get("photo_url"),
                    player.get("current_team_id"),
                    player.get("jersey_number"),
                    player.get("college"),
                    player.get("experience_years"),
                ])

            query = f"""
                INSERT INTO {table} (
                    id, first_name, last_name, full_name,
                    position, position_group, nationality, birth_date, birth_place, birth_country,
                    height_inches, weight_lbs, photo_url, current_team_id,
                    jersey_number, college, experience_years, updated_at
                )
                VALUES {", ".join(placeholders)}
                ON CONFLICT(id) DO UPDATE SET
                    first_name = COALESCE(excluded.first_name, {table}.first_name),
                    last_name = COALESCE(excluded.last_name, {table}.last_name),
                    full_name = COALESCE(excluded.full_name, {table}.full_name),
                    position = COALESCE(excluded.position, {table}.position),
                    position_group = COALESCE(excluded.position_group, {table}.position_group),
                    nationality = COALESCE(excluded.nationality, {table}.nationality),
                    birth_date = COALESCE(excluded.birth_date, {table}.birth_date),
                    birth_place = COALESCE(excluded.birth_place, {table}.birth_place),
                    birth_country = COALESCE(excluded.birth_country, {table}.birth_country),
                    height_inches = COALESCE(excluded.height_inches, {table}.height_inches),
                    weight_lbs = COALESCE(excluded.weight_lbs, {table}.weight_lbs),
                    photo_url = COALESCE(excluded.photo_url, {table}.photo_url),
                    current_team_id = COALESCE(excluded.current_team_id, {table}.current_team_id),
                    jersey_number = COALESCE(excluded.jersey_number, {table}.jersey_number),
                    college = COALESCE(excluded.college, {table}.college),
                    experience_years = COALESCE(excluded.experience_years, {table}.experience_years),
                    updated_at = NOW()
            """

        self.db.execute(query, params)
        return len(players)

    # =========================================================================
    # Two-Phase Seeding: Discovery & Profile Fetch
    # =========================================================================

    def get_entities_needing_profile(self, entity_type: str) -> list[int]:
        """
        Get entities that need their profile fetched from sport-specific table.

        Args:
            entity_type: 'team' or 'player'

        Returns:
            List of entity IDs needing profile fetch
        """
        table = self._get_team_table() if entity_type == "team" else self._get_player_table()
        result = self.db.fetchall(
            f"SELECT id FROM {table} WHERE profile_fetched_at IS NULL",
            (),
        )
        return [r["id"] for r in result]

    def mark_profile_fetched(self, entity_type: str, entity_id: int) -> None:
        """
        Mark an entity's profile as fetched in sport-specific table.

        Args:
            entity_type: 'team' or 'player'
            entity_id: Entity ID
        """
        table = self._get_team_table() if entity_type == "team" else self._get_player_table()
        self.db.execute(
            f"UPDATE {table} SET profile_fetched_at = NOW() WHERE id = %s",
            (entity_id,),
        )

    def detect_team_changes(self, player_id: int, new_team_id: Optional[int], season_id: int) -> bool:
        """
        Detect if a player has changed teams and update player_teams history.

        Args:
            player_id: Player ID
            new_team_id: New team ID from API
            season_id: Current season ID

        Returns:
            True if player changed teams (transfer detected)
        """
        table = self._get_player_table()
        existing = self.db.fetchone(
            f"SELECT current_team_id FROM {table} WHERE id = %s",
            (player_id,),
        )

        if not existing:
            return False

        old_team_id = existing.get("current_team_id")

        if old_team_id != new_team_id:
            # Close old player_teams record
            if old_team_id:
                self.db.execute(
                    """
                    UPDATE player_teams
                    SET end_date = CURRENT_DATE, is_current = false
                    WHERE player_id = %s AND team_id = %s AND is_current = true
                    """,
                    (player_id, old_team_id),
                )

            # Create new player_teams record
            if new_team_id:
                self.db.execute(
                    """
                    INSERT INTO player_teams
                    (player_id, team_id, season_id, start_date, is_current, detected_at)
                    VALUES (%s, %s, %s, CURRENT_DATE, true, NOW())
                    ON CONFLICT (player_id, team_id, season_id) DO NOTHING
                    """,
                    (player_id, new_team_id, season_id),
                )

            return True

        return False

    async def run_discovery_phase(
        self,
        season: int,
        league_id: Optional[int] = None,
    ) -> DiscoveryResult:
        """
        Phase 1: Discovery - Fetch rosters and identify new/changed entities.

        This phase:
        - Fetches all teams and players from the API
        - Compares against existing DB records
        - Identifies NEW entities (profile_fetched_at is NULL)
        - Detects player transfers (team changes)
        - Does NOT fetch full profiles yet

        Optimized with:
        - Batch ID lookups instead of individual queries
        - Batch upserts using multi-row INSERT

        Args:
            season: Season year
            league_id: Optional league ID for Football

        Returns:
            DiscoveryResult with counts and list of entities needing profiles
        """
        result = DiscoveryResult()
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("incremental", "discovery", season_id)

        try:
            # Fetch teams (minimal data)
            teams = await self.fetch_teams(season, league_id=league_id)
            team_table = self._get_team_table()
            player_table = self._get_player_table()

            if teams:
                # Batch fetch existing team IDs and profile status from sport-specific table
                team_ids = [t["id"] for t in teams]
                existing_teams = self.db.fetchall(
                    f"SELECT id, profile_fetched_at FROM {team_table} WHERE id = ANY(%s)",
                    (team_ids,),
                )
                existing_team_map = {t["id"]: t for t in existing_teams}

                # Categorize teams
                for team_data in teams:
                    tid = team_data["id"]
                    existing = existing_team_map.get(tid)

                    if existing:
                        result.teams_updated += 1
                    else:
                        result.teams_new += 1

                    # Queue for profile fetch if never fetched
                    if not existing or existing.get("profile_fetched_at") is None:
                        result.entities_needing_profile.append(("team", tid))

                    result.teams_discovered += 1

                # Batch upsert all teams (reduces N queries to 1)
                self.batch_upsert_teams(teams)

            # Fetch players (minimal data)
            players = await self.fetch_players(season, league_id=league_id)

            if players:
                # Batch fetch existing player IDs, profile status, and team from sport-specific table
                player_ids = [p["id"] for p in players]
                existing_players = self.db.fetchall(
                    f"SELECT id, profile_fetched_at, current_team_id FROM {player_table} WHERE id = ANY(%s)",
                    (player_ids,),
                )
                existing_player_map = {p["id"]: p for p in existing_players}

                # Categorize players and detect transfers
                for player_data in players:
                    pid = player_data["id"]
                    existing = existing_player_map.get(pid)

                    if existing:
                        result.players_updated += 1
                        # Check for transfer
                        if self.detect_team_changes(
                            pid,
                            player_data.get("current_team_id"),
                            season_id,
                        ):
                            result.players_transferred += 1
                    else:
                        result.players_new += 1

                    # Queue for profile fetch if never fetched
                    if not existing or existing.get("profile_fetched_at") is None:
                        result.entities_needing_profile.append(("player", pid))

                    result.players_discovered += 1

                # Batch upsert all players (reduces N queries to 1)
                self.batch_upsert_players(players)

            self._complete_sync(
                sync_id,
                result.teams_discovered + result.players_discovered,
                result.teams_new + result.players_new,
                result.teams_updated + result.players_updated,
            )

            logger.info(
                "Discovery complete for %s season %d: %d teams (%d new), %d players (%d new, %d transfers)",
                self.sport_id,
                season,
                result.teams_discovered,
                result.teams_new,
                result.players_discovered,
                result.players_new,
                result.players_transferred,
            )

            return result

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def run_profile_fetch_phase(
        self,
        entities: list[tuple[str, int]],
        batch_size: int = 10,
    ) -> int:
        """
        Phase 2: Profile Fetch - Fetch full profiles for new entities only.

        This phase:
        - Fetches complete profile data (photos, venue details, bio)
        - Only for entities in the provided list
        - Marks profile_fetched_at after successful fetch
        - Uses parallel batching for efficiency

        Args:
            entities: List of (entity_type, entity_id) tuples to fetch
            batch_size: Number of concurrent requests per batch (default 10)

        Returns:
            Number of profiles fetched
        """
        if not entities:
            logger.info("No entities need profile fetch")
            return 0

        sync_id = self._start_sync("incremental", "profiles")
        fetched = 0

        try:
            # Split entities by type for parallel processing
            teams = [(et, eid) for et, eid in entities if et == "team"]
            players = [(et, eid) for et, eid in entities if et == "player"]

            async def fetch_team_profile_and_upsert(entity: tuple[str, int]) -> bool:
                _, entity_id = entity
                profile_data = await self.fetch_team_profile(entity_id)
                if profile_data:
                    self.upsert_team(profile_data, mark_profile_fetched=True)
                    return True
                return False

            async def fetch_player_profile_and_upsert(entity: tuple[str, int]) -> bool:
                _, entity_id = entity
                profile_data = await self.fetch_player_profile(entity_id)
                if profile_data:
                    self.upsert_player(profile_data, mark_profile_fetched=True)
                    return True
                return False

            # Fetch team profiles in parallel batches
            if teams:
                logger.info(f"Fetching {len(teams)} team profiles in batches of {batch_size}")
                team_results = await run_parallel_batches(
                    teams, fetch_team_profile_and_upsert, batch_size=batch_size
                )
                fetched += sum(1 for _, result, err in team_results if result and not err)

            # Fetch player profiles in parallel batches
            if players:
                logger.info(f"Fetching {len(players)} player profiles in batches of {batch_size}")
                player_results = await run_parallel_batches(
                    players, fetch_player_profile_and_upsert, batch_size=batch_size
                )
                fetched += sum(1 for _, result, err in player_results if result and not err)

            self._complete_sync(sync_id, len(entities), fetched, 0)
            logger.info("Profile fetch complete: %d/%d entities", fetched, len(entities))
            return fetched

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def seed_two_phase(
        self,
        season: int,
        league_id: Optional[int] = None,
        skip_profiles: bool = False,
        skip_percentiles: bool = False,
    ) -> dict[str, Any]:
        """
        Run the complete two-phase seeding process.

        Phase 1: Discovery - Find all entities, detect changes
        Phase 2: Profile Fetch - Get full profiles for new entities
        Phase 3: Stats Update - Update statistics
        Phase 4: Percentile Recalculation - Recalculate all percentiles

        Args:
            season: Season year
            league_id: Optional league ID (for Football)
            skip_profiles: If True, skip profile fetch phase
            skip_percentiles: If True, skip percentile recalculation

        Returns:
            Summary of seeding results
        """
        # Phase 1: Discovery
        discovery = await self.run_discovery_phase(season, league_id=league_id)

        # Phase 2: Profile Fetch (only for new entities)
        profiles_fetched = 0
        if not skip_profiles and discovery.entities_needing_profile:
            profiles_fetched = await self.run_profile_fetch_phase(
                discovery.entities_needing_profile
            )

        # Phase 3: Stats Update
        player_stats = await self.seed_player_stats(season)
        team_stats = await self.seed_team_stats(season)

        # Phase 4: Percentile Recalculation (invalidate and recalculate)
        percentiles = {"players": 0, "teams": 0}
        if not skip_percentiles and (player_stats > 0 or team_stats > 0):
            logger.info(f"Recalculating percentiles for {self.sport_id} {season}")
            percentiles = self.recalculate_percentiles(season)

        return {
            "discovery": {
                "teams_discovered": discovery.teams_discovered,
                "teams_new": discovery.teams_new,
                "players_discovered": discovery.players_discovered,
                "players_new": discovery.players_new,
                "players_transferred": discovery.players_transferred,
            },
            "profiles_fetched": profiles_fetched,
            "player_stats": player_stats,
            "team_stats": team_stats,
            "percentiles_recalculated": percentiles,
        }

    # =========================================================================
    # Abstract Methods (Must be implemented by subclasses)
    # =========================================================================

    @abstractmethod
    async def fetch_teams(
        self,
        season: int,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """
        Fetch teams from API-Sports (discovery phase).

        Args:
            season: Season year
            league_id: Optional league ID (for Football)

        Returns:
            List of team data dicts with minimal info
        """
        pass

    @abstractmethod
    async def fetch_players(
        self,
        season: int,
        team_id: Optional[int] = None,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """
        Fetch players from API-Sports (discovery phase).

        For Football, uses league-based fetching: /players?league={id}&season={year}&page=N
        For NBA/NFL, uses team-based fetching: /players?team={id}&season={year}

        Args:
            season: Season year
            team_id: Optional team ID (for NBA/NFL)
            league_id: Optional league ID (for Football)

        Returns:
            List of player data dicts with minimal info
        """
        pass

    @abstractmethod
    async def fetch_team_profile(self, team_id: int) -> Optional[dict[str, Any]]:
        """
        Fetch full team profile from API-Sports (profile fetch phase).

        Includes: logo, venue details, founded year, etc.

        Args:
            team_id: Team ID

        Returns:
            Full team profile data or None
        """
        pass

    @abstractmethod
    async def fetch_player_profile(self, player_id: int) -> Optional[dict[str, Any]]:
        """
        Fetch full player profile from API-Sports (profile fetch phase).

        Includes: photo, height, weight, birth date, nationality, etc.

        Args:
            player_id: Player ID

        Returns:
            Full player profile data or None
        """
        pass

    @abstractmethod
    async def fetch_player_stats(
        self,
        player_id: int,
        season: int,
    ) -> Optional[dict[str, Any]]:
        """
        Fetch player statistics from API-Sports.

        Args:
            player_id: Player ID
            season: Season year

        Returns:
            Raw stats response or None
        """
        pass

    @abstractmethod
    async def fetch_team_stats(
        self,
        team_id: int,
        season: int,
    ) -> Optional[dict[str, Any]]:
        """
        Fetch team statistics from API-Sports.

        Args:
            team_id: Team ID
            season: Season year

        Returns:
            Raw stats response or None
        """
        pass

    @abstractmethod
    def transform_player_stats(
        self,
        raw_stats: dict[str, Any],
        player_id: int,
        season_id: int,
        team_id: Optional[int] = None,
    ) -> dict[str, Any]:
        """
        Transform raw API stats to database schema.

        Args:
            raw_stats: Raw API response
            player_id: Player ID
            season_id: Season ID
            team_id: Team ID

        Returns:
            Dict matching the sport-specific stats table schema
        """
        pass

    @abstractmethod
    def transform_team_stats(
        self,
        raw_stats: dict[str, Any],
        team_id: int,
        season_id: int,
    ) -> dict[str, Any]:
        """
        Transform raw API team stats to database schema.

        Args:
            raw_stats: Raw API response
            team_id: Team ID
            season_id: Season ID

        Returns:
            Dict matching the sport-specific team stats table schema
        """
        pass

    @abstractmethod
    def upsert_player_stats(self, stats: dict[str, Any]) -> None:
        """
        Insert or update player statistics.

        Args:
            stats: Transformed stats dict
        """
        pass

    @abstractmethod
    def upsert_team_stats(self, stats: dict[str, Any]) -> None:
        """
        Insert or update team statistics.

        Args:
            stats: Transformed stats dict
        """
        pass

    # =========================================================================
    # Main Seeding Methods
    # =========================================================================

    async def seed_teams(self, season: int) -> int:
        """
        Seed all teams for a season.

        Uses batch upsert for efficiency (reduces N queries to 1).

        Args:
            season: Season year

        Returns:
            Number of teams seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "teams", season_id)

        try:
            teams = await self.fetch_teams(season)
            team_table = self._get_team_table()

            if not teams:
                self._complete_sync(sync_id, 0, 0, 0)
                return 0

            # Batch fetch existing team IDs for counting from sport-specific table
            team_ids = [t["id"] for t in teams]
            existing_teams = self.db.fetchall(
                f"SELECT id FROM {team_table} WHERE id = ANY(%s)",
                (team_ids,),
            )
            existing_ids = {t["id"] for t in existing_teams}

            inserted = sum(1 for t in teams if t["id"] not in existing_ids)
            updated = len(teams) - inserted

            # Batch upsert all teams
            self.batch_upsert_teams(teams)

            self._complete_sync(sync_id, len(teams), inserted, updated)
            logger.info(
                "Seeded %d teams for %s %d (inserted: %d, updated: %d)",
                len(teams),
                self.sport_id,
                season,
                inserted,
                updated,
            )
            return len(teams)

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def seed_players(self, season: int) -> int:
        """
        Seed all players for a season.

        Uses batch upsert for efficiency (reduces N queries to 1).

        Args:
            season: Season year

        Returns:
            Number of players seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "players", season_id)

        try:
            players = await self.fetch_players(season)
            player_table = self._get_player_table()

            if not players:
                self._complete_sync(sync_id, 0, 0, 0)
                return 0

            # Batch fetch existing player IDs for counting from sport-specific table
            player_ids = [p["id"] for p in players]
            existing_players = self.db.fetchall(
                f"SELECT id FROM {player_table} WHERE id = ANY(%s)",
                (player_ids,),
            )
            existing_ids = {p["id"] for p in existing_players}

            inserted = sum(1 for p in players if p["id"] not in existing_ids)
            updated = len(players) - inserted

            # Batch upsert all players
            self.batch_upsert_players(players)

            self._complete_sync(sync_id, len(players), inserted, updated)
            logger.info(
                "Seeded %d players for %s %d (inserted: %d, updated: %d)",
                len(players),
                self.sport_id,
                season,
                inserted,
                updated,
            )
            return len(players)

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def seed_player_stats(
        self,
        season: int,
        player_ids: Optional[list[int]] = None,
        batch_size: int = 10,
    ) -> int:
        """
        Seed player statistics for a season.

        Args:
            season: Season year
            player_ids: Optional list of specific player IDs to seed
            batch_size: Number of concurrent requests per batch (default 10)

        Returns:
            Number of stat records seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "player_stats", season_id)

        try:
            # Get players to process from sport-specific table
            player_table = self._get_player_table()
            if player_ids:
                # Verify players exist in sport-specific table
                existing = self.db.fetchall(
                    f"SELECT id FROM {player_table} WHERE id = ANY(%s)",
                    (player_ids,),
                )
                existing_ids = {p["id"] for p in existing}
                players = [
                    {"id": pid, "current_team_id": None}
                    for pid in player_ids
                    if pid in existing_ids
                ]
            else:
                players = self.db.fetchall(
                    f"SELECT id, current_team_id FROM {player_table}",
                    (),
                )

            async def fetch_and_upsert_player_stats(player: dict) -> bool:
                player_id = player["id"]
                team_id = player.get("current_team_id")

                raw_stats = await self.fetch_player_stats(player_id, season)
                if raw_stats:
                    stats = self.transform_player_stats(
                        raw_stats, player_id, season_id, team_id
                    )
                    if stats:  # May be None if no valid league stats found
                        self.upsert_player_stats(stats)
                        return True
                return False

            logger.info(f"Fetching stats for {len(players)} players in batches of {batch_size}")
            results = await run_parallel_batches(
                list(players), fetch_and_upsert_player_stats, batch_size=batch_size
            )
            processed = sum(1 for _, result, err in results if result and not err)

            self._complete_sync(sync_id, len(players), processed, 0)
            logger.info(
                "Seeded stats for %d players for %s %d",
                processed,
                self.sport_id,
                season,
            )
            return processed

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def seed_team_stats(self, season: int, batch_size: int = 10) -> int:
        """
        Seed team statistics for a season.

        Args:
            season: Season year
            batch_size: Number of concurrent requests per batch (default 10)

        Returns:
            Number of stat records seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "team_stats", season_id)

        try:
            # Get teams from sport-specific table
            team_table = self._get_team_table()
            teams = self.db.fetchall(
                f"SELECT id FROM {team_table}",
                (),
            )

            async def fetch_and_upsert_team_stats(team: dict) -> bool:
                team_id = team["id"]
                raw_stats = await self.fetch_team_stats(team_id, season)
                if raw_stats:
                    stats = self.transform_team_stats(raw_stats, team_id, season_id)
                    self.upsert_team_stats(stats)
                    return True
                return False

            logger.info(f"Fetching stats for {len(teams)} teams in batches of {batch_size}")
            results = await run_parallel_batches(
                list(teams), fetch_and_upsert_team_stats, batch_size=batch_size
            )
            processed = sum(1 for _, result, err in results if result and not err)

            self._complete_sync(sync_id, len(teams), processed, 0)
            logger.info(
                "Seeded stats for %d teams for %s %d",
                processed,
                self.sport_id,
                season,
            )
            return processed

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    async def seed_all(
        self,
        seasons: list[int],
        current_season: Optional[int] = None,
    ) -> dict[str, int]:
        """
        Seed all data for multiple seasons.

        Args:
            seasons: List of season years to seed
            current_season: Which season is the current one

        Returns:
            Summary of records seeded
        """
        summary = {
            "teams": 0,
            "players": 0,
            "player_stats": 0,
            "team_stats": 0,
        }

        for season in seasons:
            is_current = season == current_season

            # Ensure season exists
            self.ensure_season(season, is_current=is_current)

            # Seed in order: teams -> players -> stats
            summary["teams"] += await self.seed_teams(season)
            summary["players"] += await self.seed_players(season)
            summary["player_stats"] += await self.seed_player_stats(season)
            summary["team_stats"] += await self.seed_team_stats(season)

        return summary
