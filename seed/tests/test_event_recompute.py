"""Durable deferred-recompute queue (FIRST-GPT-AUDIT Session 6).

A deferred backfill finalizes fixtures with recompute=False and owes each
(sport, season) one whole-season recompute + rating_history snapshot at
end-of-run. These tests cover the durable season_recompute_needed queue that
replaced the old in-memory set: the dirty marker is written in the finalize
transaction, drained only on full success, and kept (with a recorded error) on
failure.
"""

from __future__ import annotations

import contextlib
from datetime import datetime, timezone

import pytest

from services.event.cli import _drain_dirty_seasons, _seed_fixture_box_scores
from services.event.completeness import IncompleteFixtureError  # noqa: F401 (parity)
from services.event.fixtures import FixtureRow
from shared.models import BoxScoreResult, EventBoxScore, EventTeamStats

HOME, AWAY = 10, 20


# --------------------------------------------------------------------------
# Fakes
# --------------------------------------------------------------------------


class _Cursor:
    def __init__(self, *, row=None, rows=None):
        self._row = row
        self._rows = rows or []

    def fetchone(self):
        return self._row

    def fetchall(self):
        return self._rows


class _SeedConn:
    """psycopg-like connection for the _seed_fixture_box_scores path. Serves the
    provider-map lookup (None → fall back to external_id), the existing-rows
    COUNT, and finalize_fixture; records every statement so a test can assert the
    season_recompute_needed upsert fired (or didn't)."""

    def __init__(self, existing_players: int = 0):
        self.existing_players = existing_players
        self.executed: list[str] = []

    def execute(self, sql, params=None):
        self.executed.append(sql)
        if "FROM provider_fixture_map" in sql:
            return _Cursor(row=None)
        if "COUNT(*) AS n FROM event_box_scores" in sql:
            return _Cursor(row={"n": self.existing_players})
        if "finalize_fixture" in sql:
            return _Cursor(row={"players_updated": 1, "teams_updated": 1})
        return _Cursor(row=None)

    def marked_dirty(self) -> bool:
        return any("INSERT INTO season_recompute_needed" in s for s in self.executed)


class _DrainConn:
    """psycopg-like connection for the drain path. Serves the dirty-season
    SELECT and the recompute/snapshot function calls, records every statement,
    and counts commits/rollbacks. `fail_on` is a set of (sport, season) whose
    recompute_season raises."""

    def __init__(self, dirty, fail_on=None):
        self._dirty = list(dirty)
        self._fail_on = set(fail_on or ())
        self.executed: list[tuple[str, tuple | None]] = []
        self.committed = 0
        self.rolled_back = 0

    @contextlib.contextmanager
    def transaction(self):
        try:
            yield self
        except Exception:
            self.rolled_back += 1
            raise
        else:
            self.committed += 1

    def execute(self, sql, params=None):
        self.executed.append((sql, params))
        if "SELECT" in sql.upper() and "FROM season_recompute_needed" in sql:
            return _Cursor(rows=[{"sport": s, "season": y} for s, y in self._dirty])
        if "recompute_season(" in sql:
            if params and (params[0], params[1]) in self._fail_on:
                raise RuntimeError("recompute boom")
            return _Cursor(row={"players_updated": 5, "teams_updated": 2})
        if "snapshot_rating_history(" in sql:
            return _Cursor(row={"n": 3})
        return _Cursor(row=None)

    def sqls(self) -> list[str]:
        return [s for s, _ in self.executed]

    def deleted(self, sport: str, season: int) -> bool:
        return any(
            "DELETE FROM season_recompute_needed" in s and p == (sport, season)
            for s, p in self.executed
        )

    def recorded_failure(self, sport: str, season: int) -> bool:
        return any(
            "UPDATE season_recompute_needed" in s and p and p[-2:] == (sport, season)
            for s, p in self.executed
        )


class _Handler:
    def __init__(self, result: BoxScoreResult):
        self._result = result

    def get_box_score(self, external_id, fixture_id):
        return self._result


def _complete_result(n_players: int = 12) -> BoxScoreResult:
    players = [
        EventBoxScore(fixture_id=1, player_id=1000 + i, team_id=HOME)
        for i in range(n_players)
    ]
    teams = [
        EventTeamStats(fixture_id=1, team_id=HOME, score=101),
        EventTeamStats(fixture_id=1, team_id=AWAY, score=99),
    ]
    return BoxScoreResult(players=players, teams=teams, provider_status="Final")


def _fixture(sport: str = "NBA", season: int = 2018) -> FixtureRow:
    return FixtureRow(
        id=1,
        sport=sport,
        league_id=0,
        season=season,
        home_team_id=HOME,
        away_team_id=AWAY,
        start_time=datetime(2018, 1, 1, tzinfo=timezone.utc),
        seed_delay_hours=4,
        seed_attempts=0,
        external_id=999,
    )


# --------------------------------------------------------------------------
# Marking: deferred finalize records the dirty season in-transaction
# --------------------------------------------------------------------------


def test_deferred_finalize_marks_dirty_season():
    conn = _SeedConn()
    _seed_fixture_box_scores(
        conn, _fixture(), _Handler(_complete_result()), recompute=False
    )
    assert conn.marked_dirty()


def test_live_finalize_does_not_mark_dirty_season():
    conn = _SeedConn()
    _seed_fixture_box_scores(
        conn, _fixture(), _Handler(_complete_result()), recompute=True
    )
    assert not conn.marked_dirty()


# --------------------------------------------------------------------------
# Draining: clear only on full success; keep + record on failure
# --------------------------------------------------------------------------


def test_drain_empty_is_noop():
    conn = _DrainConn(dirty=[])
    pairs, p_up, t_up = _drain_dirty_seasons(conn)
    assert (pairs, p_up, t_up) == (0, 0, 0)
    # Only the dirty-season SELECT ran; no transaction was opened.
    assert conn.committed == 0 and conn.rolled_back == 0
    assert not any("recompute_season(" in s for s in conn.sqls())


def test_drain_success_clears_each_marker():
    conn = _DrainConn(dirty=[("NBA", 2018), ("NFL", 2020)])
    pairs, p_up, t_up = _drain_dirty_seasons(conn)
    assert pairs == 2
    assert (p_up, t_up) == (10, 4)  # 5+5 players, 2+2 teams
    assert conn.deleted("NBA", 2018) and conn.deleted("NFL", 2020)
    assert conn.committed == 2 and conn.rolled_back == 0
    assert not conn.recorded_failure("NBA", 2018)


def test_drain_failure_keeps_marker_and_continues():
    conn = _DrainConn(dirty=[("NBA", 2018), ("NFL", 2020)], fail_on={("NBA", 2018)})
    pairs, p_up, t_up = _drain_dirty_seasons(conn)
    assert pairs == 2
    # Only the NFL season's totals land; NBA rolled back.
    assert (p_up, t_up) == (5, 2)
    # NBA: recompute txn rolled back, marker kept, failure recorded — not deleted.
    assert conn.rolled_back == 1
    assert not conn.deleted("NBA", 2018)
    assert conn.recorded_failure("NBA", 2018)
    # NFL still drained despite NBA's failure (drain does not abort).
    assert conn.deleted("NFL", 2020)
