"""
Generic seeder that uses the new provider and repository abstractions.

This seeder is sport-agnostic and works with any sport defined in YAML config.
It replaces the sport-specific seeders (NBASeeder, NFLSeeder, etc.) with
a single, configuration-driven implementation.

Usage:
    from scoracle_data.seeders.generic_seeder import GenericSeeder
    from scoracle_data.providers import get_provider
    from scoracle_data.repositories import get_repositories
    from scoracle_data.connection import get_db
    
    db = get_db()
    provider = get_provider("api_sports")
    repos = get_repositories(db)
    
    seeder = GenericSeeder(
        sport="NBA",
        provider=provider,
        repos=repos,
        db=db,
    )
    
    result = await seeder.seed_two_phase(season=2024)
"""

from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional

from ..providers.base import DataProviderProtocol, RawEntityData

if TYPE_CHECKING:
    from ..connection import StatsDB
    from ..repositories.base import RepositorySet

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
    players_transferred: int = 0
    entities_needing_profile: list[tuple[str, int]] = field(default_factory=list)


@dataclass
class SeedingResult:
    """Complete seeding result."""
    sport: str
    season: int
    discovery: DiscoveryResult
    profiles_fetched: int = 0
    player_stats_updated: int = 0
    team_stats_updated: int = 0
    errors: list[str] = field(default_factory=list)
    duration_seconds: float = 0.0


async def run_parallel_batches(
    items: list[Any],
    async_fn,
    batch_size: int = 10,
    delay_between_batches: float = 0.5,
) -> list[tuple[Any, Any, Exception | None]]:
    """
    Execute async function on items in parallel batches.
    
    Respects rate limits by processing in batches with delays.
    """
    results = []
    
    for i in range(0, len(items), batch_size):
        batch = items[i:i + batch_size]
        
        async def safe_call(item):
            try:
                result = await async_fn(item)
                return (item, result, None)
            except Exception as e:
                logger.warning(f"Batch item failed: {e}")
                return (item, None, e)
        
        batch_results = await asyncio.gather(*[safe_call(item) for item in batch])
        results.extend(batch_results)
        
        # Delay between batches
        if i + batch_size < len(items):
            await asyncio.sleep(delay_between_batches)
    
    return results


