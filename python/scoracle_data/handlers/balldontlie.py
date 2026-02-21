"""
BallDontLie handler — fetches and normalizes NBA + NFL data to canonical format.

Extends BaseApiClient for HTTP infrastructure (rate limiting, retries, auth).
Two classes share the same BDL auth pattern but use different base URLs.

Canonical output shapes:
    Team:        {"id", "name", "short_code", "city", "conference", "division", "meta"}
    Player:      {"id", "name", "first_name", "last_name", "position", "height", "weight", "meta"}
    PlayerStats: {"player_id", "player" (dict), "stats" (flat dict), "raw" (full response)}
    TeamStats:   {"team_id", "stats" (flat dict), "raw" (full response)}
"""

from __future__ import annotations

import logging
from typing import Any, AsyncIterator

from ..core.http import BaseApiClient

logger = logging.getLogger(__name__)


# ============================================================================
# NBA Handler
# ============================================================================


class BDLNBAHandler(BaseApiClient):
    """Fetches NBA data from BallDontLie and normalizes to canonical format."""

    BASE_URL = "https://api.balldontlie.io/v1"

    def __init__(self, api_key: str, requests_per_minute: int = 600):
        super().__init__(
            headers={"Authorization": api_key},
            requests_per_minute=requests_per_minute,
        )

    # -- Fetch + normalize: Teams ------------------------------------------

    async def get_teams(self) -> list[dict[str, Any]]:
        """Return all NBA teams in canonical format."""
        response = await self._get("/teams")
        return [self._normalize_team(t) for t in response["data"]]

    def _normalize_team(self, raw: dict[str, Any]) -> dict[str, Any]:
        meta: dict[str, Any] = {}
        if raw.get("full_name"):
            meta["full_name"] = raw["full_name"]
        return {
            "id": raw["id"],
            "name": raw.get("name"),
            "short_code": raw.get("abbreviation"),
            "city": raw.get("city"),
            "conference": raw.get("conference"),
            "division": raw.get("division"),
            "meta": meta,
        }

    # -- Fetch + normalize: Players ----------------------------------------

    async def get_players(self, per_page: int = 100) -> AsyncIterator[dict[str, Any]]:
        """Iterate all NBA players in canonical format (cursor-paginated)."""
        params: dict[str, Any] = {"per_page": per_page}
        while True:
            response = await self._get("/players", params)
            for p in response["data"]:
                yield self._normalize_player(p)
            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    def _normalize_player(self, raw: dict[str, Any]) -> dict[str, Any]:
        team = raw.get("team") or {}
        name = f"{raw.get('first_name', '')} {raw.get('last_name', '')}".strip()
        if not name:
            name = f"Player {raw['id']}"

        meta: dict[str, Any] = {}
        for key in ("jersey_number", "college", "draft_year", "draft_round", "draft_number"):
            val = raw.get(key)
            if val is not None:
                meta[key] = val

        return {
            "id": raw["id"],
            "name": name,
            "first_name": raw.get("first_name"),
            "last_name": raw.get("last_name"),
            "position": raw.get("position"),
            "height": raw.get("height"),
            "weight": raw.get("weight"),
            "nationality": raw.get("country"),
            "team_id": team.get("id"),
            "meta": meta,
        }

    # -- Fetch + normalize: Player Stats -----------------------------------

    async def get_player_stats(
        self, season: int, season_type: str = "regular", per_page: int = 100
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate all player season averages in canonical format."""
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": "base",
            "per_page": per_page,
        }
        while True:
            response = await self._get("/season_averages/general", params)
            for raw in response.get("data", []):
                yield self._normalize_player_stats(raw, season_type)
            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    def _normalize_player_stats(
        self, raw: dict[str, Any], season_type: str
    ) -> dict[str, Any]:
        player = raw.get("player", {})
        s = raw.get("stats", {})
        # Dump all non-None stat values directly — keys match our stat_definitions
        stats = {k: v for k, v in s.items() if v is not None}
        stats["season_type"] = season_type
        # BDL uses "tov" — our triggers expect "turnover"
        if "tov" in stats:
            stats["turnover"] = stats.pop("tov")
        # BDL team stats use w/l — normalize for consistency
        if "w" in stats:
            stats["wins"] = stats.pop("w")
        if "l" in stats:
            stats["losses"] = stats.pop("l")
        if "gp" in stats:
            stats["games_played"] = stats.pop("gp")
        return {
            "player_id": player.get("id"),
            "player": self._normalize_player(player),
            "stats": stats,
            "raw": raw,
        }

    # -- Fetch + normalize: Team Stats -------------------------------------

    async def get_team_stats(
        self, season: int, season_type: str = "regular"
    ) -> list[dict[str, Any]]:
        """Return all team season averages in canonical format."""
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": "base",
            "per_page": 100,
        }
        all_data: list[dict[str, Any]] = []
        while True:
            response = await self._get("/team_season_averages/general", params)
            all_data.extend(response.get("data", []))
            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor
        return [self._normalize_team_stats(raw, season_type) for raw in all_data]

    def _normalize_team_stats(
        self, raw: dict[str, Any], season_type: str
    ) -> dict[str, Any]:
        team = raw.get("team", {})
        s = raw.get("stats", {})
        stats = {k: v for k, v in s.items() if v is not None}
        stats["season_type"] = season_type
        if "w" in stats:
            stats["wins"] = stats.pop("w")
        if "l" in stats:
            stats["losses"] = stats.pop("l")
        if "gp" in stats:
            stats["games_played"] = stats.pop("gp")
        return {
            "team_id": team.get("id"),
            "stats": stats,
            "raw": raw,
        }


# ============================================================================
# NFL Handler
# ============================================================================


class BDLNFLHandler(BaseApiClient):
    """Fetches NFL data from BallDontLie and normalizes to canonical format."""

    BASE_URL = "https://api.balldontlie.io/nfl/v1"

    def __init__(self, api_key: str, requests_per_minute: int = 600):
        super().__init__(
            headers={"Authorization": api_key},
            requests_per_minute=requests_per_minute,
        )

    # -- Fetch + normalize: Teams ------------------------------------------

    async def get_teams(self) -> list[dict[str, Any]]:
        """Return all NFL teams in canonical format."""
        response = await self._get("/teams")
        return [self._normalize_team(t) for t in response["data"]]

    def _normalize_team(self, raw: dict[str, Any]) -> dict[str, Any]:
        meta: dict[str, Any] = {}
        if raw.get("full_name"):
            meta["full_name"] = raw["full_name"]
        return {
            "id": raw["id"],
            "name": raw.get("name"),
            "short_code": raw.get("abbreviation"),
            "city": raw.get("location"),
            "conference": raw.get("conference"),
            "division": raw.get("division"),
            "meta": meta,
        }

    # -- Fetch + normalize: Players ----------------------------------------

    async def get_players(self, per_page: int = 100) -> AsyncIterator[dict[str, Any]]:
        """Iterate all NFL players in canonical format (cursor-paginated)."""
        params: dict[str, Any] = {"per_page": per_page}
        while True:
            response = await self._get("/players", params)
            for p in response["data"]:
                yield self._normalize_player(p)
            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    def _normalize_player(self, raw: dict[str, Any]) -> dict[str, Any]:
        team = raw.get("team") or {}
        name = f"{raw.get('first_name', '')} {raw.get('last_name', '')}".strip()
        if not name:
            name = f"Player {raw['id']}"

        meta: dict[str, Any] = {}
        for key in ("position_abbreviation", "jersey_number", "college", "experience", "age"):
            val = raw.get(key)
            if val is not None:
                meta[key] = val

        return {
            "id": raw["id"],
            "name": name,
            "first_name": raw.get("first_name"),
            "last_name": raw.get("last_name"),
            "position": raw.get("position"),
            "height": raw.get("height"),
            "weight": raw.get("weight"),
            "nationality": raw.get("country"),
            "team_id": team.get("id"),
            "meta": meta,
        }

    # -- Fetch + normalize: Player Stats -----------------------------------

    async def get_player_stats(
        self, season: int, postseason: bool = False, per_page: int = 100
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate all player season stats in canonical format."""
        params: dict[str, Any] = {
            "season": season,
            "postseason": str(postseason).lower(),
            "per_page": per_page,
        }
        while True:
            response = await self._get("/season_stats", params)
            for raw in response.get("data", []):
                yield self._normalize_player_stats(raw, postseason)
            next_cursor = response.get("meta", {}).get("next_cursor")
            if not next_cursor:
                break
            params["cursor"] = next_cursor

    def _normalize_player_stats(
        self, raw: dict[str, Any], postseason: bool
    ) -> dict[str, Any]:
        """BDL NFL returns flat keys — strip metadata, dump everything else as stats."""
        player = raw.get("player", {})
        exclude = {"player", "team", "id", "season"}
        stats = {k: v for k, v in raw.items() if k not in exclude and v is not None}
        stats["postseason"] = postseason
        return {
            "player_id": player.get("id"),
            "player": self._normalize_player(player),
            "stats": stats,
            "raw": raw,
        }

    # -- Fetch + normalize: Team Stats -------------------------------------

    async def get_team_stats(
        self, season: int, postseason: bool = False
    ) -> list[dict[str, Any]]:
        """Return all NFL team stats: standings merged with detailed stats."""
        # 1. Fetch standings (wins/losses/ties/points)
        standings_by_team: dict[int, dict] = {}
        try:
            standings_params = {"season": season}
            response = await self._get("/standings", standings_params)
            for row in response.get("data", []):
                tid = row.get("team", {}).get("id")
                if tid:
                    standings_by_team[tid] = row
            logger.info("Fetched standings for %d teams", len(standings_by_team))
        except Exception as e:
            logger.warning("Could not fetch standings (continuing without W/L): %s", e)

        # 2. Fetch detailed team season stats (requires team_ids[])
        results: list[dict[str, Any]] = []
        try:
            teams = await self._get("/teams")
            all_team_ids = [t["id"] for t in teams.get("data", [])]
            params: dict[str, Any] = {
                "season": season,
                "postseason": str(postseason).lower(),
                "per_page": 100,
                "team_ids[]": all_team_ids,
            }
            response = await self._get("/team_season_stats", params)
            for raw in response.get("data", []):
                tid = raw.get("team", {}).get("id")
                standing = standings_by_team.pop(tid, {})
                results.append(self._normalize_team_stats(raw, postseason, standing))
        except Exception as e:
            logger.warning("Could not fetch team_season_stats: %s", e)

        # 3. Fallback: teams only in standings (no detailed stats)
        for tid, standing in standings_by_team.items():
            results.append(self._normalize_team_stats_from_standings(standing, postseason))

        return results

    def _normalize_team_stats(
        self,
        raw: dict[str, Any],
        postseason: bool,
        standing: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        team = raw.get("team", {})
        standing = standing or {}

        # Start with standings data
        stats: dict[str, Any] = {"postseason": postseason}
        for key in ("wins", "losses", "ties", "points_for", "points_against",
                     "point_differential", "playoff_seed"):
            val = standing.get(key)
            if val is not None:
                stats[key] = val

        # Overlay detailed stats (exclude metadata objects)
        exclude = {"team", "id", "season"}
        for k, v in raw.items():
            if k not in exclude and v is not None and not isinstance(v, dict):
                stats[k] = v

        merged_raw = {**raw}
        if standing:
            merged_raw["_standing"] = standing

        return {
            "team_id": team.get("id"),
            "stats": stats,
            "raw": merged_raw,
        }

    def _normalize_team_stats_from_standings(
        self, standing: dict[str, Any], postseason: bool
    ) -> dict[str, Any]:
        team = standing.get("team", {})
        stats: dict[str, Any] = {"postseason": postseason}
        for key in ("wins", "losses", "ties", "points_for", "points_against",
                     "point_differential", "playoff_seed"):
            val = standing.get(key)
            if val is not None:
                stats[key] = val
        return {
            "team_id": team.get("id"),
            "stats": stats,
            "raw": standing,
        }
