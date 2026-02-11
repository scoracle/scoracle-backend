"""
BallDontLie NFL API client.

Provides access to NFL teams, players, and season stats
via the BallDontLie API (https://api.balldontlie.io/nfl).
"""

import logging
from typing import Any, AsyncIterator

from ..core.http import BaseApiClient

logger = logging.getLogger(__name__)


class BallDontLieNFL(BaseApiClient):
    """BallDontLie NFL API client."""

    BASE_URL = "https://api.balldontlie.io/nfl/v1"

    def __init__(self, api_key: str, requests_per_minute: int = 600):
        super().__init__(
            headers={"Authorization": api_key},
            requests_per_minute=requests_per_minute,
        )

    # =========================================================================
    # Teams
    # =========================================================================

    async def get_teams(self) -> list[dict[str, Any]]:
        """Get all NFL teams."""
        response = await self._get("/teams")
        return response["data"]

    async def get_team(self, team_id: int) -> dict[str, Any]:
        """Get a specific team."""
        response = await self._get(f"/teams/{team_id}")
        return response["data"]

    # =========================================================================
    # Players
    # =========================================================================

    async def get_players(self, per_page: int = 100) -> AsyncIterator[dict[str, Any]]:
        """Iterate through all players (cursor-based pagination)."""
        params: dict[str, Any] = {"per_page": per_page}

        while True:
            response = await self._get("/players", params)
            for player in response["data"]:
                yield player

            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    async def get_player(self, player_id: int) -> dict[str, Any]:
        """Get a specific player."""
        response = await self._get(f"/players/{player_id}")
        return response["data"]

    # =========================================================================
    # Season Stats
    # =========================================================================

    async def get_season_stats(
        self,
        season: int,
        postseason: bool = False,
        per_page: int = 100,
    ) -> AsyncIterator[dict[str, Any]]:
        """
        Iterate through all player season stats.

        Args:
            season: Season year (e.g., 2024)
            postseason: True for playoff stats
            per_page: Results per page
        """
        params: dict[str, Any] = {
            "season": season,
            "postseason": str(postseason).lower(),
            "per_page": per_page,
        }

        while True:
            response = await self._get("/season_stats", params)
            for stats in response.get("data", []):
                yield stats

            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    # =========================================================================
    # Team Season Stats
    # =========================================================================

    async def get_team_season_stats(
        self,
        season: int,
        postseason: bool = False,
    ) -> list[dict[str, Any]]:
        """Get team season stats."""
        params = {
            "season": season,
            "postseason": str(postseason).lower(),
        }
        response = await self._get("/team_season_stats", params)
        return response.get("data", [])
