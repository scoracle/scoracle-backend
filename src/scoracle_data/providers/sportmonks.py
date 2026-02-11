"""
SportMonks Football API client.

Provides access to football (soccer) leagues, teams, players, and statistics
via the SportMonks API (https://api.sportmonks.com/v3/football).

Uses BaseApiClient for rate limiting, retries, and lifecycle management.
The bulk /statistics/seasons/players/{id} endpoint returns empty on our
SportMonks plan, so player stats are fetched per-player via squad iteration.
"""

import logging
from typing import Any

from ..core.http import BaseApiClient

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
    # Pagination helper
    # =========================================================================

    async def _get_paginated(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        per_page: int = 50,
    ) -> list[dict[str, Any]]:
        """Fetch all pages from a paginated endpoint."""
        if params is None:
            params = {}
        params["per_page"] = per_page

        all_data: list[dict[str, Any]] = []
        page = 1

        while True:
            params["page"] = page
            response = await self._get(path, params)
            data = response.get("data", [])
            all_data.extend(data)

            pagination = response.get("pagination", {})
            if not pagination.get("has_more", False):
                break
            page += 1

        return all_data

    # =========================================================================
    # Leagues & Seasons
    # =========================================================================

    async def get_league(
        self, league_id: int, include: str | None = None
    ) -> dict[str, Any]:
        """Get a specific league."""
        params = {}
        if include:
            params["include"] = include
        response = await self._get(f"/leagues/{league_id}", params)
        return response.get("data", {})

    async def get_league_seasons(self, league_id: int) -> list[dict[str, Any]]:
        """Get all seasons for a league, sorted by name descending (newest first)."""
        response = await self._get(f"/leagues/{league_id}", {"include": "seasons"})
        data = response.get("data", {})
        seasons = data.get("seasons", [])
        return sorted(seasons, key=lambda s: s.get("name", ""), reverse=True)

    async def get_current_season_id(self, league_id: int) -> tuple[int, str] | None:
        """Get the current season ID and name for a league.

        Returns (season_id, season_name) or None if no current season.
        """
        seasons = await self.get_league_seasons(league_id)
        for season in seasons:
            if season.get("is_current"):
                return season["id"], season.get("name", "")
        # Fall back to most recent finished season
        for season in seasons:
            if season.get("finished"):
                return season["id"], season.get("name", "")
        return None

    # =========================================================================
    # Teams
    # =========================================================================

    async def get_teams_by_season(self, season_id: int) -> list[dict[str, Any]]:
        """Get all teams in a season with venue and country info."""
        return await self._get_paginated(
            f"/teams/seasons/{season_id}",
            {"include": "venue;country"},
        )

    async def get_team(self, team_id: int) -> dict[str, Any]:
        """Get a specific team."""
        response = await self._get(f"/teams/{team_id}", {"include": "venue;country"})
        return response.get("data", {})

    # =========================================================================
    # Players (Squad)
    # =========================================================================

    async def get_squad(self, season_id: int, team_id: int) -> list[dict[str, Any]]:
        """Get squad (players) for a team in a season.

        Returns list of squad entries with player_id, team_id, position_id, etc.
        """
        response = await self._get(f"/squads/seasons/{season_id}/teams/{team_id}")
        return response.get("data", [])

    async def get_player(
        self, player_id: int, include: str | None = None
    ) -> dict[str, Any]:
        """Get a specific player with optional includes."""
        params = {}
        if include:
            params["include"] = include
        response = await self._get(f"/players/{player_id}", params)
        return response.get("data", {})

    # =========================================================================
    # Player Statistics
    # =========================================================================

    async def get_player_with_stats(
        self,
        player_id: int,
        season_id: int | None = None,
    ) -> dict[str, Any]:
        """Get a player with full profile and statistics.

        Includes nationality, detailed position, and season-filtered stats
        with details (type_id-keyed stat values).
        """
        include = (
            "statistics.details;statistics.season.league;nationality;detailedPosition"
        )
        params: dict[str, Any] = {"include": include}
        if season_id:
            params["filters"] = f"playerStatisticSeasons:{season_id}"
        response = await self._get(f"/players/{player_id}", params)
        return response.get("data", {})

    async def get_squad_player_stats(
        self,
        season_id: int,
        team_id: int,
    ) -> list[dict[str, Any]]:
        """Get all player stats for a team in a season.

        Fetches the squad roster, then each player's stats individually.
        Returns list of player data dicts with statistics populated.

        This is the primary approach because the bulk endpoint
        /statistics/seasons/players/{id} returns empty on our plan tier.
        """
        squad = await self.get_squad(season_id, team_id)
        player_ids = [entry.get("player_id") or entry.get("id") for entry in squad]
        player_ids = [pid for pid in player_ids if pid]

        logger.info(
            f"Fetching stats for {len(player_ids)} players from team {team_id}..."
        )

        results = []
        for i, player_id in enumerate(player_ids):
            try:
                player_data = await self.get_player_with_stats(player_id, season_id)
                if player_data:
                    results.append(player_data)
                if (i + 1) % 10 == 0:
                    logger.info(f"  Fetched {i + 1}/{len(player_ids)} players...")
            except Exception as e:
                logger.warning(f"  Failed to fetch player {player_id}: {e}")

        return results

    # =========================================================================
    # Standings (Team Stats)
    # =========================================================================

    async def get_standings(self, season_id: int) -> list[dict[str, Any]]:
        """Get standings for a season with W/D/L/GF/GA details."""
        response = await self._get(
            f"/standings/seasons/{season_id}",
            {"include": "participant;details.type"},
        )
        return response.get("data", [])
