"""SportMonks Football API client."""

import asyncio
import logging
import time
from typing import Any, AsyncIterator

import httpx

logger = logging.getLogger(__name__)


class RateLimiter:
    """Simple rate limiter for API calls."""
    
    def __init__(self, requests_per_minute: int = 300):
        # SportMonks Advanced plan: 300 req/min
        self.delay = 60.0 / requests_per_minute
        self._last_request = 0.0
        self._lock = asyncio.Lock()
    
    async def acquire(self) -> None:
        async with self._lock:
            now = time.monotonic()
            elapsed = now - self._last_request
            if elapsed < self.delay:
                await asyncio.sleep(self.delay - elapsed)
            self._last_request = time.monotonic()


class SportMonksClient:
    """SportMonks Football API client."""
    
    BASE_URL = "https://api.sportmonks.com/v3/football"
    
    def __init__(self, api_token: str, requests_per_minute: int = 300):
        self.api_token = api_token
        self.rate_limiter = RateLimiter(requests_per_minute)
        self._client: httpx.AsyncClient | None = None
    
    async def __aenter__(self) -> "SportMonksClient":
        self._client = httpx.AsyncClient(
            base_url=self.BASE_URL,
            timeout=30.0,
        )
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None
    
    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise RuntimeError("Client not initialized. Use 'async with'")
        return self._client
    
    async def _get(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        max_retries: int = 3,
    ) -> dict[str, Any]:
        """Make a GET request with retries."""
        if params is None:
            params = {}
        params["api_token"] = self.api_token
        
        last_error: Exception | None = None
        
        for attempt in range(max_retries):
            try:
                await self.rate_limiter.acquire()
                response = await self.client.get(path, params=params)
                response.raise_for_status()
                return response.json()
            except httpx.HTTPStatusError as e:
                last_error = e
                status = e.response.status_code
                if 400 <= status < 500 and status != 429:
                    logger.error(f"Client error {status} for {path}")
                    raise
                if attempt < max_retries - 1:
                    wait = (attempt + 1) * 2
                    logger.warning(f"Request failed, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)
            except httpx.RequestError as e:
                last_error = e
                if attempt < max_retries - 1:
                    wait = (attempt + 1) * 2
                    logger.warning(f"Request error, retrying in {wait}s: {e}")
                    await asyncio.sleep(wait)
        
        raise last_error or RuntimeError("Request failed")
    
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
            include = f"statistics;statistics.details"
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
