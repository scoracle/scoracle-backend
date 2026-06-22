"""Fixture finality & completeness contract (FIRST-GPT-AUDIT Session 4)."""

from __future__ import annotations

from datetime import datetime, timezone

import pytest

from services.event.cli import _seed_fixture_box_scores
from services.event.completeness import (
    CompletenessVerdict,
    IncompleteFixtureError,
    evaluate_completeness,
    is_final,
)
from services.event.fixtures import FixtureRow
from shared.models import BoxScoreResult, EventBoxScore, EventTeamStats

HOME, AWAY = 10, 20


# --------------------------------------------------------------------------
# is_final — tri-state provider finality
# --------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("sport", "status", "expected"),
    [
        ("NBA", "Final", True),
        ("NBA", "Final/OT", True),
        ("NBA", "2026-06-21T23:00:00Z", None),   # scheduled → defer
        ("NBA", "3rd Qtr", None),                 # in-progress → defer
        ("NBA", None, None),
        ("NFL", "Final", True),
        ("NFL", "1st Qtr", None),
        ("FOOTBALL", "FT", True),
        ("FOOTBALL", "AET", True),
        ("FOOTBALL", "NS", False),                # not started → reject
        ("FOOTBALL", "POSTPONED", False),
        ("FOOTBALL", "ABANDONED", False),
        ("FOOTBALL", "SomeNewState", None),       # unrecognised → defer
        ("FOOTBALL", None, None),
    ],
)
def test_is_final(sport, status, expected):
    assert is_final(sport, status) is expected


# --------------------------------------------------------------------------
# evaluate_completeness — the acceptance predicate
# --------------------------------------------------------------------------


def _result(
    *,
    sport_status: str | None = "Final",
    teams: tuple[tuple[int, int | None], ...] = ((HOME, 101), (AWAY, 99)),
    n_players: int = 12,
) -> BoxScoreResult:
    players = [
        EventBoxScore(fixture_id=1, player_id=1000 + i, team_id=HOME)
        for i in range(n_players)
    ]
    team_rows = [
        EventTeamStats(fixture_id=1, team_id=tid, score=score) for tid, score in teams
    ]
    return BoxScoreResult(players=players, teams=team_rows, provider_status=sport_status)


def test_complete_payload_accepted():
    v = evaluate_completeness("NBA", HOME, AWAY, _result())
    assert v == CompletenessVerdict(True)


def test_recognised_not_final_rejected():
    v = evaluate_completeness(
        "FOOTBALL", HOME, AWAY, _result(sport_status="POSTPONED")
    )
    assert not v.accepted and "not final" in v.reason


def test_missing_team_rejected():
    v = evaluate_completeness("NBA", HOME, AWAY, _result(teams=((HOME, 101),)))
    assert not v.accepted and "away team" in v.reason


def test_missing_score_rejected():
    v = evaluate_completeness(
        "NBA", HOME, AWAY, _result(teams=((HOME, 101), (AWAY, None)))
    )
    assert not v.accepted and "no final score" in v.reason


def test_zero_score_is_present():
    # A legitimate 0-0 football draw is complete (0 is not "missing").
    v = evaluate_completeness(
        "FOOTBALL", HOME, AWAY, _result(sport_status="FT", teams=((HOME, 0), (AWAY, 0)))
    )
    assert v.accepted


def test_too_few_players_rejected():
    v = evaluate_completeness("NBA", HOME, AWAY, _result(n_players=3))
    assert not v.accepted and "player rows" in v.reason


def test_unknown_status_defers_to_structure():
    # No finality label, but structurally complete → accepted.
    v = evaluate_completeness("NBA", HOME, AWAY, _result(sport_status=None))
    assert v.accepted


# --------------------------------------------------------------------------
# _seed_fixture_box_scores gate — atomicity, shrink, force
# --------------------------------------------------------------------------


