"""Football data extraction from SportMonks API.

Thin handler: navigates deeply nested JSON, extracts raw key/value pairs.
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from .models import Player, PlayerStats, Team, TeamStats
from .sportmonks_client import SportMonksClient

logger = logging.getLogger(__name__)

# Position ID to name mapping (SportMonks uses numeric IDs)
_POSITION_MAP = {24: "Goalkeeper", 25: "Defender", 26: "Midfielder", 27: "Attacker"}


class FootballHandler:
    """Fetches Football data from SportMonks and returns canonical models."""

    def __init__(self, api_token: str):
        self.client = SportMonksClient(api_token, requests_per_minute=300)

    def close(self) -> None:
        self.client.close()

    # ------------------------------------------------------------------
    # Season discovery
    # ------------------------------------------------------------------

    def discover_season_ids(
        self, league_id: int, target_years: list[int]
    ) -> dict[int, int]:
        """Map target years to SportMonks season IDs for a league."""
        resp = self.client.get(f"/leagues/{league_id}", {"include": "seasons"})
        data = resp.get("data", {})
        seasons = data.get("seasons", [])

        target_set = set(target_years)
        result: dict[int, int] = {}

        for season in seasons:
            name = season.get("name", "")
            parts = name.split("/")
            try:
                start_year = int(parts[0].strip())
            except (ValueError, IndexError):
                continue
            if start_year in target_set and start_year not in result:
                result[start_year] = season["id"]

        return result

    # ------------------------------------------------------------------
    # Teams
    # ------------------------------------------------------------------

    def get_teams(self, season_id: int) -> list[Team]:
        items = self.client.get_all_pages(
            f"/teams/seasons/{season_id}",
            {"include": "venue;country"},
        )
        return [_parse_team(t) for t in items]

    # ------------------------------------------------------------------
    # Players + Stats (via squad iteration — N+1 pattern)
    # ------------------------------------------------------------------

    def get_players_with_stats(
        self,
        season_id: int,
        team_ids: list[int],
        sm_league_id: int,
        callback: Callable[[PlayerStats], None] | None = None,
    ) -> list[PlayerStats]:
        """Iterate squads, fetch per-player stats, return canonical PlayerStats."""
        results: list[PlayerStats] = []

        for i, team_id in enumerate(team_ids):
            logger.info(
                "Fetching squad team_id=%d (%d/%d)", team_id, i + 1, len(team_ids)
            )

            try:
                resp = self.client.get(f"/squads/seasons/{season_id}/teams/{team_id}")
            except Exception as exc:
                logger.warning("Squad fetch failed team_id=%d: %s", team_id, exc)
                continue

            squad = resp.get("data", [])
            player_ids = []
            for entry in squad:
                pid = entry.get("player_id") or entry.get("id", 0)
                if pid:
                    player_ids.append(pid)

            for j, pid in enumerate(player_ids):
                try:
                    player_resp = self.client.get(
                        f"/players/{pid}",
                        {
                            "include": "statistics.details.type;statistics.season.league;nationality;detailedPosition",
                            "filters": f"playerStatisticSeasons:{season_id}",
                        },
                    )
                except Exception as exc:
                    logger.warning("Player fetch failed player_id=%d: %s", pid, exc)
                    continue

                player_data = player_resp.get("data", {})
                stats = _extract_league_stats(
                    player_data.get("statistics", []), sm_league_id
                )
                player = _parse_player(player_data)

                ps = PlayerStats(
                    player_id=player_data.get("id", pid),
                    team_id=team_id,
                    player=player,
                    stats=stats,
                    raw=player_data,
                )

                if callback:
                    callback(ps)
                else:
                    results.append(ps)

                if (j + 1) % 10 == 0:
                    logger.info(
                        "Player progress team_id=%d: %d/%d",
                        team_id,
                        j + 1,
                        len(player_ids),
                    )

        return results

    # ------------------------------------------------------------------
    # Team Stats (Standings)
    # ------------------------------------------------------------------

    def get_team_stats(self, season_id: int) -> list[TeamStats]:
        resp = self.client.get(
            f"/standings/seasons/{season_id}",
            {"include": "participant;details.type"},
        )
        raw_standings = resp.get("data", [])
        results = [_parse_standing(s) for s in raw_standings]
        # Sort by position
        results.sort(key=lambda ts: ts.stats.get("position", 999))
        return results


# --------------------------------------------------------------------------
# Parsing helpers
# --------------------------------------------------------------------------


def _extract_value(val: Any) -> float | None:
    """Extract a numeric value from various API response formats.

    SportMonks returns dicts like {"total": 15, "goals": 12} for some stats.
    BDL returns flat numbers. This handles both.
    """
    if val is None:
        return None
    if isinstance(val, (int, float)):
        return float(val)
    if isinstance(val, str):
        try:
            return float(val)
        except ValueError:
            return None
    if isinstance(val, dict):
        for key in ("total", "all", "count", "average"):
            if key in val and val[key] is not None:
                return _extract_value(val[key])
    return None


def _parse_team(raw: dict[str, Any]) -> Team:
    meta: dict[str, Any] = {}
    country = None
    country_raw = raw.get("country")
    if isinstance(country_raw, dict):
        country = country_raw.get("name")

    venue_name = None
    venue_capacity = None
    venue_raw = raw.get("venue")
    if isinstance(venue_raw, dict):
        venue_name = venue_raw.get("name")
        venue_capacity = venue_raw.get("capacity")
        if venue_raw.get("city"):
            meta["venue_city"] = venue_raw["city"]
        if venue_raw.get("surface"):
            meta["venue_surface"] = venue_raw["surface"]

    return Team(
        id=raw["id"],
        name=raw.get("name", ""),
        short_code=raw.get("short_code"),
        country=country,
        logo_url=raw.get("image_path"),
        founded=raw.get("founded"),
        venue_name=venue_name,
        venue_capacity=venue_capacity,
        meta=meta,
    )


def _parse_player(raw: dict[str, Any]) -> Player:
    name = raw.get("display_name") or ""
    if not name:
        first = raw.get("firstname", "")
        last = raw.get("lastname", "")
        name = f"{first} {last}".strip()
    if not name:
        name = f"Player {raw.get('id', 0)}"

    # Position from position_id
    position = None
    pos_raw = raw.get("position")
    if isinstance(pos_raw, str):
        position = pos_raw
    if not position and raw.get("position_id"):
        position = _POSITION_MAP.get(raw["position_id"])

    # Detailed position
    detailed_position = None
    dp_raw = raw.get("detailedposition")
    if isinstance(dp_raw, dict):
        detailed_position = dp_raw.get("name")
    elif isinstance(dp_raw, str):
        detailed_position = dp_raw

    # Nationality
    nationality = None
    nat_raw = raw.get("nationality")
    if isinstance(nat_raw, dict):
        nationality = nat_raw.get("name")
    elif isinstance(nat_raw, str):
        nationality = nat_raw

    # Height/weight: store raw provider values (no unit conversion)
    height = None
    if raw.get("height") is not None:
        height = str(raw["height"])
    weight = None
    if raw.get("weight") is not None:
        weight = str(raw["weight"])

    meta: dict[str, Any] = {}
    if raw.get("display_name"):
        meta["display_name"] = raw["display_name"]
    if raw.get("position_id") is not None:
        meta["position_id"] = raw["position_id"]

    return Player(
        id=raw.get("id", 0),
        name=name,
        first_name=raw.get("firstname"),
        last_name=raw.get("lastname"),
        position=position,
        detailed_position=detailed_position,
        nationality=nationality,
        height=height,
        weight=weight,
        date_of_birth=raw.get("date_of_birth"),
        photo_url=raw.get("image_path"),
        meta=meta,
    )


def _extract_league_stats(
    statistics: list[dict[str, Any]], sm_league_id: int
) -> dict[str, Any]:
    """Extract stats for a specific league from the statistics array."""
    for block in statistics:
        season = block.get("season")
        if not isinstance(season, dict):
            continue
        league = season.get("league")
        if not isinstance(league, dict):
            continue
        if league.get("id") == sm_league_id:
            return _normalize_player_stats(block.get("details", []))
    return {}


def _normalize_player_stats(details: list[dict[str, Any]]) -> dict[str, Any]:
    """Extract stat key/value pairs from details array.

    Raw keys are passed through — Postgres handles normalization.
    """
    stats: dict[str, Any] = {}
    for detail in details:
        type_info = detail.get("type")
        if not isinstance(type_info, dict):
            continue
        code = type_info.get("code", "")
        if not code:
            continue
        val = _extract_value(detail.get("value"))
        if val is not None:
            stats[code] = val
    return stats


def _parse_standing(raw: dict[str, Any]) -> TeamStats:
    stats: dict[str, Any] = {}

    # Extract details (raw codes — Postgres normalizes)
    for detail in raw.get("details", []):
        type_info = detail.get("type")
        if not isinstance(type_info, dict):
            continue
        code = type_info.get("code", "")
        if not code:
            continue
        val = _extract_value(detail.get("value"))
        if val is not None:
            stats[code] = val

    if raw.get("points") is not None:
        stats["points"] = float(raw["points"])
    if raw.get("position") is not None:
        stats["position"] = float(raw["position"])
    if raw.get("form"):
        stats["form"] = raw["form"]

    # Parse participant for team data
    team = None
    participant = raw.get("participant")
    if isinstance(participant, dict) and participant.get("id"):
        team = _parse_team(participant)

    team_id = raw.get("participant_id", 0)
    if team and not team_id:
        team_id = team.id

    return TeamStats(
        team_id=team_id,
        team=team,
        stats=stats,
        raw=raw,
    )
