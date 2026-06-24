"""Fixture retry-cap behavior (FIRST-GPT-AUDIT Session 16).

A fixture that keeps failing — whether the provider errors (transport) or the
payload is permanently incomplete — must be bounded by a retry cap rather than
retried forever. The cap itself is enforced by the get_pending_fixtures() SQL
function (it filters out rows whose seed_attempts >= max_retries); the Python
contract this suite locks is:

  1. get_pending FORWARDS the cap (max_retries) to the SQL function, with the
     documented defaults — so the loop can never silently drop the cap and retry
     forever.
  2. BOTH failure recorders advance seed_attempts (so every failure, transport
     or incompleteness, counts toward the cap), while writing DIFFERENT columns
     so the two failure modes stay separable for diagnosis. A regression that
     stopped incrementing on one path would let that failure mode retry
     unbounded.
"""

from __future__ import annotations

from services.event import fixtures
from services.event.fixtures import FixtureRow, get_pending, record_failure, record_incomplete


class _Cursor:
    def __init__(self, rows=None):
        self._rows = rows or []

    def fetchall(self):
        return self._rows

    def fetchone(self):
        return self._rows[0] if self._rows else None


class _RecordingConn:
    """psycopg-like connection that records every (sql, params) it executes and
    serves a canned result set for the next fetchall()."""

    def __init__(self, rows=None):
        self._rows = rows or []
        self.calls: list[tuple[str, object]] = []

    def execute(self, sql, params=None):
        self.calls.append((sql, params))
        return _Cursor(self._rows)


def _pending_row(seed_attempts: int = 0) -> dict:
    return {
        "id": 1,
        "sport": "NBA",
        "league_id": None,
        "season": 2025,
        "home_team_id": 10,
        "away_team_id": 20,
        "start_time": "2026-06-20T23:00:00Z",
        "seed_delay_hours": 4,
        "seed_attempts": seed_attempts,
        "external_id": 999,
    }


# --------------------------------------------------------------------------
# 1. get_pending forwards the cap to the SQL function
# --------------------------------------------------------------------------


def test_get_pending_forwards_max_retries_and_defaults():
    conn = _RecordingConn(rows=[_pending_row(seed_attempts=2)])

    rows = get_pending(conn)

    assert len(conn.calls) == 1
    sql, params = conn.calls[0]
    assert "get_pending_fixtures" in sql
    # (sport, limit, max_retries) — default sport None, default limit 10000
    # (the "unlimited" sentinel), default cap 3.
    assert params == (None, 10000, 3)
    # The cap-relevant field survives the mapping so the loop can reason about it.
    assert rows[0].seed_attempts == 2


def test_get_pending_propagates_explicit_cap_and_limit():
    conn = _RecordingConn(rows=[])

    get_pending(conn, sport="NFL", limit=50, max_retries=5)

    _, params = conn.calls[0]
    assert params == ("NFL", 50, 5)


# --------------------------------------------------------------------------
# 2. Both recorders advance the cap; the two failure modes stay separable
# --------------------------------------------------------------------------


def test_record_failure_advances_cap_and_writes_only_transport_error():
    conn = _RecordingConn()

    record_failure(conn, fixture_id=42, error_msg="provider 503")

    sql, params = conn.calls[0]
    assert "seed_attempts = seed_attempts + 1" in sql  # counts toward the cap
    assert "last_seed_error" in sql
    assert "last_incomplete_reason" not in sql  # transport mode only
    assert params == ("provider 503", 42)


def test_record_incomplete_advances_cap_and_writes_only_incomplete_reason():
    conn = _RecordingConn()

    record_incomplete(conn, fixture_id=42, reason="missing box scores")

    sql, params = conn.calls[0]
    assert "seed_attempts = seed_attempts + 1" in sql  # bounded, not retried forever
    assert "last_incomplete_reason" in sql
    assert "last_seed_error" not in sql  # incompleteness mode only
    assert params == ("missing box scores", 42)


def test_both_failure_modes_increment_the_same_counter():
    """Locks the invariant that drives the cap: a fixture failing for EITHER
    reason advances seed_attempts, so a permanently-broken fixture is eventually
    filtered out by get_pending_fixtures regardless of which way it fails."""
    transport = _RecordingConn()
    incomplete = _RecordingConn()

    record_failure(transport, 7, "boom")
    record_incomplete(incomplete, 7, "thin payload")

    assert "seed_attempts = seed_attempts + 1" in transport.calls[0][0]
    assert "seed_attempts = seed_attempts + 1" in incomplete.calls[0][0]


def test_fixtures_module_exposes_the_retry_cap_surface():
    """Guard against an accidental rename that would silently break the loop's
    cap handling (these names are referenced by services.event.cli)."""
    for name in ("get_pending", "record_failure", "record_incomplete"):
        assert hasattr(fixtures, name)
    assert FixtureRow.__dataclass_fields__["seed_attempts"]
