"""
Base seeder class for stats database population.

All sport-specific seeders inherit from this class and implement
the abstract methods for data transformation.

Two-Phase Seeding Architecture:
  Phase 1: DISCOVERY - Fetch rosters, identify new/changed entities
  Phase 2: PROFILE FETCH - Fetch full profiles for new entities only
  Phase 3: STATS UPDATE - Update statistics for all entities
"""

from __future__ import annotations

import logging
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from ..connection import StatsDB
    from ...services.apisports import ApiSportsService

logger = logging.getLogger(__name__)


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
        self.db.execute(
            """
            INSERT INTO sync_log (sport_id, sync_type, entity_type, season_id, started_at, status)
            VALUES (?, ?, ?, ?, ?, 'running')
            """,
            (self.sport_id, sync_type, entity_type, season_id, int(time.time())),
        )
        result = self.db.fetchone("SELECT last_insert_rowid() as id")
        return result["id"]

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
                completed_at = ?,
                records_processed = ?,
                records_inserted = ?,
                records_updated = ?
            WHERE id = ?
            """,
            (
                int(time.time()),
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
                completed_at = ?,
                error_message = ?
            WHERE id = ?
            """,
            (int(time.time()), error_message, sync_id),
        )

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
            "SELECT id FROM seasons WHERE sport_id = ? AND season_year = ?",
            (self.sport_id, season_year),
        )

        if existing:
            # Update is_current if needed
            if is_current:
                self.db.execute(
                    "UPDATE seasons SET is_current = 0 WHERE sport_id = ?",
                    (self.sport_id,),
                )
                self.db.execute(
                    "UPDATE seasons SET is_current = 1 WHERE id = ?",
                    (existing["id"],),
                )
            return existing["id"]

        # Create new season
        label = self._get_season_label(season_year)

        if is_current:
            # Clear current flag from other seasons
            self.db.execute(
                "UPDATE seasons SET is_current = 0 WHERE sport_id = ?",
                (self.sport_id,),
            )

        self.db.execute(
            """
            INSERT INTO seasons (sport_id, season_year, season_label, is_current)
            VALUES (?, ?, ?, ?)
            """,
            (self.sport_id, season_year, label, is_current),
        )

        result = self.db.fetchone("SELECT last_insert_rowid() as id")
        return result["id"]

    def _get_season_label(self, season_year: int) -> str:
        """
        Get human-readable season label.

        Override in subclasses for sport-specific formatting.
        """
        return str(season_year)

    # =========================================================================
    # Team Management
    # =========================================================================

    def upsert_team(self, team_data: dict[str, Any], mark_profile_fetched: bool = False) -> int:
        """
        Insert or update a team record.

        Args:
            team_data: Team data matching TeamModel schema
            mark_profile_fetched: If True, set profile_fetched_at to now

        Returns:
            Team ID
        """
        profile_fetched_at = int(time.time()) if mark_profile_fetched else team_data.get("profile_fetched_at")

        self.db.execute(
            """
            INSERT INTO teams (
                id, sport_id, league_id, name, abbreviation, logo_url,
                conference, division, country, city, founded,
                venue_name, venue_city, venue_capacity, venue_surface, venue_image,
                profile_fetched_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = COALESCE(excluded.name, teams.name),
                abbreviation = COALESCE(excluded.abbreviation, teams.abbreviation),
                logo_url = COALESCE(excluded.logo_url, teams.logo_url),
                conference = COALESCE(excluded.conference, teams.conference),
                division = COALESCE(excluded.division, teams.division),
                country = COALESCE(excluded.country, teams.country),
                city = COALESCE(excluded.city, teams.city),
                founded = COALESCE(excluded.founded, teams.founded),
                venue_name = COALESCE(excluded.venue_name, teams.venue_name),
                venue_city = COALESCE(excluded.venue_city, teams.venue_city),
                venue_capacity = COALESCE(excluded.venue_capacity, teams.venue_capacity),
                venue_surface = COALESCE(excluded.venue_surface, teams.venue_surface),
                venue_image = COALESCE(excluded.venue_image, teams.venue_image),
                profile_fetched_at = COALESCE(excluded.profile_fetched_at, teams.profile_fetched_at),
                updated_at = excluded.updated_at
            """,
            (
                team_data["id"],
                self.sport_id,
                team_data.get("league_id"),
                team_data["name"],
                team_data.get("abbreviation"),
                team_data.get("logo_url"),
                team_data.get("conference"),
                team_data.get("division"),
                team_data.get("country"),
                team_data.get("city"),
                team_data.get("founded"),
                team_data.get("venue_name"),
                team_data.get("venue_city"),
                team_data.get("venue_capacity"),
                team_data.get("venue_surface"),
                team_data.get("venue_image"),
                profile_fetched_at,
                int(time.time()),
            ),
        )
        return team_data["id"]

    # =========================================================================
    # Player Management
    # =========================================================================

    def upsert_player(self, player_data: dict[str, Any], mark_profile_fetched: bool = False) -> int:
        """
        Insert or update a player record.

        Args:
            player_data: Player data matching PlayerModel schema
            mark_profile_fetched: If True, set profile_fetched_at to now

        Returns:
            Player ID
        """
        profile_fetched_at = int(time.time()) if mark_profile_fetched else player_data.get("profile_fetched_at")

        self.db.execute(
            """
            INSERT INTO players (
                id, sport_id, first_name, last_name, full_name,
                position, position_group, nationality, birth_date, birth_place,
                height_inches, weight_lbs, photo_url, current_team_id, current_league_id,
                jersey_number, college, experience_years, profile_fetched_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                first_name = COALESCE(excluded.first_name, players.first_name),
                last_name = COALESCE(excluded.last_name, players.last_name),
                full_name = COALESCE(excluded.full_name, players.full_name),
                position = COALESCE(excluded.position, players.position),
                position_group = COALESCE(excluded.position_group, players.position_group),
                nationality = COALESCE(excluded.nationality, players.nationality),
                birth_date = COALESCE(excluded.birth_date, players.birth_date),
                birth_place = COALESCE(excluded.birth_place, players.birth_place),
                height_inches = COALESCE(excluded.height_inches, players.height_inches),
                weight_lbs = COALESCE(excluded.weight_lbs, players.weight_lbs),
                photo_url = COALESCE(excluded.photo_url, players.photo_url),
                current_team_id = COALESCE(excluded.current_team_id, players.current_team_id),
                current_league_id = COALESCE(excluded.current_league_id, players.current_league_id),
                jersey_number = COALESCE(excluded.jersey_number, players.jersey_number),
                college = COALESCE(excluded.college, players.college),
                experience_years = COALESCE(excluded.experience_years, players.experience_years),
                profile_fetched_at = COALESCE(excluded.profile_fetched_at, players.profile_fetched_at),
                updated_at = excluded.updated_at
            """,
            (
                player_data["id"],
                self.sport_id,
                player_data.get("first_name"),
                player_data.get("last_name"),
                player_data["full_name"],
                player_data.get("position"),
                player_data.get("position_group"),
                player_data.get("nationality"),
                player_data.get("birth_date"),
                player_data.get("birth_place"),
                player_data.get("height_inches"),
                player_data.get("weight_lbs"),
                player_data.get("photo_url"),
                player_data.get("current_team_id"),
                player_data.get("current_league_id"),
                player_data.get("jersey_number"),
                player_data.get("college"),
                player_data.get("experience_years"),
                profile_fetched_at,
                int(time.time()),
            ),
        )
        return player_data["id"]

    # =========================================================================
    # Two-Phase Seeding: Discovery & Profile Fetch
    # =========================================================================

    def get_entities_needing_profile(self, entity_type: str) -> list[int]:
        """
        Get entities that need their profile fetched.

        Args:
            entity_type: 'team' or 'player'

        Returns:
            List of entity IDs needing profile fetch
        """
        table = "teams" if entity_type == "team" else "players"
        result = self.db.fetchall(
            f"SELECT id FROM {table} WHERE sport_id = ? AND profile_fetched_at IS NULL",
            (self.sport_id,),
        )
        return [r["id"] for r in result]

    def mark_profile_fetched(self, entity_type: str, entity_id: int) -> None:
        """
        Mark an entity's profile as fetched.

        Args:
            entity_type: 'team' or 'player'
            entity_id: Entity ID
        """
        table = "teams" if entity_type == "team" else "players"
        self.db.execute(
            f"UPDATE {table} SET profile_fetched_at = ? WHERE id = ?",
            (int(time.time()), entity_id),
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
        existing = self.db.fetchone(
            "SELECT current_team_id FROM players WHERE id = ?",
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
                    SET end_date = date('now'), is_current = 0
                    WHERE player_id = ? AND team_id = ? AND is_current = 1
                    """,
                    (player_id, old_team_id),
                )

            # Create new player_teams record
            if new_team_id:
                self.db.execute(
                    """
                    INSERT OR IGNORE INTO player_teams
                    (player_id, team_id, season_id, start_date, is_current, detected_at)
                    VALUES (?, ?, ?, date('now'), 1, ?)
                    """,
                    (player_id, new_team_id, season_id, int(time.time())),
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
            for team_data in teams:
                existing = self.db.fetchone(
                    "SELECT id, profile_fetched_at FROM teams WHERE id = ?",
                    (team_data["id"],),
                )

                if existing:
                    result.teams_updated += 1
                else:
                    result.teams_new += 1

                # Always upsert (updates basic info, preserves profile data)
                self.upsert_team(team_data)

                # Queue for profile fetch if never fetched
                if not existing or existing.get("profile_fetched_at") is None:
                    result.entities_needing_profile.append(("team", team_data["id"]))

                result.teams_discovered += 1

            # Fetch players (minimal data)
            players = await self.fetch_players(season, league_id=league_id)
            for player_data in players:
                existing = self.db.fetchone(
                    "SELECT id, profile_fetched_at, current_team_id FROM players WHERE id = ?",
                    (player_data["id"],),
                )

                if existing:
                    result.players_updated += 1
                    # Check for transfer
                    if self.detect_team_changes(
                        player_data["id"],
                        player_data.get("current_team_id"),
                        season_id,
                    ):
                        result.players_transferred += 1
                else:
                    result.players_new += 1

                # Always upsert
                self.upsert_player(player_data)

                # Queue for profile fetch if never fetched
                if not existing or existing.get("profile_fetched_at") is None:
                    result.entities_needing_profile.append(("player", player_data["id"]))

                result.players_discovered += 1

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
    ) -> int:
        """
        Phase 2: Profile Fetch - Fetch full profiles for new entities only.

        This phase:
        - Fetches complete profile data (photos, venue details, bio)
        - Only for entities in the provided list
        - Marks profile_fetched_at after successful fetch

        Args:
            entities: List of (entity_type, entity_id) tuples to fetch

        Returns:
            Number of profiles fetched
        """
        if not entities:
            logger.info("No entities need profile fetch")
            return 0

        sync_id = self._start_sync("incremental", "profiles")
        fetched = 0

        try:
            for entity_type, entity_id in entities:
                try:
                    if entity_type == "team":
                        profile_data = await self.fetch_team_profile(entity_id)
                        if profile_data:
                            self.upsert_team(profile_data, mark_profile_fetched=True)
                            fetched += 1
                    elif entity_type == "player":
                        profile_data = await self.fetch_player_profile(entity_id)
                        if profile_data:
                            self.upsert_player(profile_data, mark_profile_fetched=True)
                            fetched += 1
                except Exception as e:
                    logger.warning("Failed to fetch profile for %s %d: %s", entity_type, entity_id, e)
                    continue

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
    ) -> dict[str, Any]:
        """
        Run the complete two-phase seeding process.

        Phase 1: Discovery - Find all entities, detect changes
        Phase 2: Profile Fetch - Get full profiles for new entities
        Phase 3: Stats Update - Update statistics

        Args:
            season: Season year
            league_id: Optional league ID (for Football)
            skip_profiles: If True, skip profile fetch phase

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

        Args:
            season: Season year

        Returns:
            Number of teams seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "teams", season_id)

        try:
            teams = await self.fetch_teams(season)
            inserted = 0
            updated = 0

            for team_data in teams:
                existing = self.db.get_team(team_data["id"], self.sport_id)
                self.upsert_team(team_data)

                if existing:
                    updated += 1
                else:
                    inserted += 1

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

        Args:
            season: Season year

        Returns:
            Number of players seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "players", season_id)

        try:
            players = await self.fetch_players(season)
            inserted = 0
            updated = 0

            for player_data in players:
                existing = self.db.get_player(player_data["id"], self.sport_id)
                self.upsert_player(player_data)

                if existing:
                    updated += 1
                else:
                    inserted += 1

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
    ) -> int:
        """
        Seed player statistics for a season.

        Args:
            season: Season year
            player_ids: Optional list of specific player IDs to seed

        Returns:
            Number of stat records seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "player_stats", season_id)

        try:
            # Get players to process
            if player_ids:
                players = [
                    {"id": pid}
                    for pid in player_ids
                    if self.db.get_player(pid, self.sport_id)
                ]
            else:
                players = self.db.fetchall(
                    "SELECT id, current_team_id FROM players WHERE sport_id = ?",
                    (self.sport_id,),
                )

            processed = 0
            for player in players:
                player_id = player["id"]
                team_id = player.get("current_team_id")

                raw_stats = await self.fetch_player_stats(player_id, season)
                if raw_stats:
                    stats = self.transform_player_stats(
                        raw_stats, player_id, season_id, team_id
                    )
                    if stats:  # May be None if no valid league stats found
                        self.upsert_player_stats(stats)
                        processed += 1

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

    async def seed_team_stats(self, season: int) -> int:
        """
        Seed team statistics for a season.

        Args:
            season: Season year

        Returns:
            Number of stat records seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "team_stats", season_id)

        try:
            teams = self.db.fetchall(
                "SELECT id FROM teams WHERE sport_id = ?",
                (self.sport_id,),
            )

            processed = 0
            for team in teams:
                team_id = team["id"]

                raw_stats = await self.fetch_team_stats(team_id, season)
                if raw_stats:
                    stats = self.transform_team_stats(raw_stats, team_id, season_id)
                    self.upsert_team_stats(stats)
                    processed += 1

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
