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

from ..core.types import PLAYERS_TABLE

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


@dataclass
class PostMatchResult:
    """Result of a post-match seeding operation."""

    fixture_id: int
    sport: str
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
            "sport": self.sport,
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

        Fetches the fixture row, determines the sport, instantiates the
        matching seed runner, and calls its ``seed_player_stats`` /
        ``seed_team_stats`` methods which refresh full-season data from
        the upstream provider.

        Args:
            fixture_id: Fixture ID to seed
            recalculate_percentiles: If True, recalculate percentiles after update

        Returns:
            PostMatchResult with counts and status
        """
        start_time = datetime.now()

        # Get fixture details (unified schema: sport, season — no seasons table)
        fixture = self.db.fetchone(
            """
            SELECT id, sport, league_id, season,
                   home_team_id, away_team_id, status
            FROM fixtures
            WHERE id = %s
            """,
            (fixture_id,),
        )

        if not fixture:
            return PostMatchResult(
                fixture_id=fixture_id,
                sport="",
                home_team_id=0,
                away_team_id=0,
                success=False,
                error=f"Fixture {fixture_id} not found",
            )

        sport = fixture["sport"]

        result = PostMatchResult(
            fixture_id=fixture_id,
            sport=sport,
            home_team_id=fixture["home_team_id"],
            away_team_id=fixture["away_team_id"],
        )

        try:
            seed_runner = self._make_seed_runner(sport)
            if not seed_runner:
                raise ValueError(f"No seeder available for sport: {sport}")

            # Log roster sizes for observability (non-critical)
            try:
                home_roster = self._get_team_roster(sport, fixture["home_team_id"])
                away_roster = self._get_team_roster(sport, fixture["away_team_id"])
                logger.info(
                    f"Fixture {fixture_id}: "
                    f"{len(home_roster)} home + {len(away_roster)} away players in DB"
                )
            except Exception:
                logger.debug(f"Fixture {fixture_id}: roster lookup skipped")

            # -- Player stats (full-season refresh) ---------------------------
            player_result = await self._seed_player_stats(
                seed_runner,
                sport,
                fixture,
            )
            result.players_updated = player_result.player_stats_upserted
            if player_result.errors:
                logger.warning(
                    f"Fixture {fixture_id}: player stats errors: {player_result.errors}"
                )

            # -- Team stats (full-season refresh) -----------------------------
            team_result = await self._seed_team_stats(
                seed_runner,
                sport,
                fixture,
            )
            result.teams_updated = team_result.team_stats_upserted
            if team_result.errors:
                logger.warning(
                    f"Fixture {fixture_id}: team stats errors: {team_result.errors}"
                )

            # -- Percentile recalculation -------------------------------------
            if recalculate_percentiles and result.players_updated > 0:
                try:
                    from ..percentiles.python_calculator import (
                        PythonPercentileCalculator,
                    )

                    calculator = PythonPercentileCalculator(self.db)
                    calculator.recalculate_all_percentiles(
                        sport,
                        fixture["season"],
                    )
                    result.percentiles_recalculated = True
                except Exception as e:
                    logger.warning(f"Percentile recalculation failed: {e}")

            # Mark fixture as seeded via Postgres function
            self.db.execute(
                "SELECT mark_fixture_seeded(%s)",
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
            is_last = i == len(fixture_ids) - 1
            result = await self.seed_fixture(
                fixture_id,
                recalculate_percentiles=(recalculate_percentiles and is_last),
            )
            results.append(result)

        return results

    # -- Internals ------------------------------------------------------------

    def _get_sport_seeder(self, sport: str):
        """
        Return an instantiated seed runner for *sport*.

        Each runner takes ``(db, client)`` — we import lazily so that
        provider-specific dependencies aren't required at import time.
        """
        from ..seeders import FootballSeedRunner, NBASeedRunner, NFLSeedRunner

        runner_map: dict[str, type] = {
            "NBA": NBASeedRunner,
            "NFL": NFLSeedRunner,
            "FOOTBALL": FootballSeedRunner,
        }

        runner_class = runner_map.get(sport)
        if runner_class is None:
            return None
        return runner_class(self.db, self.provider_client)

    _make_seed_runner = _get_sport_seeder  # alias for clarity

    def _get_team_roster(
        self,
        sport: str,
        team_id: int,
    ) -> list[dict[str, Any]]:
        """Get roster for a team from the unified players table."""
        players = self.db.fetchall(
            f"SELECT id, team_id FROM {PLAYERS_TABLE} WHERE team_id = %s AND sport = %s",
            (team_id, sport),
        )
        return [dict(p) for p in players] if players else []

    def _resolve_provider_season_id(
        self, league_id: int, season_year: int
    ) -> int | None:
        """
        Resolve a provider season_id from league_id + season year.

        Queries the provider_seasons table (populated by migration 006).
        Returns None if the mapping is unknown (caller should log and skip).
        """
        result = self.db.fetchone(
            "SELECT resolve_provider_season_id(%s, %s) AS season_id",
            (league_id, season_year),
        )
        season_id = result["season_id"] if result else None
        if season_id is None:
            logger.warning(
                f"No provider season_id for league {league_id}, season {season_year}"
            )
        return season_id

    def _resolve_sportmonks_league_id(self, league_id: int) -> int | None:
        """Look up the SportMonks league ID from our internal league ID."""
        result = self.db.fetchone(
            "SELECT sportmonks_id FROM leagues WHERE id = %s",
            (league_id,),
        )
        return result["sportmonks_id"] if result else None

    async def _seed_player_stats(self, seed_runner, sport: str, fixture: dict):
        """
        Call the correct ``seed_player_stats`` overload for *sport*.

        Signatures:
            NBA:      seed_player_stats(season, season_type="regular")
            NFL:      seed_player_stats(season, postseason=False)
            FOOTBALL: seed_player_stats(season_id, league_id, season_year, sportmonks_league_id)
        """
        from ..seeders.common import SeedResult

        season = fixture["season"]

        if sport == "NBA":
            return await seed_runner.seed_player_stats(season)
        elif sport == "NFL":
            return await seed_runner.seed_player_stats(season)
        elif sport == "FOOTBALL":
            league_id = fixture["league_id"]
            sm_season_id = self._resolve_provider_season_id(league_id, season)
            if sm_season_id is None:
                logger.error(
                    f"Cannot seed FOOTBALL player stats: no SportMonks season_id "
                    f"for league {league_id}, season {season}"
                )
                return SeedResult()
            sm_league_id = self._resolve_sportmonks_league_id(league_id)
            if sm_league_id is None:
                logger.error(
                    f"Cannot seed FOOTBALL player stats: no sportmonks_id "
                    f"for league {league_id}"
                )
                return SeedResult()
            return await seed_runner.seed_player_stats(
                sm_season_id,
                league_id,
                season,
                sm_league_id,
            )
        else:
            logger.warning(f"Unknown sport for player stats: {sport}")
            return SeedResult()

    async def _seed_team_stats(self, seed_runner, sport: str, fixture: dict):
        """
        Call the correct ``seed_team_stats`` overload for *sport*.

        Signatures:
            NBA:      seed_team_stats(season, season_type="regular")
            NFL:      seed_team_stats(season, postseason=False)
            FOOTBALL: seed_team_stats(season_id, league_id, season_year)
        """
        from ..seeders.common import SeedResult

        season = fixture["season"]

        if sport == "NBA":
            return await seed_runner.seed_team_stats(season)
        elif sport == "NFL":
            return await seed_runner.seed_team_stats(season)
        elif sport == "FOOTBALL":
            league_id = fixture["league_id"]
            sm_season_id = self._resolve_provider_season_id(league_id, season)
            if sm_season_id is None:
                logger.error(
                    f"Cannot seed FOOTBALL team stats: no SportMonks season_id "
                    f"for league {league_id}, season {season}"
                )
                return SeedResult()
            return await seed_runner.seed_team_stats(
                sm_season_id,
                league_id,
                season,
            )
        else:
            logger.warning(f"Unknown sport for team stats: {sport}")
            return SeedResult()
