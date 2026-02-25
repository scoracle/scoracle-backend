"""
Fixture management for schedule-driven seeding.

This module provides:
- FixtureLoader: Import match schedules from CSV/JSON
- PostMatchSeeder: Update stats for specific fixtures
- SchedulerService: Find fixtures ready for seeding

Workflow:
    1. Load schedule at season start: loader.load_from_csv("schedule.csv")
    2. Run scheduler periodically: scheduler.process_pending_fixtures()
    3. Or seed manually: seeder.seed_fixture(fixture_id)
"""

from .loader import FixtureLoader
from .post_match_seeder import PostMatchSeeder
from .scheduler import SchedulerService

__all__ = ["FixtureLoader", "PostMatchSeeder", "SchedulerService"]
