"""
API client interface and implementations for statsdb.

This module provides a clean abstraction over the API-Sports client,
allowing statsdb to work both:
1. Within Scoracle (using app.services.apisports)
2. Standalone in scoracle-data (using built-in httpx client)

Usage:
    # In Scoracle
    from scoracle_data.api_client import get_api_client
    client = get_api_client()  # Uses apisports_service if available

    # In scoracle-data (standalone)
    from scoracle_data.api_client import StandaloneApiClient
    client = StandaloneApiClient(api_key="your_key")
"""
from __future__ import annotations

import logging
import os
from abc import ABC, abstractmethod
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)


class ApiClientProtocol(ABC):
    """Protocol for API-Sports client implementations.

    Both the Scoracle apisports_service and standalone clients
    implement this interface.
    """

    @abstractmethod
    async def list_teams(
        self,
        sport_key: str,
        league: Optional[Any] = None,
        season: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """List all teams for a sport."""
        ...

    @abstractmethod
    async def list_players(
        self,
        sport_key: str,
        season: Optional[str] = None,
        page: int = 1,
        league: Optional[Any] = None,
        team_id: Optional[int] = None
    ) -> List[Dict[str, Any]]:
        """List players for a sport with pagination."""
        ...

    @abstractmethod
    async def get_player_statistics(
        self,
        player_id: str,
        sport_key: str,
        season: Optional[str] = None,
        league_id: Optional[int] = None
    ) -> Dict[str, Any]:
        """Fetch player statistics."""
        ...

    @abstractmethod
    async def get_team_statistics(
        self,
        team_id: str,
        sport_key: str,
        season: Optional[str] = None,
        league_id: Optional[int] = None
    ) -> Dict[str, Any]:
        """Fetch team statistics."""
        ...

    @abstractmethod
    async def get_standings(
        self,
        sport_key: str,
        season: Optional[str] = None
    ) -> Dict[str, Any]:
        """Fetch standings for a sport."""
        ...


class StandaloneApiClient(ApiClientProtocol):
    """Standalone API client for scoracle-data repo.

    Lightweight httpx-based client that doesn't depend on Scoracle's
    service layer. Used when running statsdb as a standalone package.
    """

    # API-Sports base URLs by sport
    BASE_URLS = {
        "FOOTBALL": "https://v3.football.api-sports.io",
        "NBA": "https://v2.nba.api-sports.io",
        "NFL": "https://v1.american-football.api-sports.io",
    }

    def __init__(self, api_key: Optional[str] = None):
        self.api_key = api_key or os.getenv("API_SPORTS_KEY")
        if not self.api_key:
            logger.warning("API_SPORTS_KEY not set - API calls will fail")

    def _get_base_url(self, sport_key: str) -> str:
        url = self.BASE_URLS.get(sport_key.upper())
        if not url:
            raise ValueError(f"Unsupported sport: {sport_key}")
        return url

    def _headers(self) -> Dict[str, str]:
        return {"x-apisports-key": self.api_key or ""}

    async def _request(
        self,
        sport_key: str,
        endpoint: str,
        params: Optional[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        """Make an API request."""
        import httpx

        base = self._get_base_url(sport_key)
        headers = self._headers()

        async with httpx.AsyncClient(timeout=30.0) as client:
            r = await client.get(f"{base}/{endpoint}", headers=headers, params=params)
            r.raise_for_status()
            return r.json()

    async def list_teams(
        self,
        sport_key: str,
        league: Optional[Any] = None,
        season: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        params: Dict[str, Any] = {}
        if league:
            params["league"] = league
        if season:
            params["season"] = season

        payload = await self._request(sport_key, "teams", params or None)
        response = payload.get("response", [])

        # Normalize team data
        out = []
        for t in response:
            team = t.get("team") or t
            out.append({
                "id": team.get("id"),
                "name": team.get("name"),
                "abbreviation": team.get("code") or team.get("nickname") or team.get("name"),
            })
        return out

    async def list_players(
        self,
        sport_key: str,
        season: Optional[str] = None,
        page: int = 1,
        league: Optional[Any] = None,
        team_id: Optional[int] = None
    ) -> List[Dict[str, Any]]:
        params: Dict[str, Any] = {}
        if season:
            params["season"] = season
        if team_id:
            params["team"] = team_id
        if sport_key.upper() == "FOOTBALL":
            params["page"] = page
            if league:
                params["league"] = league

        payload = await self._request(sport_key, "players", params)
        return payload.get("response", [])

    async def get_player_statistics(
        self,
        player_id: str,
        sport_key: str,
        season: Optional[str] = None,
        league_id: Optional[int] = None
    ) -> Dict[str, Any]:
        sport_up = sport_key.upper()
        params: Dict[str, Any] = {"id": player_id}
        if season:
            params["season"] = int(season) if str(season).isdigit() else season
        if league_id and sport_up == "FOOTBALL":
            params["league"] = league_id

        endpoint = "players" if sport_up == "FOOTBALL" else "players/statistics"
        payload = await self._request(sport_key, endpoint, params)
        rows = payload.get("response", [])
        return rows[0] if rows else {}

    async def get_team_statistics(
        self,
        team_id: str,
        sport_key: str,
        season: Optional[str] = None,
        league_id: Optional[int] = None
    ) -> Dict[str, Any]:
        sport_up = sport_key.upper()

        if sport_up == "NFL":
            params: Dict[str, Any] = {"team": team_id, "league": league_id or 1}
            if season:
                params["season"] = int(season) if str(season).isdigit() else season
            payload = await self._request(sport_key, "standings", params)
            rows = payload.get("response", [])
            return rows[0] if rows else {}

        params = {"id" if sport_up == "NBA" else "team": team_id}
        if season:
            params["season"] = int(season) if str(season).isdigit() else season
        if sport_up == "FOOTBALL" and league_id:
            params["league"] = league_id

        payload = await self._request(sport_key, "teams/statistics", params)
        resp = payload.get("response")
        if isinstance(resp, list):
            return resp[0] if resp else {}
        return resp or {}

    async def get_standings(
        self,
        sport_key: str,
        season: Optional[str] = None
    ) -> Dict[str, Any]:
        params: Dict[str, Any] = {}
        if season:
            params["season"] = int(season) if str(season).isdigit() else season
        return await self._request(sport_key, "standings", params)


# Global client instance
_api_client: Optional[ApiClientProtocol] = None


def get_api_client() -> ApiClientProtocol:
    """Get the API client instance.

    Tries to use Scoracle's apisports_service first (when running within Scoracle).
    Falls back to StandaloneApiClient (when running as standalone scoracle-data).
    """
    global _api_client

    if _api_client is not None:
        return _api_client

    # Try to import Scoracle's apisports_service
    try:
        from ..services.apisports import apisports_service
        _api_client = apisports_service
        logger.debug("Using Scoracle apisports_service")
        return _api_client
    except ImportError:
        pass

    # Fall back to standalone client
    _api_client = StandaloneApiClient()
    logger.debug("Using standalone API client")
    return _api_client


def set_api_client(client: ApiClientProtocol) -> None:
    """Set a custom API client (for testing or custom implementations)."""
    global _api_client
    _api_client = client
