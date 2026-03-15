"""NBA data extraction from BallDontLie API.

Thin handler: extracts raw key/value pairs from API responses.
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from .bdl_client import BDLClient
from .models import Player, PlayerStats, Team, TeamStats

logger = logging.getLogger(__name__)

NBA_BASE_URL = "https://api.balldontlie.io/v1"


class NBAHandler:
    """Fetches NBA data from BallDontLie and returns canonical models."""

    def __init__(self, api_key: str):
        self.client = BDLClient(NBA_BASE_URL, api_key, requests_per_minute=600)

    def close(self) -> None:
        self.client.close()

    # ------------------------------------------------------------------
    # Teams
    # ------------------------------------------------------------------

    def get_teams(self) -> list[Team]:
        resp = self.client.get("/teams")
        return [_parse_team(t) for t in resp.get("data", [])]

    # ------------------------------------------------------------------
    # Player Stats (includes embedded player profiles)
    # ------------------------------------------------------------------

    def get_player_stats(
        self,
        season: int,
        season_type: str = "regular",
        callback: Callable[[PlayerStats], None] | None = None,
    ) -> list[PlayerStats]:
        """Fetch all player season averages via cursor pagination.

        If callback is provided, calls it for each PlayerStats and returns
        an empty list. Otherwise collects and returns all.
        """
        results: list[PlayerStats] = []
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": "base",
        }

        for page in self.client.get_paginated("/season_averages/general", params):
            for raw in page:
                ps = _parse_player_stats(raw, season_type)
                if callback:
                    callback(ps)
                else:
                    results.append(ps)

        return results

    # ------------------------------------------------------------------
    # Team Stats
    # ------------------------------------------------------------------

    def get_team_stats(
        self, season: int, season_type: str = "regular"
    ) -> list[TeamStats]:
        params: dict[str, Any] = {
            "season": season,
            "season_type": season_type,
            "type": "base",
        }
        items = self.client.get_all_pages("/team_season_averages/general", params)
        return [_parse_team_stats(raw, season_type) for raw in items]


# --------------------------------------------------------------------------
# Parsing helpers — extract raw values, no normalization
# --------------------------------------------------------------------------


def _parse_team(raw: dict[str, Any]) -> Team:
    meta: dict[str, Any] = {}
    if raw.get("full_name"):
        meta["full_name"] = raw["full_name"]

    return Team(
        id=raw["id"],
        name=raw.get("name", ""),
        short_code=raw.get("abbreviation"),
        city=raw.get("city"),
        conference=raw.get("conference"),
        division=raw.get("division"),
        meta=meta,
    )


def _parse_player(raw: dict[str, Any]) -> Player:
    first = raw.get("first_name", "")
    last = raw.get("last_name", "")
    name = f"{first} {last}".strip() or f"Player {raw['id']}"

    meta: dict[str, Any] = {}
    jersey = raw.get("jersey_number")
    if jersey is not None:
        meta["jersey_number"] = jersey
    if raw.get("college"):
        meta["college"] = raw["college"]
    for key in ("draft_year", "draft_round", "draft_number"):
        if raw.get(key) is not None:
            meta[key] = raw[key]

    team_id = None
    team_raw = raw.get("team")
    if isinstance(team_raw, dict) and team_raw.get("id"):
        team_id = team_raw["id"]

    return Player(
        id=raw["id"],
        name=name,
        first_name=first or None,
        last_name=last or None,
        position=raw.get("position") or None,
        height=raw.get("height") or None,
        weight=raw.get("weight") or None,
        nationality=raw.get("country") or None,
        team_id=team_id,
        meta=meta,
    )


def _parse_player_stats(raw: dict[str, Any], season_type: str) -> PlayerStats:
    player_raw = raw.get("player", {})
    player = _parse_player(player_raw)

    # Stats are in raw["stats"] — pass through as-is (raw provider keys)
    stats = dict(raw.get("stats", {}))
    # Filter out None values
    stats = {k: v for k, v in stats.items() if v is not None}
    stats["season_type"] = season_type

    return PlayerStats(
        player_id=player.id,
        team_id=player.team_id,
        player=player,
        stats=stats,
        raw=raw,
    )


def _parse_team_stats(raw: dict[str, Any], season_type: str) -> TeamStats:
    team_raw = raw.get("team", {})
    stats = dict(raw.get("stats", {}))
    stats = {k: v for k, v in stats.items() if v is not None}
    stats["season_type"] = season_type

    return TeamStats(
        team_id=team_raw.get("id", 0),
        stats=stats,
        raw=raw,
    )
