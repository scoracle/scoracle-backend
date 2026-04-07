"""Fixture schedule management: loading, querying pending, and processing.

Fixture-driven seeding model: all seeding is triggered by fixtures becoming
ready (start_time + seed_delay_hours <= NOW()).
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from datetime import datetime
from typing import Any

import psycopg

logger = logging.getLogger(__name__)


@dataclass
class FixtureRow:
    """A fixture row from the database."""

    id: int
    sport: str
    league_id: int | None
    season: int
    home_team_id: int
    away_team_id: int
    start_time: datetime
    seed_delay_hours: int
    seed_attempts: int
    external_id: int | None


def get_pending(
    conn: psycopg.Connection,
    sport: str | None = None,
    limit: int | None = None,
    max_retries: int = 3,
) -> list[FixtureRow]:
    """Get fixtures ready for seeding via get_pending_fixtures() SQL function."""
    # Use a large number if no limit specified (unlimited)
    limit_val = limit if limit is not None else 10000
    rows = conn.execute(
        "SELECT * FROM get_pending_fixtures(%s, %s, %s)",
        (sport, limit_val, max_retries),
    ).fetchall()

    return [
        FixtureRow(
            id=r["id"],
            sport=r["sport"],
            league_id=r.get("league_id"),
            season=r["season"],
            home_team_id=r["home_team_id"],
            away_team_id=r["away_team_id"],
            start_time=r["start_time"],
            seed_delay_hours=r["seed_delay_hours"],
            seed_attempts=r["seed_attempts"],
            external_id=r.get("external_id"),
        )
        for r in rows
    ]


def get_by_id(conn: psycopg.Connection, fixture_id: int) -> FixtureRow | None:
    """Get a single fixture by ID."""
    r = conn.execute(
        """SELECT id, sport, league_id, season, home_team_id, away_team_id,
                  start_time, seed_delay_hours, seed_attempts, external_id
           FROM fixtures WHERE id = %s""",
        (fixture_id,),
    ).fetchone()

    if not r:
        return None

    return FixtureRow(
        id=r["id"],
        sport=r["sport"],
        league_id=r.get("league_id"),
        season=r["season"],
        home_team_id=r["home_team_id"],
        away_team_id=r["away_team_id"],
        start_time=r["start_time"],
        seed_delay_hours=r["seed_delay_hours"],
        seed_attempts=r["seed_attempts"],
        external_id=r.get("external_id"),
    )


def record_failure(conn: psycopg.Connection, fixture_id: int, error_msg: str) -> None:
    """Increment seed_attempts and record the error."""
    conn.execute(
        """UPDATE fixtures
           SET seed_attempts = seed_attempts + 1,
               last_seed_error = %s,
               updated_at = NOW()
           WHERE id = %s""",
        (error_msg, fixture_id),
    )


def upsert_fixture(
    conn: psycopg.Connection,
    external_id: int,
    sport: str,
    league_id: int,
    season: int,
    home_team_id: int,
    away_team_id: int,
    start_time: str,
    venue_name: str | None = None,
    round_name: str | None = None,
    seed_delay_hours: int = 4,
) -> int:
    """Upsert a fixture from a provider schedule API. Returns fixture ID."""
    row = conn.execute(
        "SELECT upsert_fixture(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)",
        (
            external_id,
            sport,
            league_id,
            season,
            home_team_id,
            away_team_id,
            start_time,
            venue_name,
            round_name,
            seed_delay_hours,
        ),
    ).fetchone()
    return row["upsert_fixture"] if row else 0


def get_provider_fixture_id(
    conn: psycopg.Connection,
    fixture_id: int,
    provider: str,
    sport: str,
) -> str | None:
    """Resolve provider fixture ID for a canonical fixture."""
    row = conn.execute(
        """
        SELECT provider_fixture_id
        FROM provider_fixture_map
        WHERE fixture_id = %s
          AND provider = %s
          AND sport = %s
        """,
        (fixture_id, provider, sport),
    ).fetchone()
    if not row:
        return None
    return row.get("provider_fixture_id")


def resolve_canonical_entity_id(
    conn: psycopg.Connection,
    provider: str,
    sport: str,
    entity_type: str,
    provider_entity_id: str,
) -> int | None:
    """Resolve provider entity ID to canonical player/team ID."""
    row = conn.execute(
        """
        SELECT canonical_entity_id
        FROM provider_entity_map
        WHERE provider = %s
          AND sport = %s
          AND entity_type = %s
          AND provider_entity_id = %s
        """,
        (provider, sport, entity_type, provider_entity_id),
    ).fetchone()
    if not row:
        return None
    return row.get("canonical_entity_id")
