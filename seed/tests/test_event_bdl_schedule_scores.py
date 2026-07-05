"""BDL schedule-loading error semantics and zero-score preservation.

Two distinct correctness rules:

1. Schedule loading must distinguish "every request failed" (raise, so the
   operator sees the failure) from "a request succeeded with no rows" (return
   [], a legitimately empty window). Returning [] on failure would silently
   report "0 fixtures".
2. Score parsing must preserve a legitimate 0 (e.g. an NFL shutout). The old
   `visitor_team_score or away_team_score` fallback dropped a real 0 to the
   (usually absent) `away_team_score` key.
"""

import pytest

from services.event.handlers import bdl_nba, bdl_nfl
from services.event.handlers.bdl_nba import NBAHandler
from services.event.handlers.bdl_nfl import NFLHandler
from services.event.handlers.sportmonks_football import FootballHandler


class _FailingPagesClient:
    """get_all_pages always raises — simulates HTTP 500 / transport failure."""

    def __init__(self, exc):
        self._exc = exc
        self.calls = 0

    def get_all_pages(self, path, params=None):
        self.calls += 1
        raise self._exc


class _EmptyPagesClient:
    """get_all_pages succeeds but returns no rows — an empty window."""

    def __init__(self):
        self.calls = 0

    def get_all_pages(self, path, params=None):
        self.calls += 1
        return []


class _FixedPagesClient:
    """get_all_pages returns a fixed row list on the first (primary) call."""

    def __init__(self, rows):
        self._rows = rows

    def get_all_pages(self, path, params=None):
        return list(self._rows)


def _bare(handler_class):
    return handler_class.__new__(handler_class)


# --- schedule loading: failure vs. empty ----------------------------------


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_schedule_all_candidates_fail_raises(handler_class):
    handler = _bare(handler_class)
    handler.client = _FailingPagesClient(RuntimeError("HTTP 500"))
    with pytest.raises(RuntimeError, match="HTTP 500"):
        handler.get_games(2025, from_date="2026-01-01", to_date="2026-01-07")
    # Both the primary `seasons[]` candidate and the `season` fallback were
    # tried before surfacing the error.
    assert handler.client.calls == 2


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_schedule_success_empty_returns_empty(handler_class):
    handler = _bare(handler_class)
    handler.client = _EmptyPagesClient()
    games = handler.get_games(2025, from_date="2026-06-20", to_date="2026-06-30")
    assert games == []


# --- zero-score preservation ----------------------------------------------


def test_pick_score_preserves_zero():
    assert bdl_nba._pick_score(0, 99) == 0
    assert bdl_nfl._pick_score(0, 99) == 0


def test_pick_score_falls_back_only_when_absent():
    assert bdl_nba._pick_score(None, 17) == 17
    assert bdl_nfl._pick_score(None, 17) == 17
    assert bdl_nba._pick_score(None, None) is None


def test_nba_get_games_preserves_zero_away_score():
    handler = _bare(NBAHandler)
    handler.client = _FixedPagesClient(
        [
            {
                "id": 999,
                "home_team": {"id": 1, "full_name": "Hawks", "abbreviation": "ATL"},
                "visitor_team": {"id": 2, "full_name": "Celtics", "abbreviation": "BOS"},
                "date": "2026-01-02",
                "season": 2025,
                "status": "Final",
                "home_team_score": 100,
                "visitor_team_score": 0,
            }
        ]
    )
    games = handler.get_games(2025)
    assert len(games) == 1
    assert games[0]["home_score"] == 100
    assert games[0]["away_score"] == 0  # not None, not dropped by `or`


def test_nfl_get_games_preserves_zero_away_score():
    """An NFL shutout — the away team scored exactly 0."""
    handler = _bare(NFLHandler)
    handler.client = _FixedPagesClient(
        [
            {
                "id": 555,
                "home_team": {
                    "id": 10,
                    "full_name": "Team A",
                    "abbreviation": "A",
                    "location": "A City",
                },
                "visitor_team": {
                    "id": 11,
                    "full_name": "Team B",
                    "abbreviation": "B",
                    "location": "B City",
                },
                "date": "2026-01-02",
                "season": 2025,
                "week": 18,
                "home_team_score": 24,
                "visitor_team_score": 0,
            }
        ]
    )
    games = handler.get_games(2025)
    assert len(games) == 1
    assert games[0]["home_score"] == 24
    assert games[0]["away_score"] == 0


def test_handlers_have_no_season_stats_entrypoint():
    assert not hasattr(NBAHandler, "get_player_stats")
    assert not hasattr(NBAHandler, "get_team_stats")
    assert not hasattr(NFLHandler, "get_player_stats")
    assert not hasattr(NFLHandler, "get_team_stats")
    assert not hasattr(FootballHandler, "get_players_with_stats")
    assert not hasattr(FootballHandler, "get_team_stats")
