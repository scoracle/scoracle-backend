"""NBA data extraction from BallDontLie API.

Thin handler: extracts raw key/value pairs from API responses.
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from .bdl_client import BDLClient
from .models import EventBoxScore, EventTeamStats, Player, PlayerStats, Team, TeamStats

logger = logging.getLogger(__name__)

NBA_BASE_URL = "https://api.balldontlie.io"

# Valid NBA team IDs (1-30) - filters out historical BAA/NFL and defunct teams
NBA_TEAM_IDS = set(range(1, 31))


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
        resp = self.client.get("/nba/v1/teams")
        teams = []
        for t in resp.get("data", []):
            team_id = t.get("id")
            if team_id in NBA_TEAM_IDS:
                teams.append(_parse_team(t))
            else:
                logger.debug(
                    "Skipping non-current NBA team: %s (ID: %s)", t.get("name"), team_id
                )
        return teams

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

        for page in self.client.get_paginated(
            "/nba/v1/season_averages/general", params
        ):
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
        items = self.client.get_all_pages(
            "/nba/v1/team_season_averages/general", params
        )
        return [_parse_team_stats(raw, season_type) for raw in items]

    # ------------------------------------------------------------------
    # Fixture schedule + fixture box scores
    # ------------------------------------------------------------------

    def get_games(
        self,
        season: int,
        from_date: str | None = None,
        to_date: str | None = None,
    ) -> list[dict[str, Any]]:
        """Fetch fixture schedule rows for a season/date range."""
        param_candidates: list[dict[str, Any]] = []
        primary: dict[str, Any] = {"seasons[]": season}
        fallback: dict[str, Any] = {"season": season}
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
                items = self.client.get_all_pages("/nba/v1/games", params)
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
            # Filter out games with non-current NBA teams
            if home_id not in NBA_TEAM_IDS or away_id not in NBA_TEAM_IDS:
                logger.debug(
                    "Skipping game with non-NBA teams: %s vs %s", home_id, away_id
                )
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
                    "round": raw.get("status"),
                    "home_score": raw.get("home_team_score"),
                    "away_score": raw.get("visitor_team_score")
                    or raw.get("away_team_score"),
                }
            )

        return games

    def get_box_score(
        self, external_game_id: int, fixture_id: int
    ) -> tuple[list[EventBoxScore], list[EventTeamStats]]:
        """Fetch one game's box score and return event-level player/team lines."""
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

            stats = _extract_numeric_stats(raw, explicit_stats_key="stats")
            minutes_val = raw.get("min") or raw.get("minutes")
            if minutes_val is None and isinstance(raw.get("stats"), dict):
                minutes_val = raw["stats"].get("minutes")
            minutes = _parse_minutes(minutes_val)

            player = _parse_player(player_raw) if isinstance(player_raw, dict) else None
            if player and player.team_id is None:
                player.team_id = team_id

            players.append(
                EventBoxScore(
                    fixture_id=fixture_id,
                    player_id=player_id,
                    team_id=team_id,
                    player=player,
                    minutes_played=minutes,
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
        """Fetch player stats for a game."""
        # Use /stats endpoint which is most reliable
        params = {"game_ids[]": external_game_id, "per_page": 100}
        try:
            return self.client.get_all_pages("/nba/v1/stats", params)
        except Exception as e:
            logger.warning(f"Failed to fetch stats for game {external_game_id}: {e}")
            return []

    def get_advanced_stats(self, game_id: int) -> list[dict[str, Any]]:
        """Fetch advanced stats V2 (GOAT tier) - includes hustle, tracking, PIE, etc."""
        params = {"game_ids[]": game_id, "per_page": 100}
        try:
            return self.client.get_all_pages("/nba/v2/stats/advanced", params)
        except Exception as e:
            logger.debug(f"Advanced stats not available for game {game_id}: {e}")
            return []

    def get_lineups(self, game_id: int) -> list[dict[str, Any]]:
        """Fetch lineup data (GOAT tier) - starters, positions, etc."""
        params = {"game_ids[]": game_id, "per_page": 100}
        try:
            return self.client.get_all_pages("/nba/v1/lineups", params)
        except Exception as e:
            logger.debug(f"Lineups not available for game {game_id}: {e}")
            return []

    def get_team_box_score(self, game_date: str) -> dict[str, Any] | None:
        """Fetch team-level box score data (GOAT tier) - quarter scores, timeouts, etc."""
        try:
            result = self.client.get(
                "/nba/v1/box_scores", {"date": game_date, "per_page": 1}
            )
            data = result.get("data", [])
            return data[0] if data else None
        except Exception as e:
            logger.debug(f"Team box score not available for date {game_date}: {e}")
            return None


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
    player_id = raw.get("id", 0)
    first = raw.get("first_name", "")
    last = raw.get("last_name", "")
    name = f"{first} {last}".strip() or f"Player {player_id}"

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
        id=player_id,
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
    "date",
    "min",
    "minutes",
}


def _extract_numeric_stats(
    raw: dict[str, Any], explicit_stats_key: str | None = None
) -> dict[str, Any]:
    stats: dict[str, Any] = {}

    if explicit_stats_key and isinstance(raw.get(explicit_stats_key), dict):
        for k, v in raw[explicit_stats_key].items():
            if isinstance(v, (int, float)):
                stats[k] = float(v)
            elif isinstance(v, str):
                try:
                    stats[k] = float(v)
                except ValueError:
                    continue

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


def _parse_minutes(val: Any) -> float | None:
    if val is None:
        return None
    if isinstance(val, (int, float)):
        return float(val)
    if isinstance(val, str):
        if ":" in val:
            parts = val.split(":")
            if len(parts) == 2:
                try:
                    minutes = int(parts[0])
                    seconds = int(parts[1])
                    return round(minutes + (seconds / 60.0), 2)
                except ValueError:
                    return None
        try:
            return float(val)
        except ValueError:
            return None
    return None
