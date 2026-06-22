"""Rate-limit propagation for live BDL fixture ingestion."""

import pytest

from services.event.handlers.bdl_nba import NBAHandler
from services.event.handlers.bdl_nfl import NFLHandler
from shared.api_errors import RateLimitExhausted


class _RateLimitedClient:
    def get_all_pages(self, path, params):
        raise RateLimitExhausted("balldontlie", f"path={path}")


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_schedule_rate_limit_propagates(handler_class):
    handler = handler_class.__new__(handler_class)
    handler.client = _RateLimitedClient()

    with pytest.raises(RateLimitExhausted):
        handler.get_games(2025, from_date="2026-06-20", to_date="2026-06-30")


@pytest.mark.parametrize("handler_class", [NBAHandler, NFLHandler])
def test_box_score_rate_limit_propagates(handler_class):
    handler = handler_class.__new__(handler_class)
    handler.client = _RateLimitedClient()

    with pytest.raises(RateLimitExhausted):
        handler._fetch_box_score_lines(123)


def test_nfl_team_stats_rate_limit_propagates():
    handler = NFLHandler.__new__(NFLHandler)
    handler.client = _RateLimitedClient()

    with pytest.raises(RateLimitExhausted):
        handler._fetch_team_stats(123)