class GenericSeeder:
    """
    Generic, provider-agnostic seeder.
    
    Uses configuration-driven approach to seed any sport from any provider.
    All sport-specific logic lives in YAML config files.
    """
    
    def __init__(
        self,
        sport: str,
        provider: DataProviderProtocol,
        repos: "RepositorySet",
        db: "StatsDB",
    ):
        """
        Initialize the generic seeder.
        
        Args:
            sport: Sport identifier (NBA, NFL, FOOTBALL)
            provider: Data provider implementation
            repos: Repository set for data persistence
            db: Database connection (for season management)
        """
        self.sport = sport.upper()
        self.provider = provider
        self.repos = repos
        self.db = db
    
    # ==========================================================================
    # Season Management
    # ==========================================================================
    
    def ensure_season(self, season_year: int, is_current: bool = False) -> int:
        """Ensure a season exists and return its ID."""
        existing = self.db.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (self.sport, season_year),
        )
        
        if existing:
            if is_current:
                self.db.execute(
                    "UPDATE seasons SET is_current = false WHERE sport_id = %s",
                    (self.sport,),
                )
                self.db.execute(
                    "UPDATE seasons SET is_current = true WHERE id = %s",
                    (existing["id"],),
                )
            return existing["id"]
        
        # Create new season
        label = self._get_season_label(season_year)
        
        if is_current:
            self.db.execute(
                "UPDATE seasons SET is_current = false WHERE sport_id = %s",
                (self.sport,),
            )
        
        result = self.db.fetchone(
            """
            INSERT INTO seasons (sport_id, season_year, season_label, is_current)
            VALUES (%s, %s, %s, %s)
            RETURNING id
            """,
            (self.sport, season_year, label, is_current),
        )
        
        return result["id"] if result else 0
    
    def _get_season_label(self, season_year: int) -> str:
        """Get human-readable season label."""
        if self.sport == "NBA":
            next_year = (season_year + 1) % 100
            return f"{season_year}-{next_year:02d}"
        elif self.sport == "NFL":
            return f"{season_year} Season"
        else:
            return str(season_year)
    
    # ==========================================================================
    # Sync Tracking
    # ==========================================================================
    
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
            (self.sport, sync_type, entity_type, season_id),
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
            (records_processed, records_inserted, records_updated, sync_id),
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
    
    # ==========================================================================
    # Two-Phase Seeding
    # ==========================================================================
    
    async def seed_two_phase(
        self,
        season: int,
        *,
        league_id: Optional[int] = None,
        skip_profiles: bool = False,
        skip_stats: bool = False,
        batch_size: int = 10,
    ) -> SeedingResult:
        """
        Run the complete two-phase seeding process.
        
        Phase 1: Discovery - Find all entities, detect changes
        Phase 2: Profile Fetch - Get full profiles for new entities
        Phase 3: Stats Update - Update statistics
        
        Args:
            season: Season year
            league_id: Optional league ID (required for FOOTBALL)
            skip_profiles: If True, skip profile fetch phase
            skip_stats: If True, skip stats update phase
            batch_size: Batch size for parallel fetching
            
        Returns:
            SeedingResult with summary
        """
        start_time = datetime.now()
        result = SeedingResult(
            sport=self.sport,
            season=season,
            discovery=DiscoveryResult(),
        )
        
        try:
            # Phase 1: Discovery
            logger.info(f"Starting discovery for {self.sport} {season}")
            result.discovery = await self._run_discovery(season, league_id=league_id)
            
            # Phase 2: Profile Fetch
            if not skip_profiles and result.discovery.entities_needing_profile:
                logger.info(f"Fetching {len(result.discovery.entities_needing_profile)} profiles")
                result.profiles_fetched = await self._run_profile_fetch(
                    result.discovery.entities_needing_profile,
                    batch_size=batch_size,
                )
            
            # Phase 3: Stats Update
            if not skip_stats:
                season_id = self.ensure_season(season)
                logger.info(f"Updating stats for {self.sport} {season}")
                
                result.player_stats_updated = await self._seed_player_stats(
                    season, 
                    season_id,
                    league_id=league_id,
                    batch_size=batch_size,
                )
                result.team_stats_updated = await self._seed_team_stats(
                    season,
                    season_id,
                    league_id=league_id,
                )
            
        except Exception as e:
            logger.error(f"Seeding failed: {e}")
            result.errors.append(str(e))
            raise
        
        result.duration_seconds = (datetime.now() - start_time).total_seconds()
        
        logger.info(
            f"Seeding complete for {self.sport} {season}: "
            f"{result.discovery.teams_discovered} teams, "
            f"{result.discovery.players_discovered} players, "
            f"{result.player_stats_updated} player stats, "
            f"{result.team_stats_updated} team stats "
            f"in {result.duration_seconds:.1f}s"
        )
        
        return result
    
    async def _run_discovery(
        self,
        season: int,
        *,
        league_id: Optional[int] = None,
    ) -> DiscoveryResult:
        """
        Phase 1: Discovery - Fetch all entities and identify new/changed ones.
        """
        result = DiscoveryResult()
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("incremental", "discovery", season_id)
        
        try:
            # Get existing entity IDs for comparison
            existing_team_ids = set(self.repos.teams.get_all_ids(self.sport))
            existing_player_ids = set(self.repos.players.get_all_ids(self.sport))
            
            # Fetch teams from provider
            teams = await self.provider.fetch_teams(
                self.sport, season, league_id=league_id
            )
            
            if teams:
                # Categorize and upsert
                team_data = []
                team_raw = []
                
                for team in teams:
                    team_id = int(team.provider_id)
                    
                    if team_id in existing_team_ids:
                        result.teams_updated += 1
                    else:
                        result.teams_new += 1
                        result.entities_needing_profile.append(("team", team_id))
                    
                    result.teams_discovered += 1
                    team_data.append(team.canonical_data)
                    team_raw.append(team.raw_response)
                
                # Batch upsert teams
                self.repos.teams.batch_upsert(self.sport, team_data, team_raw)
            
            # Fetch players from provider
            players = await self.provider.fetch_players(
                self.sport, season, league_id=league_id
            )
            
            if players:
                player_data = []
                player_raw = []
                
                for player in players:
                    player_id = int(player.provider_id)
                    
                    if player_id in existing_player_ids:
                        result.players_updated += 1
                        # Could check for team changes here
                    else:
                        result.players_new += 1
                        result.entities_needing_profile.append(("player", player_id))
                    
                    result.players_discovered += 1
                    player_data.append(player.canonical_data)
                    player_raw.append(player.raw_response)
                
                # Batch upsert players
                self.repos.players.batch_upsert(self.sport, player_data, player_raw)
            
            self._complete_sync(
                sync_id,
                result.teams_discovered + result.players_discovered,
                result.teams_new + result.players_new,
                result.teams_updated + result.players_updated,
            )
            
            logger.info(
                f"Discovery complete: {result.teams_discovered} teams ({result.teams_new} new), "
                f"{result.players_discovered} players ({result.players_new} new)"
            )
            
            return result
            
        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise
    
    async def _run_profile_fetch(
        self,
        entities: list[tuple[str, int]],
        batch_size: int = 10,
    ) -> int:
        """
        Phase 2: Profile Fetch - Fetch full profiles for new entities.
        """
        if not entities:
            return 0
        
        sync_id = self._start_sync("incremental", "profiles")
        fetched = 0
        
        try:
            teams = [(et, eid) for et, eid in entities if et == "team"]
            players = [(et, eid) for et, eid in entities if et == "player"]
            
            # Fetch team profiles
            if teams:
                async def fetch_team(entity):
                    _, team_id = entity
                    profile = await self.provider.fetch_team_profile(self.sport, team_id)
                    if profile:
                        self.repos.teams.upsert(
                            self.sport,
                            profile.canonical_data,
                            profile.raw_response,
                            mark_profile_fetched=True,
                        )
                        return True
                    return False
                
                results = await run_parallel_batches(teams, fetch_team, batch_size)
                fetched += sum(1 for _, r, e in results if r and not e)
            
            # Fetch player profiles
            if players:
                async def fetch_player(entity):
                    _, player_id = entity
                    profile = await self.provider.fetch_player_profile(self.sport, player_id)
                    if profile:
                        self.repos.players.upsert(
                            self.sport,
                            profile.canonical_data,
                            profile.raw_response,
                            mark_profile_fetched=True,
                        )
                        return True
                    return False
                
                results = await run_parallel_batches(players, fetch_player, batch_size)
                fetched += sum(1 for _, r, e in results if r and not e)
            
            self._complete_sync(sync_id, len(entities), fetched, 0)
            logger.info(f"Profile fetch complete: {fetched}/{len(entities)} entities")
            return fetched
            
        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise
    
    async def _seed_player_stats(
        self,
        season: int,
        season_id: int,
        *,
        league_id: Optional[int] = None,
        batch_size: int = 10,
    ) -> int:
        """
        Phase 3a: Update player statistics.
        """
        sync_id = self._start_sync("incremental", "player_stats", season_id)
        updated = 0
        
        try:
            # Get all player IDs
            player_ids = self.repos.players.get_all_ids(self.sport)
            
            async def fetch_and_upsert_stats(player_id: int):
                stats = await self.provider.fetch_player_stats(
                    self.sport, player_id, season, league_id=league_id
                )
                if stats:
                    # Add season_id to stats
                    stats.canonical_data["season_id"] = season_id
                    self.repos.player_stats.upsert(
                        self.sport,
                        stats.canonical_data,
                        stats.raw_response,
                    )
                    return True
                return False
            
            results = await run_parallel_batches(
                player_ids, 
                fetch_and_upsert_stats, 
                batch_size
            )
            updated = sum(1 for _, r, e in results if r and not e)
            
            self._complete_sync(sync_id, len(player_ids), updated, 0)
            logger.info(f"Player stats updated: {updated}/{len(player_ids)}")
            return updated
            
        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise
    
    async def _seed_team_stats(
        self,
        season: int,
        season_id: int,
        *,
        league_id: Optional[int] = None,
    ) -> int:
        """
        Phase 3b: Update team statistics.
        """
        sync_id = self._start_sync("incremental", "team_stats", season_id)
        updated = 0
        
        try:
            team_ids = self.repos.teams.get_all_ids(self.sport)
            
            for team_id in team_ids:
                try:
                    stats = await self.provider.fetch_team_stats(
                        self.sport, team_id, season, league_id=league_id
                    )
                    if stats:
                        stats.canonical_data["season_id"] = season_id
                        self.repos.team_stats.upsert(
                            self.sport,
                            stats.canonical_data,
                            stats.raw_response,
                        )
                        updated += 1
                except Exception as e:
                    logger.warning(f"Failed to fetch stats for team {team_id}: {e}")
            
            self._complete_sync(sync_id, len(team_ids), updated, 0)
            logger.info(f"Team stats updated: {updated}/{len(team_ids)}")
            return updated
            
        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise
    
    # ==========================================================================
    # Debug/Limited Seeding
    # ==========================================================================
    
    async def seed_debug(
        self,
        season: int,
        *,
        league_id: Optional[int] = None,
        max_teams: int = 5,
        max_players: int = 25,
    ) -> SeedingResult:
        """
        Run limited seeding for debugging.
        
        Fetches a small subset of data for testing.
        """
        start_time = datetime.now()
        result = SeedingResult(
            sport=self.sport,
            season=season,
            discovery=DiscoveryResult(),
        )
        
        season_id = self.ensure_season(season)
        
        # Fetch limited teams
        teams = await self.provider.fetch_teams(self.sport, season, league_id=league_id)
        teams = teams[:max_teams]
        
        for team in teams:
            self.repos.teams.upsert(
                self.sport,
                team.canonical_data,
                team.raw_response,
            )
            result.discovery.teams_discovered += 1
        
        # Fetch limited players (from first team)
        if teams:
            team_id = int(teams[0].provider_id)
            players = await self.provider.fetch_players(
                self.sport, season, team_id=team_id, league_id=league_id
            )
            players = players[:max_players]
            
            for player in players:
                self.repos.players.upsert(
                    self.sport,
                    player.canonical_data,
                    player.raw_response,
                )
                result.discovery.players_discovered += 1
        
        result.duration_seconds = (datetime.now() - start_time).total_seconds()
        
        logger.info(
            f"Debug seeding complete: {result.discovery.teams_discovered} teams, "
            f"{result.discovery.players_discovered} players"
        )
        
        return result
