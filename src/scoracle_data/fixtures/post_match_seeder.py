"""
Post-match seeder for targeted stat updates.

After a match completes (+ seed delay), this seeder:
1. Fetches rosters for both teams
2. Fetches updated stats for all players who participated
3. Batch upserts player stats
4. Updates team stats
5. Recalculates percentiles (optional)
6. Marks fixture as seeded

This is much more efficient than full-league seeding since it only
updates ~40-50 players (both team rosters) per match.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB
    from ..api_client import StandaloneApiClient

logger = logging.getLogger(__name__)


@dataclass
class PostMatchResult:
    """Result of a post-match seeding operation."""
    fixture_id: int
    sport_id: str
    home_team_id: int
    away_team_id: int
    players_updated: int = 0
    teams_updated: int = 0
    percentiles_recalculated: bool = False
    success: bool = True
    error: Optional[str] = None
    duration_seconds: float = 0.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "fixture_id": self.fixture_id,
            "sport_id": self.sport_id,
            "home_team_id": self.home_team_id,
            "away_team_id": self.away_team_id,
            "players_updated": self.players_updated,
            "teams_updated": self.teams_updated,
            "percentiles_recalculated": self.percentiles_recalculated,
            "success": self.success,
            "error": self.error,
            "duration_seconds": self.duration_seconds,
        }


class PostMatchSeeder:
    """
    Seeder for post-match stat updates.

    Designed for schedule-driven seeding where stats are updated
    after each match completes, rather than bulk updates.
    """

    def __init__(
        self,
        db: "PostgresDB",
        api: "StandaloneApiClient",
    ):
        """
        Initialize the seeder.

        Args:
            db: PostgreSQL database connection
            api: API-Sports client for fetching stats
        """
        self.db = db
        self.api = api

    async def seed_fixture(
        self,
        fixture_id: int,
        recalculate_percentiles: bool = True,
    ) -> PostMatchResult:
        """
        Seed stats for a specific fixture.

        Args:
            fixture_id: Fixture ID to seed
            recalculate_percentiles: If True, recalculate percentiles after update

        Returns:
            PostMatchResult with counts and status
        """
        start_time = datetime.now()

        # Get fixture details
        fixture = self.db.fetchone(
            """
            SELECT
                f.id, f.sport_id, f.league_id, f.season_id,
                f.home_team_id, f.away_team_id, f.status,
                s.season_year
            FROM fixtures f
            JOIN seasons s ON s.id = f.season_id
            WHERE f.id = %s
            """,
            (fixture_id,),
        )

        if not fixture:
            return PostMatchResult(
                fixture_id=fixture_id,
                sport_id="",
                home_team_id=0,
                away_team_id=0,
                success=False,
                error=f"Fixture {fixture_id} not found",
            )

        result = PostMatchResult(
            fixture_id=fixture_id,
            sport_id=fixture["sport_id"],
            home_team_id=fixture["home_team_id"],
            away_team_id=fixture["away_team_id"],
        )

        try:
            # Get the appropriate seeder for this sport
            seeder = self._get_sport_seeder(fixture["sport_id"])
            if not seeder:
                raise ValueError(f"No seeder available for sport: {fixture['sport_id']}")

            # Get rosters for both teams
            home_roster = await self._get_team_roster(
                fixture["sport_id"],
                fixture["home_team_id"],
                fixture["season_year"],
            )
            away_roster = await self._get_team_roster(
                fixture["sport_id"],
                fixture["away_team_id"],
                fixture["season_year"],
            )

            all_players = home_roster + away_roster
            logger.info(
                f"Fixture {fixture_id}: Found {len(home_roster)} home + {len(away_roster)} away players"
            )

            # Fetch and update player stats
            player_stats_to_upsert = []
            for player in all_players:
                try:
                    raw_stats = await seeder.fetch_player_stats(
                        player["id"],
                        fixture["season_year"],
                        league_id=fixture.get("league_id"),
                    )
                    if raw_stats:
                        transformed = seeder.transform_player_stats(
                            raw_stats,
                            player["id"],
                            fixture["season_id"],
                            player.get("team_id"),
                        )
                        if transformed:
                            player_stats_to_upsert.append(transformed)
                except Exception as e:
                    logger.debug(f"Failed to fetch stats for player {player['id']}: {e}")

            # Batch upsert player stats
            if player_stats_to_upsert:
                result.players_updated = self._batch_upsert_player_stats(
                    seeder,
                    player_stats_to_upsert,
                )

            # Update team stats for both teams
            for team_id in [fixture["home_team_id"], fixture["away_team_id"]]:
                try:
                    raw_stats = await seeder.fetch_team_stats(
                        team_id,
                        fixture["season_year"],
                        league_id=fixture.get("league_id"),
                    )
                    if raw_stats:
                        transformed = seeder.transform_team_stats(
                            raw_stats,
                            team_id,
                            fixture["season_id"],
                        )
                        seeder.upsert_team_stats(transformed)
                        result.teams_updated += 1
                except Exception as e:
                    logger.warning(f"Failed to update team stats for {team_id}: {e}")

            # Recalculate percentiles if requested
            if recalculate_percentiles and result.players_updated > 0:
                try:
                    from ..percentiles.pg_calculator import PostgresPercentileCalculator
                    calculator = PostgresPercentileCalculator(self.db)
                    calculator.recalculate_all_percentiles(
                        fixture["sport_id"],
                        fixture["season_year"],
                    )
                    result.percentiles_recalculated = True
                except Exception as e:
                    logger.warning(f"Percentile recalculation failed: {e}")

            # Mark fixture as seeded
            self.db.execute(
                """
                UPDATE fixtures
                SET status = 'seeded', seeded_at = NOW(), updated_at = NOW()
                WHERE id = %s
                """,
                (fixture_id,),
            )

            result.success = True

        except Exception as e:
            result.success = False
            result.error = str(e)
            logger.error(f"Failed to seed fixture {fixture_id}: {e}")

            # Record failure
            self.db.execute(
                """
                UPDATE fixtures
                SET seed_attempts = seed_attempts + 1,
                    last_seed_error = %s,
                    updated_at = NOW()
                WHERE id = %s
                """,
                (str(e), fixture_id),
            )

        result.duration_seconds = (datetime.now() - start_time).total_seconds()

        logger.info(
            f"Fixture {fixture_id} seeding {'completed' if result.success else 'failed'}: "
            f"{result.players_updated} players, {result.teams_updated} teams "
            f"in {result.duration_seconds:.1f}s"
        )

        return result

    async def seed_fixtures_batch(
        self,
        fixture_ids: list[int],
        recalculate_percentiles: bool = True,
    ) -> list[PostMatchResult]:
        """
        Seed multiple fixtures sequentially.

        Args:
            fixture_ids: List of fixture IDs to seed
            recalculate_percentiles: If True, recalculate percentiles after all updates

        Returns:
            List of PostMatchResult for each fixture
        """
        results = []

        # Process fixtures, but only recalculate percentiles at the end
        for i, fixture_id in enumerate(fixture_ids):
            is_last = (i == len(fixture_ids) - 1)
            result = await self.seed_fixture(
                fixture_id,
                recalculate_percentiles=(recalculate_percentiles and is_last),
            )
            results.append(result)

        return results

    def _get_sport_seeder(self, sport_id: str):
        """Get the appropriate seeder for a sport."""
        from ..seeders import NBASeeder, NFLSeeder, FootballSeeder

        seeder_map = {
            "NBA": NBASeeder,
            "NFL": NFLSeeder,
            "FOOTBALL": FootballSeeder,
        }

        seeder_class = seeder_map.get(sport_id)
        if seeder_class:
            return seeder_class(self.db, self.api)
        return None

    async def _get_team_roster(
        self,
        sport_id: str,
        team_id: int,
        season_year: int,
    ) -> list[dict[str, Any]]:
        """
        Get the roster for a team.

        First tries the database, falls back to API if needed.
        """
        # Try database first
        players = self.db.fetchall(
            """
            SELECT id, full_name, position, current_team_id as team_id
            FROM players
            WHERE current_team_id = %s AND sport_id = %s
            """,
            (team_id, sport_id),
        )

        if players:
            return [dict(p) for p in players]

        # Fall back to API
        try:
            api_players = await self.api.list_players(
                sport_id,
                season=str(season_year),
                team_id=team_id,
            )
            return [
                {
                    "id": p.get("id"),
                    "full_name": p.get("name") or f"{p.get('firstname', '')} {p.get('lastname', '')}".strip(),
                    "team_id": team_id,
                }
                for p in api_players
                if p.get("id")
            ]
        except Exception as e:
            logger.warning(f"Failed to fetch roster for team {team_id}: {e}")
            return []

    def _batch_upsert_player_stats(
        self,
        seeder,
        stats_list: list[dict[str, Any]],
    ) -> int:
        """
        Batch upsert player stats.

        Uses the seeder's upsert method but batches the operations.
        """
        count = 0
        for stats in stats_list:
            try:
                seeder.upsert_player_stats(stats)
                count += 1
            except Exception as e:
                logger.debug(f"Failed to upsert stats: {e}")
        return count


class PostMatchSeederByMatch:
    """
    Alternative seeder that fetches stats by match/fixture ID from API-Sports.

    Some API-Sports endpoints support fetching stats by fixture ID,
    which can be more efficient than fetching by player.
    """

    def __init__(
        self,
        db: "PostgresDB",
        api: "StandaloneApiClient",
    ):
        self.db = db
        self.api = api

    async def seed_by_external_fixture_id(
        self,
        external_fixture_id: int,
        sport_id: str,
    ) -> PostMatchResult:
        """
        Seed stats using the external fixture ID from API-Sports.

        This method fetches all player statistics for a match in a single API call,
        which is more efficient than fetching per-player.

        Args:
            external_fixture_id: The fixture ID from API-Sports
            sport_id: Sport identifier

        Returns:
            PostMatchResult
        """
        # Get fixture from database by external ID
        fixture = self.db.fetchone(
            """
            SELECT
                f.id, f.sport_id, f.league_id, f.season_id,
                f.home_team_id, f.away_team_id,
                s.season_year
            FROM fixtures f
            JOIN seasons s ON s.id = f.season_id
            WHERE f.external_id = %s
            """,
            (external_fixture_id,),
        )

        if not fixture:
            return PostMatchResult(
                fixture_id=0,
                sport_id=sport_id,
                home_team_id=0,
                away_team_id=0,
                success=False,
                error=f"Fixture with external_id {external_fixture_id} not found",
            )

        # For now, delegate to the standard seeder
        # In the future, this could use fixture-specific API endpoints
        seeder = PostMatchSeeder(self.db, self.api)
        return await seeder.seed_fixture(fixture["id"])
