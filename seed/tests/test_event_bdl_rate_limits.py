"""Rate-limit propagation for live BDL fixture ingestion.

A provider 429 must surface as RateLimitExhausted all the way to the seeder
loop (which pauses the run without burning fixture retries) — it must never be
swallowed into an empty result that looks like "the provider had no data".
"""

import pytest

from services.event.handlers.bdl_nba import NBAHandler
from services.event.handlers.bdl_nfl import NFLHandler
from shared.api_errors import RateLimitExhausted


class _RateLimitedClient:
    """Every access path raises RateLimitExhausted, like a hard 429 quota hit."""

    def get(self, path, params=None):
        raise RateLimitExhausted("balldontlie", f"path={path}")

    def get_all_pages(self, path, params=None):
        raise RateLimitExhausted("balldontlie", f"path={path}")

    def get_paginated(self, path, params=None):
        raise RateLimitExhausted("balldontlie", f"path={path}")


def _rate_limited(handler_class):
    handler = handler_class.__new__(handler_class)
    handler.client = _RateLimitedClient()
    return handler


# --- fixture-processing path (schedule + box scores) ----------------------


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_schedule_rate_limit_propagates(handler_class):
    handler = _rate_limited(handler_class)
    with pytest.raises(RateLimitExhausted):
        handler.get_games(2025, from_date="2026-06-20", to_date="2026-06-30")


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_box_score_rate_limit_propagates(handler_class):
    handler = _rate_limited(handler_class)
    with pytest.raises(RateLimitExhausted):
        handler._fetch_box_score_lines(123)


def test_nfl_team_stats_rate_limit_propagates():
    handler = _rate_limited(NFLHandler)
    with pytest.raises(RateLimitExhausted):
        handler._fetch_team_stats(123)


# --- meta / roster path (teams, players) ----------------------------------
# These caught broad Exception and returned [] / None, masking a 429 as
# "no data". A rate limit must still stop the run.


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_get_teams_rate_limit_propagates(handler_class):
    handler = _rate_limited(handler_class)
    with pytest.raises(RateLimitExhausted):
        handler.get_teams()


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_get_player_rate_limit_propagates(handler_class):
    handler = _rate_limited(handler_class)
    with pytest.raises(RateLimitExhausted):
        handler.get_player(42)


def test_nba_get_all_players_rate_limit_propagates():
    handler = _rate_limited(NBAHandler)
    with pytest.raises(RateLimitExhausted):
        handler.get_all_players()


def test_nfl_get_all_players_rate_limit_propagates():
    handler = _rate_limited(NFLHandler)
    with pytest.raises(RateLimitExhausted):
        handler.get_all_players(2025)


def test_nba_get_first_success_reraises_rate_limit_without_second_request():
    """A 429 on the first path must not fall through to a second request."""
    handler = NBAHandler.__new__(NBAHandler)

    class _CountingClient:
        def __init__(self):
            self.calls = 0

        def get(self, path, params=None):
            self.calls += 1
            raise RateLimitExhausted("balldontlie", f"path={path}")

    handler.client = _CountingClient()
    with pytest.raises(RateLimitExhausted):
        handler._get_first_success(["/v1/teams", "/nba/v1/teams"])
    assert handler.client.calls == 1
