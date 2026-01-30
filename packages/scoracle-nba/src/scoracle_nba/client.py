"""BallDontLie NBA API client."""

import asyncio
import logging
import time
from typing import Any, AsyncIterator

import httpx

logger = logging.getLogger(__name__)


class RateLimiter:
    """Simple rate limiter for API calls."""
    
    def __init__(self, requests_per_minute: int = 600):
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


class BallDontLieNBA:
    """BallDontLie NBA API client."""
    
    BASE_URL = "https://api.balldontlie.io/v1"
    
    def __init__(self, api_key: str, requests_per_minute: int = 600):
        self.api_key = api_key
        self.rate_limiter = RateLimiter(requests_per_minute)
        self._client: httpx.AsyncClient | None = None
    
    async def __aenter__(self) -> "BallDontLieNBA":
        self._client = httpx.AsyncClient(
            base_url=self.BASE_URL,
            headers={"Authorization": self.api_key},
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
    
    async def get_players(
        self,
        per_page: int = 100,
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate through all players."""
        params = {"per_page": per_page}
        
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
            # BallDontLie uses array params like player_ids[]=1&player_ids[]=2
            params["player_ids[]"] = player_ids
        
        # Note: endpoint path includes /nba/ prefix for this endpoint
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
        # Team season averages use /nba/v1/ prefix
        response = await self._get("/team_season_averages/general", params)
        return response.get("data", [])
