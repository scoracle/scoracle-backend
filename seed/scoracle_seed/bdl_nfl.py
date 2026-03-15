"""NFL data extraction from BallDontLie API.

Thin handler: extracts raw key/value pairs from API responses.
NFL stats use flat top-level fields (not nested under "stats" key).
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from .bdl_client import BDLClient
from .models import Player, PlayerStats, Team, TeamStats

logger = logging.getLogger(__name__)

NFL_BASE_URL = "https://api.balldontlie.io/nfl/v1"

# Keys in the /season_stats response that are metadata, not stat values
_NON_STAT_KEYS = {"player", "season", "postseason", "team"}


class NFLHandler:
    """Fetches NFL data from BallDontLie and returns canonical models."""

    def __init__(self, api_key: str):
        self.client = BDLClient(NFL_BASE_URL, api_key, requests_per_minute=600)

    def close(self) -> None:
        self.client.close()

    # ------------------------------------------------------------------
    # Teams
    # ------------------------------------------------------------------

    def get_teams(self) -> list[Team]:
        resp = self.client.get("/teams")
        return [_parse_team(t) for t in resp.get("data", [])]

    # ------------------------------------------------------------------
    # Player Stats — flat JSON format
    # ------------------------------------------------------------------

    def get_player_stats(
        self,
        season: int,
        postseason: bool = False,
        callback: Callable[[PlayerStats], None] | None = None,
    ) -> list[PlayerStats]:
        results: list[PlayerStats] = []
        params: dict[str, Any] = {
            "season": season,
            "postseason": str(postseason).lower(),
        }

        for page in self.client.get_paginated("/season_stats", params):
            for raw in page:
                ps = _parse_player_stats_flat(raw, postseason)
                if ps is None:
                    continue
                if callback:
                    callback(ps)
                else:
                    results.append(ps)

        return results

    # ------------------------------------------------------------------
    # Team Stats (via standings)
    # ------------------------------------------------------------------

    def get_team_stats(
        self, season: int, season_type: str = "regular"
    ) -> list[TeamStats]:
        params: dict[str, Any] = {"season": season}
        items = self.client.get_all_pages("/standings", params)
        return [_parse_standing(raw, season_type) for raw in items]


# --------------------------------------------------------------------------
# Parsing helpers
# --------------------------------------------------------------------------


def _parse_team(raw: dict[str, Any]) -> Team:
    meta: dict[str, Any] = {}
    if raw.get("full_name"):
        meta["full_name"] = raw["full_name"]

    return Team(
        id=raw["id"],
        name=raw.get("name", ""),
        short_code=raw.get("abbreviation"),
        city=raw.get("location"),
        conference=raw.get("conference"),
        division=raw.get("division"),
        meta=meta,
    )


def _parse_player(raw: dict[str, Any]) -> Player:
    first = raw.get("first_name", "")
    last = raw.get("last_name", "")
    name = f"{first} {last}".strip() or f"Player {raw.get('id', 0)}"

    meta: dict[str, Any] = {}
    if raw.get("position_abbreviation"):
        meta["position_abbreviation"] = raw["position_abbreviation"]
    jersey = raw.get("jersey_number")
    if jersey is not None:
        meta["jersey_number"] = jersey
    if raw.get("college"):
        meta["college"] = raw["college"]
    exp = raw.get("experience")
    if exp is not None:
        meta["experience"] = exp
    if raw.get("age") is not None:
        meta["age"] = raw["age"]

    team_id = None
    team_raw = raw.get("team")
    if isinstance(team_raw, dict) and team_raw.get("id"):
        team_id = team_raw["id"]

    return Player(
        id=raw.get("id", 0),
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


def _parse_player_stats_flat(
    item: dict[str, Any], postseason: bool
) -> PlayerStats | None:
    """Parse NFL flat season_stats format.

    NFL /season_stats returns flat JSON where stat fields are top-level
    alongside "player", "season", and "postseason". We extract the player,
    then treat all remaining numeric fields as stats.
    """
    player_raw = item.get("player")
    if not isinstance(player_raw, dict):
        return None

    player = _parse_player(player_raw)

    # All non-metadata fields are stats
    stats: dict[str, Any] = {}
    for k, v in item.items():
        if k in _NON_STAT_KEYS:
            continue
        if isinstance(v, (int, float)):
            stats[k] = v
        elif isinstance(v, str):
            # Try to parse string-encoded numbers
            try:
                stats[k] = float(v)
            except ValueError:
                pass

    stats["season_type"] = "postseason" if postseason else "regular"

    return PlayerStats(
        player_id=player.id,
        team_id=player.team_id,
        player=player,
        stats=stats,
        raw=item,
    )


def _parse_standing(raw: dict[str, Any], season_type: str) -> TeamStats:
    team_raw = raw.get("team", {})
    stats: dict[str, Any] = {
        "wins": float(raw.get("wins", 0)),
        "losses": float(raw.get("losses", 0)),
        "ties": float(raw.get("ties", 0)),
        "points_for": float(raw.get("points_for", 0)),
        "points_against": float(raw.get("points_against", 0)),
        "point_differential": float(raw.get("point_differential", 0)),
        "season_type": season_type,
    }

    return TeamStats(
        team_id=team_raw.get("id", raw.get("id", 0)),
        stats=stats,
        raw=raw,
    )
