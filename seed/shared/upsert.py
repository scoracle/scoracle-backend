"""Database upsert functions for teams, players, and stats.

All INSERT ON CONFLICT DO UPDATE queries. Ported from Go's seed/upsert.go.
Stats are inserted with raw provider keys — Postgres triggers normalize them.
"""

from __future__ import annotations

import json
import logging
from typing import Any

import psycopg

from .aliases import generate_player_aliases, generate_team_aliases
from .models import EventBoxScore, EventTeamStats, Player, PlayerStats, Team, TeamStats

logger = logging.getLogger(__name__)


def upsert_team(conn: psycopg.Connection, sport: str, team: Team) -> None:
    """Upsert a team into the teams table."""
    # Generate search aliases if not already set.
    aliases = team.search_aliases or generate_team_aliases(
        team.name,
        sport,
        team.short_code,
        team.meta,
    )

    conn.execute(
        """
        INSERT INTO teams (
            id, sport, name, short_code, city, country, conference,
            division, venue_name, venue_capacity, founded, logo_url,
            league_id, search_aliases, meta
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (id, sport) DO UPDATE SET
            name = EXCLUDED.name,
            short_code = COALESCE(EXCLUDED.short_code, teams.short_code),
            city = COALESCE(EXCLUDED.city, teams.city),
            country = COALESCE(EXCLUDED.country, teams.country),
            conference = COALESCE(EXCLUDED.conference, teams.conference),
            division = COALESCE(EXCLUDED.division, teams.division),
            venue_name = COALESCE(EXCLUDED.venue_name, teams.venue_name),
            venue_capacity = COALESCE(EXCLUDED.venue_capacity, teams.venue_capacity),
            founded = COALESCE(EXCLUDED.founded, teams.founded),
            logo_url = COALESCE(EXCLUDED.logo_url, teams.logo_url),
            league_id = COALESCE(EXCLUDED.league_id, teams.league_id),
            search_aliases = EXCLUDED.search_aliases,
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
            team.league_id,
            aliases,
            json.dumps(team.meta or {}),
        ),
    )


def upsert_player(conn: psycopg.Connection, sport: str, player: Player) -> None:
    """Upsert a player meta record. Position is NOT persisted here — it lives
    on player_stats / event_box_scores so the stats domain and meta domain
    stay independent. Only the player ID links the two tables.
    """
    # Generate search aliases if not already set.
    aliases = player.search_aliases or generate_player_aliases(
        player.name,
        sport,
        player.first_name,
        player.last_name,
        player.meta,
    )

    conn.execute(
        """
        INSERT INTO players (
            id, sport, name, first_name, last_name,
            nationality, height, weight,
            date_of_birth, photo_url, team_id, search_aliases, meta,
            raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (id, sport) DO UPDATE SET
            name = COALESCE(EXCLUDED.name, players.name),
            first_name = COALESCE(EXCLUDED.first_name, players.first_name),
            last_name = COALESCE(EXCLUDED.last_name, players.last_name),
            nationality = COALESCE(EXCLUDED.nationality, players.nationality),
            height = COALESCE(EXCLUDED.height, players.height),
            weight = COALESCE(EXCLUDED.weight, players.weight),
            date_of_birth = COALESCE(EXCLUDED.date_of_birth, players.date_of_birth),
            photo_url = COALESCE(EXCLUDED.photo_url, players.photo_url),
            -- Current team is owned by roster/current-identity sync, not by
            -- arbitrary historical meta payloads.
            team_id = players.team_id,
            search_aliases = COALESCE(EXCLUDED.search_aliases, players.search_aliases),
            meta = COALESCE(EXCLUDED.meta, players.meta),
            raw_response = COALESCE(EXCLUDED.raw_response, players.raw_response),
            updated_at = NOW()
        """,
        (
            player.id,
            sport,
            player.name,
            player.first_name or None,
            player.last_name or None,
            player.nationality or None,
            player.height or None,
            player.weight or None,
            player.date_of_birth or None,
            player.photo_url or None,
            player.team_id,
            aliases or None,
            json.dumps(player.meta or {}),
            json.dumps(player.raw) if player.raw else None,
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
            position, stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            position = COALESCE(EXCLUDED.position, player_stats.position),
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
            data.position or None,
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


def upsert_event_box_score(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    league_id: int,
    data: EventBoxScore,
) -> None:
    """Upsert one player fixture-level box score line."""
    conn.execute(
        """
        INSERT INTO event_box_scores (
            fixture_id, player_id, team_id, sport, season, league_id,
            position, minutes_played, stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (fixture_id, player_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            position = COALESCE(EXCLUDED.position, event_box_scores.position),
            minutes_played = EXCLUDED.minutes_played,
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
        """,
        (
            data.fixture_id,
            data.player_id,
            data.team_id,
            sport,
            season,
            league_id,
            data.position or None,
            data.minutes_played,
            json.dumps(data.stats or {}),
            json.dumps(data.raw or {}),
        ),
    )


def upsert_event_team_stats(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    league_id: int,
    data: EventTeamStats,
) -> None:
    """Upsert one team fixture-level stat line."""
    conn.execute(
        """
        INSERT INTO event_team_stats (
            fixture_id, team_id, sport, season, league_id,
            score, stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (fixture_id, team_id) DO UPDATE SET
            score = EXCLUDED.score,
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
        """,
        (
            data.fixture_id,
            data.team_id,
            sport,
            season,
            league_id,
            data.score,
            json.dumps(data.stats or {}),
            json.dumps(data.raw or {}),
        ),
    )


def upsert_provider_entity_map(
    conn: psycopg.Connection,
    provider: str,
    sport: str,
    entity_type: str,
    provider_entity_id: str,
    canonical_entity_id: int,
    meta: dict[str, Any] | None = None,
) -> None:
    """Upsert provider->canonical entity mapping."""
    conn.execute(
        """
        INSERT INTO provider_entity_map (
            provider, sport, entity_type, provider_entity_id, canonical_entity_id, meta
        ) VALUES (%s,%s,%s,%s,%s,%s)
        ON CONFLICT (provider, sport, entity_type, provider_entity_id) DO UPDATE SET
            canonical_entity_id = EXCLUDED.canonical_entity_id,
            meta = EXCLUDED.meta,
            updated_at = NOW()
        """,
        (
            provider,
            sport,
            entity_type,
            provider_entity_id,
            canonical_entity_id,
            json.dumps(meta or {}),
        ),
    )


def upsert_provider_fixture_map(
    conn: psycopg.Connection,
    provider: str,
    sport: str,
    provider_fixture_id: str,
    fixture_id: int,
    meta: dict[str, Any] | None = None,
) -> None:
    """Upsert provider->fixture mapping."""
    conn.execute(
        """
        INSERT INTO provider_fixture_map (
            provider, sport, provider_fixture_id, fixture_id, meta
        ) VALUES (%s,%s,%s,%s,%s)
        ON CONFLICT (provider, sport, provider_fixture_id) DO UPDATE SET
            fixture_id = EXCLUDED.fixture_id,
            meta = EXCLUDED.meta,
            updated_at = NOW()
        """,
        (
            provider,
            sport,
            provider_fixture_id,
            fixture_id,
            json.dumps(meta or {}),
        ),
    )


def upsert_team_roster(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    team_id: int,
    player_id: int,
    *,
    jersey_number: str | None = None,
    position: str | None = None,
    source: str | None = None,
) -> None:
    """Upsert one season-scoped team_rosters membership row.

    position_group is resolved in SQL via public.position_group(sport, position)
    so it stays aligned with the database's canonical mapping.
    """
    conn.execute(
        """
        INSERT INTO team_rosters (
            sport, season, team_id, player_id,
            jersey_number, position, position_group,
            is_active, source, first_seen, last_seen
        ) VALUES (
            %s, %s, %s, %s,
            %s, %s, position_group(%s, %s),
            TRUE, %s, NOW(), NOW()
        )
        ON CONFLICT (sport, season, team_id, player_id) DO UPDATE SET
            jersey_number = COALESCE(EXCLUDED.jersey_number, team_rosters.jersey_number),
            position = COALESCE(EXCLUDED.position, team_rosters.position),
            position_group = COALESCE(EXCLUDED.position_group, team_rosters.position_group),
            is_active = TRUE,
            source = COALESCE(EXCLUDED.source, team_rosters.source),
            last_seen = NOW()
        """,
        (
            sport,
            season,
            team_id,
            player_id,
            jersey_number,
            position,
            sport,
            position,
            source,
        ),
    )


def deactivate_missing_team_rosters(
    conn: psycopg.Connection,
    sport: str,
    season: int,
    team_id: int,
    active_player_ids: list[int],
) -> int:
    """Mark team_rosters rows inactive when they are absent from the latest pull.

    Returns the number of rows marked inactive.
    """
    if active_player_ids:
        cur = conn.execute(
            """
            UPDATE team_rosters
            SET is_active = FALSE, last_seen = NOW()
            WHERE sport = %s
              AND season = %s
              AND team_id = %s
              AND is_active
              AND NOT (player_id = ANY(%s))
            """,
            (sport, season, team_id, active_player_ids),
        )
    else:
        cur = conn.execute(
            """
            UPDATE team_rosters
            SET is_active = FALSE, last_seen = NOW()
            WHERE sport = %s
              AND season = %s
              AND team_id = %s
              AND is_active
            """,
            (sport, season, team_id),
        )
    return cur.rowcount


def sync_player_team_membership(
    conn: psycopg.Connection,
    sport: str,
    player_id: int,
    team_id: int | None,
    league_id: int | None,
) -> None:
    """Refresh players.team_id / players.league_id from current roster membership."""
    conn.execute(
        """
        UPDATE players
        SET team_id = %s,
            league_id = %s,
            updated_at = NOW()
        WHERE id = %s
          AND sport = %s
        """,
        (team_id, league_id, player_id, sport),
    )


def finalize_fixture(
    conn: psycopg.Connection, fixture_id: int, recompute: bool = True
) -> tuple[int, int]:
    """Call Postgres finalize_fixture() — per-fixture aggregation + (when
    recompute=True, the default) the whole-season percentile/rating recompute +
    refresh views, then mark fixture seeded. Returns (players_updated,
    teams_updated).

    Pass recompute=False for bulk historical backfill to skip the expensive
    per-fixture whole-season recompute, then call recompute_season() once at the
    end (O(M) instead of O(M^2)). mark_fixture_seeded still runs, so resume state
    stays correct."""
    row = conn.execute(
        "SELECT * FROM finalize_fixture(%s, %s)", (fixture_id, recompute)
    ).fetchone()
    if row:
        return row["players_updated"], row["teams_updated"]
    return 0, 0


def recompute_season(
    conn: psycopg.Connection, sport: str, season: int
) -> tuple[int, int]:
    """One-pass whole-season percentile + rating-engine recompute for
    (sport, season). Use once after a deferred (recompute=False) backfill run.
    Idempotent. Returns (players_updated, teams_updated)."""
    row = conn.execute(
        "SELECT * FROM recompute_season(%s, %s)", (sport, season)
    ).fetchone()
    if row:
        return row["players_updated"], row["teams_updated"]
    return 0, 0


def snapshot_rating_history(
    conn: psycopg.Connection, sport: str, season: int, trigger: str = "recompute"
) -> int:
    """Append per-entity rating snapshots for (sport, season) into rating_history
    (debounced insert-if-changed). Returns the number of rows inserted."""
    row = conn.execute(
        "SELECT snapshot_rating_history(%s, %s, %s) AS n", (sport, season, trigger)
    ).fetchone()
    return row["n"] if row and row["n"] is not None else 0


# ---------------------------------------------------------------------------
# Durable deferred-recompute queue (FIRST-GPT-AUDIT Session 6)
#
# A deferred backfill finalizes fixtures with recompute=False and owes the
# season ONE recompute_season + rating_history snapshot at end-of-run. These
# helpers make that "still owed" state durable in season_recompute_needed
# (migration 101) instead of an in-memory Python set, so a process death cannot
# strand a seeded-but-unrecomputed season invisibly.
# ---------------------------------------------------------------------------


def mark_season_recompute_needed(
    conn: psycopg.Connection, sport: str, season: int
) -> None:
    """Record that (sport, season) has a fixture finalized in deferred mode and
    still owes a whole-season recompute + rating_history snapshot. Idempotent.

    Call inside the SAME transaction that finalizes the deferred fixture, so a
    crash before the end-of-run drain cannot lose the dirty marker."""
    conn.execute(
        """
        INSERT INTO season_recompute_needed (sport, season)
        VALUES (%s, %s)
        ON CONFLICT (sport, season) DO NOTHING
        """,
        (sport, season),
    )


def load_dirty_seasons(conn: psycopg.Connection) -> list[tuple[str, int]]:
    """Return every (sport, season) awaiting a deferred recompute, oldest
    request first."""
    rows = conn.execute(
        "SELECT sport, season FROM season_recompute_needed "
        "ORDER BY requested_at, sport, season"
    ).fetchall()
    return [(r["sport"], r["season"]) for r in rows]


def clear_season_recompute_needed(
    conn: psycopg.Connection, sport: str, season: int
) -> None:
    """Delete the dirty marker for (sport, season). Call ONLY after a successful
    recompute_season + snapshot_rating_history (ideally in the same transaction,
    so the delete commits atomically with the recompute)."""
    conn.execute(
        "DELETE FROM season_recompute_needed WHERE sport = %s AND season = %s",
        (sport, season),
    )


def record_recompute_failure(
    conn: psycopg.Connection, sport: str, season: int, error: str
) -> None:
    """Bump the attempt count and record the latest error for a dirty season
    whose recompute failed, leaving the marker in place for a later retry."""
    conn.execute(
        """
        UPDATE season_recompute_needed
           SET attempts = attempts + 1, last_error = %s
         WHERE sport = %s AND season = %s
        """,
        (error[:1000], sport, season),
    )
