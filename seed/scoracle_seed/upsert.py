"""Database upsert functions for teams, players, and stats.

All INSERT ON CONFLICT DO UPDATE queries. Ported from Go's seed/upsert.go.
Stats are inserted with raw provider keys — Postgres triggers normalize them.
"""

from __future__ import annotations

import json
import logging
from typing import Any

import psycopg

from .models import Player, PlayerStats, Team, TeamStats

logger = logging.getLogger(__name__)


def upsert_team(conn: psycopg.Connection, sport: str, team: Team) -> None:
    """Upsert a team into the teams table."""
    conn.execute(
        """
        INSERT INTO teams (
            id, sport, name, short_code, city, country, conference,
            division, venue_name, venue_capacity, founded, logo_url, meta
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (id, sport) DO UPDATE SET
            name = EXCLUDED.name,
            short_code = EXCLUDED.short_code,
            city = EXCLUDED.city,
            country = EXCLUDED.country,
            conference = EXCLUDED.conference,
            division = EXCLUDED.division,
            venue_name = EXCLUDED.venue_name,
            venue_capacity = EXCLUDED.venue_capacity,
            founded = EXCLUDED.founded,
            logo_url = EXCLUDED.logo_url,
            meta = EXCLUDED.meta,
            updated_at = NOW()
        """,
        (
            team.id,
            sport,
            team.name,
            team.short_code or None,
            team.city or None,
            team.country or None,
            team.conference or None,
            team.division or None,
            team.venue_name or None,
            team.venue_capacity,
            team.founded,
            team.logo_url or None,
            json.dumps(team.meta or {}),
        ),
    )


def upsert_player(conn: psycopg.Connection, sport: str, player: Player) -> None:
    """Upsert a player using COALESCE to preserve existing non-null values."""
    conn.execute(
        """
        INSERT INTO players (
            id, sport, name, first_name, last_name, position,
            detailed_position, nationality, height, weight,
            date_of_birth, photo_url, team_id, meta
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (id, sport) DO UPDATE SET
            name = COALESCE(EXCLUDED.name, players.name),
            first_name = COALESCE(EXCLUDED.first_name, players.first_name),
            last_name = COALESCE(EXCLUDED.last_name, players.last_name),
            position = COALESCE(EXCLUDED.position, players.position),
            detailed_position = COALESCE(EXCLUDED.detailed_position, players.detailed_position),
            nationality = COALESCE(EXCLUDED.nationality, players.nationality),
            height = COALESCE(EXCLUDED.height, players.height),
            weight = COALESCE(EXCLUDED.weight, players.weight),
            date_of_birth = COALESCE(EXCLUDED.date_of_birth, players.date_of_birth),
            photo_url = COALESCE(EXCLUDED.photo_url, players.photo_url),
            team_id = COALESCE(EXCLUDED.team_id, players.team_id),
            meta = COALESCE(EXCLUDED.meta, players.meta),
            updated_at = NOW()
        """,
        (
            player.id,
            sport,
            player.name,
            player.first_name or None,
            player.last_name or None,
            player.position or None,
            player.detailed_position or None,
            player.nationality or None,
            player.height or None,
            player.weight or None,
            player.date_of_birth or None,
            player.photo_url or None,
            player.team_id,
            json.dumps(player.meta or {}),
        ),
    )


def upsert_player_stats(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    league_id: int,
    data: PlayerStats,
) -> None:
    """Upsert player stats. Raw provider keys — Postgres trigger normalizes."""
    conn.execute(
        """
        INSERT INTO player_stats (
            player_id, sport, season, league_id, team_id,
            stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
        """,
        (
            data.player_id,
            sport,
            season,
            league_id,
            data.team_id,
            json.dumps(data.stats or {}),
            json.dumps(data.raw or {}),
        ),
    )


def upsert_team_stats(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    league_id: int,
    data: TeamStats,
) -> None:
    """Upsert team stats. Raw provider keys — Postgres trigger normalizes."""
    conn.execute(
        """
        INSERT INTO team_stats (
            team_id, sport, season, league_id,
            stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s)
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
        """,
        (
            data.team_id,
            sport,
            season,
            league_id,
            json.dumps(data.stats or {}),
            json.dumps(data.raw or {}),
        ),
    )


def finalize_fixture(conn: psycopg.Connection, fixture_id: int) -> tuple[int, int]:
    """Call Postgres finalize_fixture() — recalculates percentiles, refreshes
    views, marks fixture seeded. Returns (players_updated, teams_updated)."""
    row = conn.execute("SELECT * FROM finalize_fixture(%s)", (fixture_id,)).fetchone()
    if row:
        return row["players_updated"], row["teams_updated"]
    return 0, 0
