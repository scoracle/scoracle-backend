"""
BallDontLie NBA API client.

Provides access to NBA teams, players, and season averages
via the BallDontLie API (https://api.balldontlie.io).
"""

import logging
from typing import Any, AsyncIterator

from ..core.http import BaseApiClient

logger = logging.getLogger(__name__)


class BallDontLieNBA(BaseApiClient):
    """BallDontLie NBA API client."""

    BASE_URL = "https://api.balldontlie.io/v1"

    def __init__(self, api_key: str, requests_per_minute: int = 600):
        super().__init__(
            headers={"Authorization": api_key},
            requests_per_minute=requests_per_minute,
        )

    # =========================================================================
    # Teams
    # =========================================================================

    async def get_teams(self) -> list[dict[str, Any]]:
        """Get all NBA teams."""
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
    # Season Averages
    # =========================================================================

    async def get_season_averages(
        self,
        season: int,
        season_type: str = "regular",
        player_ids: list[int] | None = None,
        category: str = "general",
        stat_type: str = "base",
    ) -> list[dict[str, Any]]:
        """
        Get player season averages.

        Args:
            season: Season year (e.g., 2024 for 2024-25 season)
            season_type: "regular", "playoffs", "ist", or "playin"
            player_ids: Optional list of player IDs to filter
            category: Stats category (e.g., "general", "shooting", "defense")
            stat_type: Type within category (e.g., "base", "advanced")
        """
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": stat_type,
        }

        if player_ids:
            params["player_ids[]"] = player_ids

        response = await self._get(f"/season_averages/{category}", params)
        return response.get("data", [])

    async def get_all_season_averages(
        self,
        season: int,
        season_type: str = "regular",
        per_page: int = 100,
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate through all player season averages for a season."""
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": "base",
            "per_page": per_page,
        }

        while True:
            response = await self._get("/season_averages/general", params)
            for stats in response.get("data", []):
                yield stats

            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    # =========================================================================
    # Team Season Averages
    # =========================================================================

    async def get_team_season_averages(
        self,
        season: int,
        season_type: str = "regular",
    ) -> list[dict[str, Any]]:
        """Get team season averages."""
        params = {
            "season": season,
            "season_type": season_type,
            "type": "base",
        }
        response = await self._get("/team_season_averages/general", params)
        return response.get("data", [])
