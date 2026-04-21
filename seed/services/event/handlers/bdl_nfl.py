"""NFL data extraction from BallDontLie API.

Thin handler: extracts raw key/value pairs from API responses.
NFL stats use flat top-level fields (not nested under "stats" key).
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from shared.bdl_client import BDLClient
from shared.models import (
    EventBoxScore,
    EventTeamStats,
    Player,
    PlayerStats,
    Team,
    TeamStats,
)
from shared.stat_keys import canonicalize

logger = logging.getLogger(__name__)

NFL_BASE_URL = "https://api.balldontlie.io"

# Keys in the /season_stats response that are metadata, not stat values
_NON_STAT_KEYS = {"player", "season", "postseason", "team"}

# BDL provider-key -> canonical-key maps. Identical to the NBA handler since
# both share the BallDontLie schema; kept inline so each handler is
# self-contained.
_PLAYER_STAT_MAP: dict[str, str] = {
    "tov": "turnover",
    "gp":  "games_played",
}
_TEAM_STAT_MAP: dict[str, str] = {
    "tov": "turnover",
    "gp":  "games_played",
    "w":   "wins",
    "l":   "losses",
}


class NFLHandler:
    """Fetches NFL data from BallDontLie and returns canonical models."""

    def __init__(self, api_key: str):
        self.client = BDLClient(NFL_BASE_URL, api_key)

    def close(self) -> None:
        self.client.close()

    # ------------------------------------------------------------------
    # Teams
    # ------------------------------------------------------------------

    def get_teams(self) -> list[Team]:
        resp = self.client.get("/nfl/v1/teams")
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

        for page in self.client.get_paginated("/nfl/v1/season_stats", params):
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
        items = self.client.get_all_pages("/nfl/v1/standings", params)
        return [_parse_standing(raw, season_type) for raw in items]

    # ------------------------------------------------------------------
    # Fixture schedule + fixture box scores
    # ------------------------------------------------------------------

    def get_games(
        self,
        season: int,
        week: int | None = None,
        from_date: str | None = None,
        to_date: str | None = None,
    ) -> list[dict[str, Any]]:
        """Fetch NFL fixture rows for a season (optionally week/date scoped)."""
        param_candidates: list[dict[str, Any]] = []
        primary: dict[str, Any] = {"seasons[]": season}
        fallback: dict[str, Any] = {"season": season}
        if week is not None:
            primary["weeks[]"] = week
            fallback["week"] = week
        if from_date:
            primary["start_date"] = from_date
            primary["dates[]"] = from_date
            fallback["start_date"] = from_date
            fallback["date"] = from_date
        if to_date:
            primary["end_date"] = to_date
            fallback["end_date"] = to_date
        param_candidates.extend([primary, fallback])

        items: list[dict[str, Any]] = []
        for params in param_candidates:
            try:
                items = self.client.get_all_pages("/nfl/v1/games", params)
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

    def get_player(self, player_id: int) -> dict | None:
        """Fetch individual player profile including photo, bio, etc.

        Args:
            player_id: BDL player ID

        Returns:
            Player profile dict or None if not found
        """
        try:
            resp = self.client.get(f"/nfl/v1/players/{player_id}")
            return resp.get("data")
        except Exception as e:
            logger.warning(f"Failed to fetch player {player_id}: {e}")
            return None

    def get_all_players(self, season: int, limit: int | None = None) -> list[dict]:
        """Fetch all NFL players for a season.

        Args:
            season: NFL season year

        Returns:
            List of player profile dicts
        """
        limit_val = limit if (limit is not None and limit > 0) else None
        try:
            items: list[dict[str, Any]] = []
            for page in self.client.get_paginated(
                "/nfl/v1/players", {"season": season, "per_page": 100}
            ):
                items.extend(page)
                if limit_val is not None and len(items) >= limit_val:
                    return items[:limit_val]
            return items
        except Exception as e:
            logger.warning(f"Failed to fetch all players for season {season}: {e}")
            return []

    def get_box_score(
        self, external_game_id: int, fixture_id: int
    ) -> tuple[list[EventBoxScore], list[EventTeamStats]]:
        """Fetch one NFL game's box score and return event-level lines."""
        raw_lines = self._fetch_box_score_lines(external_game_id)
        if not raw_lines:
            return [], []

        # Authoritative team-level aggregates (first downs, drives, red-zone,
        # possession time, penalties, etc.) that can't be derived from
        # summing player rows. Applied after player accumulation so BDL
        # team values win for keys it covers.
        team_stat_overrides = self._fetch_team_stats(external_game_id)

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

            raw_stats = _extract_numeric_stats(raw)

            player = _parse_player(player_raw) if isinstance(player_raw, dict) else None
            if player and player.team_id is None:
                player.team_id = team_id

            players.append(
                EventBoxScore(
                    fixture_id=fixture_id,
                    player_id=player_id,
                    team_id=team_id,
                    player=player,
                    stats=canonicalize(raw_stats, _PLAYER_STAT_MAP),
                    raw=raw,
                )
            )

            # Accumulate raw codes; team-side canonicalization applied at end.
            acc = team_stats_acc.setdefault(team_id, {})
            for key, value in raw_stats.items():
                if isinstance(value, (int, float)):
                    acc[key] = acc.get(key, 0.0) + float(value)

            if isinstance(game_raw, dict):
                home_team_obj = game_raw.get("home_team")
                away_team_obj = game_raw.get("visitor_team") or game_raw.get(
                    "away_team"
                )
                home_team_id = (
                    home_team_obj.get("id")
                    if isinstance(home_team_obj, dict)
                    else game_raw.get("home_team_id")
                )
                away_team_id = (
                    away_team_obj.get("id")
                    if isinstance(away_team_obj, dict)
                    else game_raw.get("visitor_team_id")
                    or game_raw.get("away_team_id")
                )
                home_score = game_raw.get("home_team_score")
                away_score = game_raw.get("visitor_team_score") or game_raw.get(
                    "away_team_score"
                )
                if isinstance(home_team_id, int) and isinstance(home_score, int):
                    team_scores[home_team_id] = home_score
                if isinstance(away_team_id, int) and isinstance(away_score, int):
                    team_scores[away_team_id] = away_score

        # Overlay BDL team-aggregate stats on top of player-sum accumulation.
        for team_id, overrides in team_stat_overrides.items():
            team_stats_acc.setdefault(team_id, {}).update(overrides)

        teams: list[EventTeamStats] = []
        for team_id, agg in team_stats_acc.items():
            teams.append(
                EventTeamStats(
                    fixture_id=fixture_id,
                    team_id=team_id,
                    score=team_scores.get(team_id),
                    stats=canonicalize(agg, _TEAM_STAT_MAP),
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
        """Fetch player stats for a game."""
        try:
            items = self.client.get_all_pages(
                "/nfl/v1/stats", {"game_ids[]": external_game_id, "per_page": 100}
            )
            if items:
                return items
        except Exception as e:
            logger.warning(f"Failed to fetch stats for game {external_game_id}: {e}")
        return []

    def _fetch_team_stats(self, external_game_id: int) -> dict[int, dict[str, Any]]:
        """Fetch team-aggregate stats for a game, keyed by team.id.

        Returns a dict[team_id, {stat_key: value}] containing fields that
        cannot be derived from summing player rows: first downs, third/fourth
        down efficiency, red zone, drives, possession time, penalties.
        Returns an empty dict on failure — player-sum fallback still works.
        """
        try:
            items = self.client.get_all_pages(
                "/nfl/v1/team_stats",
                {"game_ids[]": external_game_id, "per_page": 10},
            )
        except Exception as e:
            logger.warning(
                f"Failed to fetch team_stats for game {external_game_id}: {e}"
            )
            return {}

        out: dict[int, dict[str, Any]] = {}
        for row in items:
            team_raw = row.get("team")
            team_id = team_raw.get("id") if isinstance(team_raw, dict) else None
            if not isinstance(team_id, int):
                continue
            out[team_id] = _extract_team_numeric_stats(row)
        return out


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
    raw_stats: dict[str, Any] = {}
    for k, v in item.items():
        if k in _NON_STAT_KEYS:
            continue
        if isinstance(v, (int, float)):
            raw_stats[k] = v
        elif isinstance(v, str):
            # Try to parse string-encoded numbers
            try:
                raw_stats[k] = float(v)
            except ValueError:
                pass

    stats = canonicalize(raw_stats, _PLAYER_STAT_MAP)
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

# Fields in /nfl/v1/team_stats that aren't numeric stats — either metadata
# or string-format duplicates of numeric fields (possession_time is "31:14"
# but possession_time_seconds is available; third/fourth_down_efficiency is
# "4-12" but conversions/attempts are already separate fields).
_TEAM_STATS_SKIP_KEYS = {
    "game",
    "team",
    "home_away",
    "possession_time",
    "third_down_efficiency",
    "fourth_down_efficiency",
}


def _extract_team_numeric_stats(raw: dict[str, Any]) -> dict[str, Any]:
    """Pull numeric fields from a /nfl/v1/team_stats row, skipping metadata
    and string-format duplicates of numeric fields."""
    stats: dict[str, Any] = {}
    for k, v in raw.items():
        if k in _TEAM_STATS_SKIP_KEYS:
            continue
        if isinstance(v, (int, float)):
            stats[k] = v
        elif isinstance(v, str):
            try:
                stats[k] = float(v)
            except ValueError:
                continue
    return stats


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