class _FakeCursor:
    def __init__(self, row):
        self._row = row

    def fetchone(self):
        return self._row


class _FakeConn:
    """Minimal psycopg-like connection that records executed SQL.

    Returns canned rows for the two SELECTs the gate path issues (provider
    fixture map lookup → None so it falls back to external_id; the existing
    player-row COUNT) and for finalize_fixture; everything else is a no-op.
    """

    def __init__(self, existing_players: int = 0):
        self.existing_players = existing_players
        self.executed: list[str] = []

    def execute(self, sql, params=None):
        self.executed.append(sql)
        if "FROM provider_fixture_map" in sql:
            return _FakeCursor(None)
        if "COUNT(*) AS n FROM event_box_scores" in sql:
            return _FakeCursor({"n": self.existing_players})
        if "finalize_fixture" in sql:
            return _FakeCursor({"players_updated": 1, "teams_updated": 1})
        return _FakeCursor(None)

    def deleted_event_rows(self) -> bool:
        return any("DELETE FROM event_" in sql for sql in self.executed)


class _FakeHandler:
    def __init__(self, result: BoxScoreResult):
        self._result = result

    def get_box_score(self, external_id, fixture_id):
        return self._result


def _fixture() -> FixtureRow:
    return FixtureRow(
        id=1,
        sport="NBA",
        league_id=0,
        season=2025,
        home_team_id=HOME,
        away_team_id=AWAY,
        start_time=datetime(2025, 1, 1, tzinfo=timezone.utc),
        seed_delay_hours=4,
        seed_attempts=0,
        external_id=999,
    )


def test_gate_rejects_empty_payload_even_with_force():
    conn = _FakeConn()
    handler = _FakeHandler(BoxScoreResult())
    with pytest.raises(IncompleteFixtureError, match="no event rows"):
        _seed_fixture_box_scores(conn, _fixture(), handler, recompute=False, force=True)
    assert not conn.deleted_event_rows()


def test_gate_rejects_incomplete_before_mutation():
    # Team-only payload (no player rows, one team) → rejected, and no DELETE
    # was issued, so existing rows would survive.
    conn = _FakeConn(existing_players=12)
    result = BoxScoreResult(
        players=[],
        teams=[EventTeamStats(fixture_id=1, team_id=HOME, score=101)],
        provider_status="Final",
    )
    handler = _FakeHandler(result)
    with pytest.raises(IncompleteFixtureError):
        _seed_fixture_box_scores(conn, _fixture(), handler, recompute=False)
    assert not conn.deleted_event_rows()
    assert not any("finalize_fixture" in s for s in conn.executed)


def test_gate_accepts_complete_payload():
    conn = _FakeConn(existing_players=0)
    result = _result()
    handler = _FakeHandler(result)
    box, team, p_up, t_up = _seed_fixture_box_scores(
        conn, _fixture(), handler, recompute=False
    )
    assert box == 12 and team == 2
    assert conn.deleted_event_rows()
    assert any("finalize_fixture" in s for s in conn.executed)


def test_gate_rejects_shrinking_reseed():
    conn = _FakeConn(existing_players=20)  # already had 20 player rows
    handler = _FakeHandler(_result(n_players=12))  # provider now returns fewer
    with pytest.raises(IncompleteFixtureError, match="shrinks player rows"):
        _seed_fixture_box_scores(conn, _fixture(), handler, recompute=False)
    assert not conn.deleted_event_rows()


def test_force_bypasses_completeness_and_shrink():
    # Not-final status + shrinking re-seed, but --force writes it anyway.
    conn = _FakeConn(existing_players=20)
    handler = _FakeHandler(_result(sport_status="3rd Qtr", n_players=12))
    box, team, _, _ = _seed_fixture_box_scores(
        conn, _fixture(), handler, recompute=False, force=True
    )
    assert box == 12
    assert conn.deleted_event_rows()
    assert any("finalize_fixture" in s for s in conn.executed)
