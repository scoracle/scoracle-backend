"""
Scheduler service for automatic fixture seeding.

This module provides the SchedulerService class that:
1. Finds fixtures ready to seed (start_time + delay has passed)
2. Triggers post-match seeding for each
3. Handles retries and error logging
4. Reports progress for monitoring

Designed to be called by:
- Cron jobs (run every 15-30 minutes)
- Celery beat tasks
- Manual CLI invocation
- Cloud scheduler (AWS Lambda, Google Cloud Functions, etc.)

Usage:
    from scoracle_data.fixtures import SchedulerService

    async def scheduled_task():
        scheduler = SchedulerService(db, api)
        result = await scheduler.process_pending_fixtures()
        print(f"Processed {result.processed} fixtures")
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB
    from ..api_client import StandaloneApiClient

from .post_match_seeder import PostMatchSeeder, PostMatchResult

# Sport-specific team profile table mappings for fixture queries
# These are needed because fixtures can reference teams from any sport
TEAM_PROFILE_TABLES_FOR_FIXTURES = {
    "NBA": "nba_team_profiles",
    "NFL": "nfl_team_profiles",
    "FOOTBALL": "football_team_profiles",
}

logger = logging.getLogger(__name__)


@dataclass
class SchedulerResult:
    """Result of a scheduler run."""
    run_started: datetime = field(default_factory=datetime.now)
    run_completed: Optional[datetime] = None
    fixtures_found: int = 0
    fixtures_processed: int = 0
    fixtures_succeeded: int = 0
    fixtures_failed: int = 0
    total_players_updated: int = 0
    total_teams_updated: int = 0
    total_duration_seconds: float = 0.0
    errors: list[str] = field(default_factory=list)
    results: list[PostMatchResult] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "run_started": self.run_started.isoformat(),
            "run_completed": self.run_completed.isoformat() if self.run_completed else None,
            "fixtures_found": self.fixtures_found,
            "fixtures_processed": self.fixtures_processed,
            "fixtures_succeeded": self.fixtures_succeeded,
            "fixtures_failed": self.fixtures_failed,
            "total_players_updated": self.total_players_updated,
            "total_teams_updated": self.total_teams_updated,
            "total_duration_seconds": self.total_duration_seconds,
            "errors": self.errors,
            "success_rate": (
                self.fixtures_succeeded / self.fixtures_processed * 100
                if self.fixtures_processed > 0 else 0
            ),
        }


class SchedulerService:
    """
    Service for automated fixture seeding based on schedule.

    Designed to run periodically (e.g., every 15-30 minutes) and:
    1. Find fixtures that are past their seed time
    2. Seed stats for those fixtures
    3. Update fixture status
    4. Log results for monitoring
    """

    def __init__(
        self,
        db: "PostgresDB",
        api: "StandaloneApiClient",
        max_fixtures_per_run: int = 10,
        max_retries: int = 3,
    ):
        """
        Initialize the scheduler.

        Args:
            db: PostgreSQL database connection
            api: API-Sports client
            max_fixtures_per_run: Maximum fixtures to process in one run
            max_retries: Skip fixtures with more than this many failed attempts
        """
        self.db = db
        self.api = api
        self.max_fixtures_per_run = max_fixtures_per_run
        self.max_retries = max_retries
        self.seeder = PostMatchSeeder(db, api)

    async def process_pending_fixtures(
        self,
        sport_id: Optional[str] = None,
        recalculate_percentiles: bool = True,
    ) -> SchedulerResult:
        """
        Process all pending fixtures that are ready to seed.

        This is the main entry point for scheduled runs.

        Args:
            sport_id: Optional sport filter (NBA, NFL, FOOTBALL)
            recalculate_percentiles: Whether to recalculate percentiles after seeding

        Returns:
            SchedulerResult with counts and details
        """
        result = SchedulerResult()

        try:
            # Find pending fixtures
            pending = self._get_pending_fixtures(sport_id)
            result.fixtures_found = len(pending)

            if not pending:
                logger.info("No pending fixtures to seed")
                result.run_completed = datetime.now()
                result.total_duration_seconds = (
                    result.run_completed - result.run_started
                ).total_seconds()
                return result

            logger.info(f"Found {len(pending)} fixtures ready to seed")

            # Process each fixture
            for fixture in pending:
                try:
                    fixture_result = await self.seeder.seed_fixture(
                        fixture["id"],
                        recalculate_percentiles=recalculate_percentiles,
                    )
                    result.results.append(fixture_result)
                    result.fixtures_processed += 1

                    if fixture_result.success:
                        result.fixtures_succeeded += 1
                        result.total_players_updated += fixture_result.players_updated
                        result.total_teams_updated += fixture_result.teams_updated
                        logger.info(
                            f"Seeded fixture {fixture['id']}: "
                            f"{fixture_result.players_updated} players, "
                            f"{fixture_result.teams_updated} teams"
                        )
                    else:
                        result.fixtures_failed += 1
                        result.errors.append(
                            f"Fixture {fixture['id']}: {fixture_result.error}"
                        )
                        logger.warning(
                            f"Failed to seed fixture {fixture['id']}: {fixture_result.error}"
                        )

                except Exception as e:
                    result.fixtures_processed += 1
                    result.fixtures_failed += 1
                    result.errors.append(f"Fixture {fixture['id']}: {str(e)}")
                    logger.error(f"Error seeding fixture {fixture['id']}: {e}")

        except Exception as e:
            result.errors.append(f"Scheduler error: {str(e)}")
            logger.error(f"Scheduler run failed: {e}")

        result.run_completed = datetime.now()
        result.total_duration_seconds = (
            result.run_completed - result.run_started
        ).total_seconds()

        # Log summary
        logger.info(
            f"Scheduler run completed: "
            f"{result.fixtures_succeeded}/{result.fixtures_processed} succeeded, "
            f"{result.total_players_updated} players updated, "
            f"{result.total_teams_updated} teams updated, "
            f"took {result.total_duration_seconds:.1f}s"
        )

        return result

    def _get_pending_fixtures(self, sport_id: Optional[str] = None) -> list[dict]:
        """
        Get fixtures that are ready to seed.

        A fixture is ready when:
        - Status is 'scheduled' or 'completed'
        - Current time > start_time + seed_delay_hours
        - seed_attempts < max_retries
        """
        query = """
            SELECT
                f.id,
                f.sport_id,
                f.league_id,
                f.season_id,
                f.home_team_id,
                f.away_team_id,
                f.start_time,
                f.seed_delay_hours,
                f.seed_attempts,
                f.external_id
            FROM fixtures f
            WHERE (f.status = 'scheduled' OR f.status = 'completed')
              AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
              AND f.seed_attempts < %s
        """
        params = [self.max_retries]

        if sport_id:
            query += " AND f.sport_id = %s"
            params.append(sport_id)

        query += """
            ORDER BY f.start_time ASC
            LIMIT %s
        """
        params.append(self.max_fixtures_per_run)

        rows = self.db.fetchall(query, tuple(params))
        return [dict(row) for row in rows]

    def get_pending_count(self, sport_id: Optional[str] = None) -> dict[str, int]:
        """
        Get count of pending fixtures by sport.

        Useful for monitoring and dashboards.

        Returns:
            Dict mapping sport_id to count
        """
        query = """
            SELECT sport_id, COUNT(*) as count
            FROM fixtures
            WHERE (status = 'scheduled' OR status = 'completed')
              AND NOW() >= start_time + (seed_delay_hours || ' hours')::INTERVAL
              AND seed_attempts < %s
        """
        params = [self.max_retries]

        if sport_id:
            query += " AND sport_id = %s"
            params.append(sport_id)

        query += " GROUP BY sport_id"

        rows = self.db.fetchall(query, tuple(params))
        return {row["sport_id"]: row["count"] for row in rows}

    def get_upcoming_fixtures(
        self,
        sport_id: Optional[str] = None,
        hours_ahead: int = 24,
        limit: int = 50,
    ) -> list[dict]:
        """
        Get fixtures scheduled in the next N hours.

        Useful for monitoring what's coming up.

        Args:
            sport_id: Optional sport filter
            hours_ahead: How far ahead to look
            limit: Maximum fixtures to return

        Returns:
            List of upcoming fixture details
        """
        # Use COALESCE to get team names from sport-specific profile tables
        # This handles fixtures from any sport (NBA, NFL, FOOTBALL)
        query = """
            SELECT
                f.id,
                f.sport_id,
                f.external_id,
                f.start_time,
                f.seed_delay_hours,
                f.status,
                COALESCE(nba_ht.name, nfl_ht.name, fb_ht.name) as home_team_name,
                COALESCE(nba_at.name, nfl_at.name, fb_at.name) as away_team_name,
                s.season_year
            FROM fixtures f
            LEFT JOIN nba_team_profiles nba_ht ON nba_ht.id = f.home_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nba_team_profiles nba_at ON nba_at.id = f.away_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nfl_team_profiles nfl_ht ON nfl_ht.id = f.home_team_id AND f.sport_id = 'NFL'
            LEFT JOIN nfl_team_profiles nfl_at ON nfl_at.id = f.away_team_id AND f.sport_id = 'NFL'
            LEFT JOIN football_team_profiles fb_ht ON fb_ht.id = f.home_team_id AND f.sport_id = 'FOOTBALL'
            LEFT JOIN football_team_profiles fb_at ON fb_at.id = f.away_team_id AND f.sport_id = 'FOOTBALL'
            JOIN seasons s ON s.id = f.season_id
            WHERE f.status = 'scheduled'
              AND f.start_time BETWEEN NOW() AND NOW() + (%s || ' hours')::INTERVAL
        """
        params = [hours_ahead]

        if sport_id:
            query += " AND f.sport_id = %s"
            params.append(sport_id)

        query += """
            ORDER BY f.start_time ASC
            LIMIT %s
        """
        params.append(limit)

        rows = self.db.fetchall(query, tuple(params))
        return [dict(row) for row in rows]

    def get_recent_seeds(
        self,
        sport_id: Optional[str] = None,
        hours_back: int = 24,
        limit: int = 50,
    ) -> list[dict]:
        """
        Get recently seeded fixtures.

        Useful for verifying scheduled seeding is working.

        Args:
            sport_id: Optional sport filter
            hours_back: How far back to look
            limit: Maximum fixtures to return

        Returns:
            List of recently seeded fixture details
        """
        # Use COALESCE to get team names from sport-specific profile tables
        query = """
            SELECT
                f.id,
                f.sport_id,
                f.external_id,
                f.start_time,
                f.seeded_at,
                f.status,
                COALESCE(nba_ht.name, nfl_ht.name, fb_ht.name) as home_team_name,
                COALESCE(nba_at.name, nfl_at.name, fb_at.name) as away_team_name,
                s.season_year
            FROM fixtures f
            LEFT JOIN nba_team_profiles nba_ht ON nba_ht.id = f.home_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nba_team_profiles nba_at ON nba_at.id = f.away_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nfl_team_profiles nfl_ht ON nfl_ht.id = f.home_team_id AND f.sport_id = 'NFL'
            LEFT JOIN nfl_team_profiles nfl_at ON nfl_at.id = f.away_team_id AND f.sport_id = 'NFL'
            LEFT JOIN football_team_profiles fb_ht ON fb_ht.id = f.home_team_id AND f.sport_id = 'FOOTBALL'
            LEFT JOIN football_team_profiles fb_at ON fb_at.id = f.away_team_id AND f.sport_id = 'FOOTBALL'
            JOIN seasons s ON s.id = f.season_id
            WHERE f.status = 'seeded'
              AND f.seeded_at >= NOW() - (%s || ' hours')::INTERVAL
        """
        params = [hours_back]

        if sport_id:
            query += " AND f.sport_id = %s"
            params.append(sport_id)

        query += """
            ORDER BY f.seeded_at DESC
            LIMIT %s
        """
        params.append(limit)

        rows = self.db.fetchall(query, tuple(params))
        return [dict(row) for row in rows]

    def get_failed_fixtures(
        self,
        sport_id: Optional[str] = None,
        limit: int = 50,
    ) -> list[dict]:
        """
        Get fixtures that have failed seeding attempts.

        Useful for debugging and manual intervention.

        Args:
            sport_id: Optional sport filter
            limit: Maximum fixtures to return

        Returns:
            List of failed fixture details
        """
        # Use COALESCE to get team names from sport-specific profile tables
        query = """
            SELECT
                f.id,
                f.sport_id,
                f.external_id,
                f.start_time,
                f.status,
                f.seed_attempts,
                f.last_seed_error,
                COALESCE(nba_ht.name, nfl_ht.name, fb_ht.name) as home_team_name,
                COALESCE(nba_at.name, nfl_at.name, fb_at.name) as away_team_name,
                s.season_year
            FROM fixtures f
            LEFT JOIN nba_team_profiles nba_ht ON nba_ht.id = f.home_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nba_team_profiles nba_at ON nba_at.id = f.away_team_id AND f.sport_id = 'NBA'
            LEFT JOIN nfl_team_profiles nfl_ht ON nfl_ht.id = f.home_team_id AND f.sport_id = 'NFL'
            LEFT JOIN nfl_team_profiles nfl_at ON nfl_at.id = f.away_team_id AND f.sport_id = 'NFL'
            LEFT JOIN football_team_profiles fb_ht ON fb_ht.id = f.home_team_id AND f.sport_id = 'FOOTBALL'
            LEFT JOIN football_team_profiles fb_at ON fb_at.id = f.away_team_id AND f.sport_id = 'FOOTBALL'
            JOIN seasons s ON s.id = f.season_id
            WHERE f.seed_attempts > 0
              AND f.status != 'seeded'
        """
        params = []

        if sport_id:
            query += " AND f.sport_id = %s"
            params.append(sport_id)

        query += """
            ORDER BY f.seed_attempts DESC, f.start_time DESC
            LIMIT %s
        """
        params.append(limit)

        rows = self.db.fetchall(query, tuple(params) if params else (limit,))
        return [dict(row) for row in rows]

    def reset_fixture_for_retry(self, fixture_id: int) -> bool:
        """
        Reset a fixture to allow retry after manual intervention.

        Args:
            fixture_id: Fixture ID to reset

        Returns:
            True if reset, False if fixture not found
        """
        result = self.db.fetchone(
            """
            UPDATE fixtures
            SET seed_attempts = 0,
                last_seed_error = NULL,
                status = 'completed',
                updated_at = NOW()
            WHERE id = %s
            RETURNING id
            """,
            (fixture_id,),
        )
        return result is not None

    def get_fixture_status_summary(self, sport_id: Optional[str] = None) -> dict[str, int]:
        """
        Get count of fixtures by status.

        Useful for dashboards and monitoring.

        Returns:
            Dict mapping status to count
        """
        query = """
            SELECT status, COUNT(*) as count
            FROM fixtures
        """
        params = []

        if sport_id:
            query += " WHERE sport_id = %s"
            params.append(sport_id)

        query += " GROUP BY status ORDER BY status"

        rows = self.db.fetchall(query, tuple(params) if params else None)
        return {row["status"]: row["count"] for row in rows}
