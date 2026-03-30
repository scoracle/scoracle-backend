"""NFL data extraction from BallDontLie API.

Thin handler: extracts raw key/value pairs from API responses.
NFL stats use flat top-level fields (not nested under "stats" key).
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from .bdl_client import BDLClient
from .models import EventBoxScore, EventTeamStats, Player, PlayerStats, Team, TeamStats

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

    # ------------------------------------------------------------------
    # Fixture schedule + fixture box scores
    # ------------------------------------------------------------------

    def get_games(self, season: int, week: int | None = None) -> list[dict[str, Any]]:
        """Fetch NFL fixture rows for a season (optionally week-scoped)."""
        param_candidates: list[dict[str, Any]] = []
        primary: dict[str, Any] = {"seasons[]": season}
        fallback: dict[str, Any] = {"season": season}
        if week is not None:
            primary["weeks[]"] = week
            fallback["week"] = week
        param_candidates.extend([primary, fallback])

        items: list[dict[str, Any]] = []
        for params in param_candidates:
            try:
                items = self.client.get_all_pages("/games", params)
            except Exception:
                continue
            if items:
                break
        games: list[dict[str, Any]] = []

        for raw in items:
            home_raw = raw.get("home_team") or raw.get("home")
            away_raw = (
                raw.get("visitor_team") or raw.get("away_team") or raw.get("away")
            )
            if not isinstance(home_raw, dict) or not isinstance(away_raw, dict):
                continue

            home_id = home_raw.get("id")
            away_id = away_raw.get("id")
            external_id = raw.get("id")
            if not isinstance(home_id, int) or not isinstance(away_id, int):
                continue
            if not isinstance(external_id, int):
                continue

            start_time = (
                raw.get("date")
                or raw.get("datetime")
                or raw.get("start_time")
                or raw.get("scheduled")
            )
            if not isinstance(start_time, str):
                continue

            games.append(
                {
                    "external_id": external_id,
                    "home_team_id": home_id,
                    "away_team_id": away_id,
                    "home_team": _parse_team(home_raw),
                    "away_team": _parse_team(away_raw),
                    "start_time": start_time,
                    "season": raw.get("season", season),
                    "round": str(raw.get("week") or raw.get("status") or ""),
                    "home_score": raw.get("home_team_score"),
                    "away_score": raw.get("visitor_team_score")
                    or raw.get("away_team_score"),
                }
            )

        return games

    def get_box_score(
        self, external_game_id: int, fixture_id: int
    ) -> tuple[list[EventBoxScore], list[EventTeamStats]]:
        """Fetch one NFL game's box score and return event-level lines."""
        raw_lines = self._fetch_box_score_lines(external_game_id)
        if not raw_lines:
            return [], []

        players: list[EventBoxScore] = []
        team_stats_acc: dict[int, dict[str, float]] = {}
        team_scores: dict[int, int] = {}

        for raw in raw_lines:
            player_raw = raw.get("player")
            team_raw = raw.get("team")
            game_raw = raw.get("game")

            if not isinstance(team_raw, dict):
                continue
            team_id = team_raw.get("id")
            if not isinstance(team_id, int):
                continue

            player_id = raw.get("player_id")
            if not isinstance(player_id, int) and isinstance(player_raw, dict):
                player_id = player_raw.get("id")
            if not isinstance(player_id, int):
                continue

            stats = _extract_numeric_stats(raw)

            player = _parse_player(player_raw) if isinstance(player_raw, dict) else None
            if player and player.team_id is None:
                player.team_id = team_id

            players.append(
                EventBoxScore(
                    fixture_id=fixture_id,
                    player_id=player_id,
                    team_id=team_id,
                    player=player,
                    stats=stats,
                    raw=raw,
                )
            )

            acc = team_stats_acc.setdefault(team_id, {})
            for key, value in stats.items():
                if isinstance(value, (int, float)):
                    acc[key] = acc.get(key, 0.0) + float(value)

            if isinstance(game_raw, dict):
                home_team_id = game_raw.get("home_team_id")
                away_team_id = game_raw.get("visitor_team_id") or game_raw.get(
                    "away_team_id"
                )
                home_score = game_raw.get("home_team_score")
                away_score = game_raw.get("visitor_team_score") or game_raw.get(
                    "away_team_score"
                )
                if isinstance(home_team_id, int) and isinstance(home_score, int):
                    team_scores[home_team_id] = home_score
                if isinstance(away_team_id, int) and isinstance(away_score, int):
                    team_scores[away_team_id] = away_score

        teams: list[EventTeamStats] = []
        for team_id, agg in team_stats_acc.items():
            teams.append(
                EventTeamStats(
                    fixture_id=fixture_id,
                    team_id=team_id,
                    score=team_scores.get(team_id),
                    stats=agg,
                    raw={"provider": "bdl", "external_game_id": external_game_id},
                )
            )

        for team_id, score in team_scores.items():
            if any(t.team_id == team_id for t in teams):
                continue
            teams.append(
                EventTeamStats(
                    fixture_id=fixture_id,
                    team_id=team_id,
                    score=score,
                    stats={},
                    raw={"provider": "bdl", "external_game_id": external_game_id},
                )
            )

        return players, teams

    def _fetch_box_score_lines(self, external_game_id: int) -> list[dict[str, Any]]:
        # Use /stats endpoint with game_ids[] filter for per-game stats
        try:
            items = self.client.get_all_pages(
                "/stats", {"game_ids[]": external_game_id}
            )
            if items:
                return items
        except Exception:
            pass
        return []


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


_BOX_SCORE_META_KEYS = {
    "id",
    "player",
    "player_id",
    "team",
    "team_id",
    "game",
    "game_id",
    "season",
    "postseason",
    "week",
    "date",
}


def _extract_numeric_stats(raw: dict[str, Any]) -> dict[str, Any]:
    stats: dict[str, Any] = {}
    for k, v in raw.items():
        if k in _BOX_SCORE_META_KEYS:
            continue
        if isinstance(v, (int, float)):
            stats[k] = float(v)
        elif isinstance(v, str):
            try:
                stats[k] = float(v)
            except ValueError:
                continue
    return stats
