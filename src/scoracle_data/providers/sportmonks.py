"""
SportMonks Football API client.

Provides access to football (soccer) leagues, teams, players, and statistics
via the SportMonks API (https://api.sportmonks.com/v3/football).
"""

import logging
from typing import Any, AsyncIterator

from .http import BaseApiClient

logger = logging.getLogger(__name__)


class SportMonksClient(BaseApiClient):
    """SportMonks Football API client."""

    BASE_URL = "https://api.sportmonks.com/v3/football"

    def __init__(self, api_token: str, requests_per_minute: int = 300):
        # SportMonks uses api_token as a query parameter, not a header
        super().__init__(
            params={"api_token": api_token},
            requests_per_minute=requests_per_minute,
        )

    # =========================================================================
    # Leagues
    # =========================================================================

    async def get_league(self, league_id: int, include: str | None = None) -> dict[str, Any]:
        """Get a specific league."""
        params = {}
        if include:
            params["include"] = include
        response = await self._get(f"/leagues/{league_id}", params)
        return response.get("data", {})

    async def get_league_seasons(self, league_id: int) -> list[dict[str, Any]]:
        """Get all seasons for a league."""
        response = await self._get(f"/leagues/{league_id}", {"include": "seasons"})
        data = response.get("data", {})
        return data.get("seasons", [])

    # =========================================================================
    # Teams
    # =========================================================================

    async def get_teams_by_season(self, season_id: int) -> list[dict[str, Any]]:
        """Get all teams in a season."""
        response = await self._get(f"/teams/seasons/{season_id}", {"include": "venue"})
        return response.get("data", [])

    async def get_team(self, team_id: int) -> dict[str, Any]:
        """Get a specific team."""
        response = await self._get(f"/teams/{team_id}", {"include": "venue"})
        return response.get("data", {})

    # =========================================================================
    # Players (Squad)
    # =========================================================================

    async def get_squad(self, season_id: int, team_id: int) -> list[dict[str, Any]]:
        """Get squad (players) for a team in a season."""
        response = await self._get(f"/squads/seasons/{season_id}/teams/{team_id}")
        return response.get("data", [])

    async def get_player(self, player_id: int, include: str | None = None) -> dict[str, Any]:
        """Get a specific player with optional includes."""
        params = {}
        if include:
            params["include"] = include
        response = await self._get(f"/players/{player_id}", params)
        return response.get("data", {})

    # =========================================================================
    # Player Statistics
    # =========================================================================

    async def get_player_statistics(
        self,
        player_id: int,
        season_id: int | None = None,
    ) -> dict[str, Any]:
        """Get player with statistics."""
        include = "statistics"
        if season_id:
            include = "statistics;statistics.details"
        response = await self._get(f"/players/{player_id}", {"include": include})
        return response.get("data", {})

    async def get_season_player_statistics(
        self,
        season_id: int,
        page: int = 1,
        per_page: int = 100,
    ) -> tuple[list[dict[str, Any]], dict[str, Any]]:
        """Get all player statistics for a season (paginated)."""
        params = {
            "include": "player;team",
            "page": page,
            "per_page": per_page,
        }
        response = await self._get(f"/statistics/seasons/players/{season_id}", params)
        data = response.get("data", [])
        pagination = response.get("pagination", {})
        return data, pagination

    async def iterate_season_player_statistics(
        self,
        season_id: int,
        per_page: int = 100,
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate through all player statistics for a season."""
        page = 1
        while True:
            data, pagination = await self.get_season_player_statistics(
                season_id, page=page, per_page=per_page
            )
            for stat in data:
                yield stat

            has_more = pagination.get("has_more", False)
            if not has_more:
                break
            page += 1

    # =========================================================================
    # Standings (Team Stats)
    # =========================================================================

    async def get_standings(self, season_id: int) -> list[dict[str, Any]]:
        """Get standings for a season."""
        response = await self._get(f"/standings/seasons/{season_id}", {"include": "participant"})
        return response.get("data", [])
