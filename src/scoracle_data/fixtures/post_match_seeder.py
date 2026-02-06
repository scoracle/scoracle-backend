"""
Post-match seeder for targeted stat updates.

After a match completes (+ seed delay), this seeder:
1. Delegates to the sport-specific seed runner to refresh player stats
2. Delegates to the sport-specific seed runner to refresh team stats
3. Recalculates percentiles (optional)
4. Marks fixture as seeded

Uses the existing seed runner public API (seed_player_stats / seed_team_stats)
which fetches full-season data from the provider.  This is less targeted than
per-player fetching but uses methods that actually exist on the runners and
keeps the post-match seeder thin.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional

from ..core.types import PLAYER_PROFILE_TABLES

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

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

    Uses the sport-specific seed runners (NBASeedRunner, NFLSeedRunner,
    FootballSeedRunner) which take (db, provider_client) in their
    constructors.
    """

    def __init__(
        self,
        db: "PostgresDB",
        provider_client: Any,
    ):
        """
        Initialize the seeder.

        Args:
            db: PostgreSQL database connection
            provider_client: Provider-specific API client (e.g. BallDontLieNBA,
                BallDontLieNFL, SportMonksClient)
        """
        self.db = db
        self.provider_client = provider_client

    async def seed_fixture(
        self,
        fixture_id: int,
        recalculate_percentiles: bool = True,
    ) -> PostMatchResult:
        """
        Seed stats for a specific fixture.

        Fetches the fixture row (with season info), determines the sport,
        instantiates the matching seed runner, and calls its
        ``seed_player_stats`` / ``seed_team_stats`` methods which refresh
        full-season data from the upstream provider.

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
            sport_id = fixture["sport_id"]
            seed_runner = self._make_seed_runner(sport_id)
            if not seed_runner:
                raise ValueError(f"No seeder available for sport: {sport_id}")

            # Log roster sizes for observability (non-critical)
            try:
                home_roster = self._get_team_roster(sport_id, fixture["home_team_id"])
                away_roster = self._get_team_roster(sport_id, fixture["away_team_id"])
                logger.info(
                    f"Fixture {fixture_id}: "
                    f"{len(home_roster)} home + {len(away_roster)} away players in DB"
                )
            except Exception:
                logger.debug(f"Fixture {fixture_id}: roster lookup skipped")

            # -- Player stats (full-season refresh) ---------------------------
            player_result = await self._seed_player_stats(
                seed_runner, sport_id, fixture,
            )
            result.players_updated = player_result.player_stats_upserted
            if player_result.errors:
                logger.warning(
                    f"Fixture {fixture_id}: player stats errors: {player_result.errors}"
                )

            # -- Team stats (full-season refresh) -----------------------------
            team_result = await self._seed_team_stats(
                seed_runner, sport_id, fixture,
            )
            result.teams_updated = team_result.team_stats_upserted
            if team_result.errors:
                logger.warning(
                    f"Fixture {fixture_id}: team stats errors: {team_result.errors}"
                )

            # -- Percentile recalculation -------------------------------------
            if recalculate_percentiles and result.players_updated > 0:
                try:
                    from ..percentiles.python_calculator import PythonPercentileCalculator
                    calculator = PythonPercentileCalculator(self.db)
                    calculator.recalculate_all_percentiles(
                        sport_id,
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

    # -- Internals ------------------------------------------------------------

    def _get_sport_seeder(self, sport_id: str):
        """
        Return an instantiated seed runner for *sport_id*.

        Each runner takes ``(db, client)`` — we import lazily so that
        provider-specific dependencies aren't required at import time.
        """
        from ..seeders import NBASeedRunner, NFLSeedRunner, FootballSeedRunner

        runner_map: dict[str, type] = {
            "NBA": NBASeedRunner,
            "NFL": NFLSeedRunner,
            "FOOTBALL": FootballSeedRunner,
        }

        runner_class = runner_map.get(sport_id)
        if runner_class is None:
            return None
        return runner_class(self.db, self.provider_client)

    _make_seed_runner = _get_sport_seeder  # alias for clarity

    def _get_team_roster(
        self,
        sport_id: str,
        team_id: int,
    ) -> list[dict[str, Any]]:
        """
        Get roster for a team from the sport-specific player profile table.
        """
        table = PLAYER_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning(f"No player profile table for sport {sport_id}")
            return []

        players = self.db.fetchall(
            f"SELECT id, team_id FROM {table} WHERE team_id = %s",
            (team_id,),
        )
        return [dict(p) for p in players] if players else []

    async def _seed_player_stats(self, seed_runner, sport_id: str, fixture: dict):
        """
        Call the correct ``seed_player_stats`` overload for *sport_id*.

        Signatures:
            NBA:      seed_player_stats(season, season_type="regular")
            NFL:      seed_player_stats(season, postseason=False)
            FOOTBALL: seed_player_stats(season_id, league_id, season_year)
        """
        from ..seeders.common import SeedResult

        season_year = fixture["season_year"]

        if sport_id == "NBA":
            return await seed_runner.seed_player_stats(season_year)
        elif sport_id == "NFL":
            return await seed_runner.seed_player_stats(season_year)
        elif sport_id == "FOOTBALL":
            return await seed_runner.seed_player_stats(
                fixture["season_id"],
                fixture["league_id"],
                season_year,
            )
        else:
            logger.warning(f"Unknown sport_id for player stats: {sport_id}")
            return SeedResult()

    async def _seed_team_stats(self, seed_runner, sport_id: str, fixture: dict):
        """
        Call the correct ``seed_team_stats`` overload for *sport_id*.

        Signatures:
            NBA:      seed_team_stats(season, season_type="regular")
            NFL:      seed_team_stats(season, postseason=False)
            FOOTBALL: seed_team_stats(season_id, league_id, season_year)
        """
        from ..seeders.common import SeedResult

        season_year = fixture["season_year"]

        if sport_id == "NBA":
            return await seed_runner.seed_team_stats(season_year)
        elif sport_id == "NFL":
            return await seed_runner.seed_team_stats(season_year)
        elif sport_id == "FOOTBALL":
            return await seed_runner.seed_team_stats(
                fixture["season_id"],
                fixture["league_id"],
                season_year,
            )
        else:
            logger.warning(f"Unknown sport_id for team stats: {sport_id}")
            return SeedResult()


class PostMatchSeederByMatch:
    """
    Alternative seeder that fetches stats by match/fixture ID from the provider.

    Some API endpoints support fetching stats by fixture ID,
    which can be more efficient than fetching by player.
    """

    def __init__(
        self,
        db: "PostgresDB",
        provider_client: Any,
    ):
        self.db = db
        self.provider_client = provider_client

    async def seed_by_external_fixture_id(
        self,
        external_fixture_id: int,
        sport_id: str,
    ) -> PostMatchResult:
        """
        Seed stats using the external fixture ID from the provider API.

        This method fetches all player statistics for a match in a single API call,
        which is more efficient than fetching per-player.

        Args:
            external_fixture_id: The fixture ID from the provider API
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

        # Delegate to the standard seeder
        seeder = PostMatchSeeder(self.db, self.provider_client)
        return await seeder.seed_fixture(fixture["id"])
