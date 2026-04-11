"""Football data extraction from SportMonks API.

Thin handler: navigates deeply nested JSON, extracts raw key/value pairs.
Stat key normalization is handled by Postgres triggers.
"""

from __future__ import annotations

import logging
from typing import Any, Callable

from shared.models import (
    EventBoxScore,
    EventTeamStats,
    Player,
    PlayerStats,
    Team,
    TeamStats,
)
from shared.sportmonks_client import SportMonksClient

logger = logging.getLogger(__name__)

# Position ID to name mapping (SportMonks uses numeric IDs)
_POSITION_MAP = {24: "Goalkeeper", 25: "Defender", 26: "Midfielder", 27: "Attacker"}


class FootballHandler:
    """Fetches Football data from SportMonks and returns canonical models."""

    def __init__(self, api_token: str):
        self.client = SportMonksClient(api_token)

    def close(self) -> None:
        self.client.close()

    # ------------------------------------------------------------------
    # Teams
    # ------------------------------------------------------------------

    def get_teams(self, season_id: int) -> list[Team]:
        items = self.client.get_all_pages(
            f"/teams/seasons/{season_id}",
            {"include": "venue;country"},
        )
        return [_parse_team(t) for t in items]

    def get_team_squad(self, season_id: int, team_id: int) -> list[dict]:
        """Fetch team squad with jersey numbers.

        Returns list of player entries with:
        - player_id
        - jersey_number
        - position_id
        """
        resp = self.client.get(f"/squads/seasons/{season_id}/teams/{team_id}")
        return resp.get("data", [])

    def get_player_profile(self, player_id: int) -> dict | None:
        """Fetch detailed player profile including photo, bio, nationality, etc.

        Args:
            player_id: SportMonks player ID

        Returns:
            Player profile dict or None if not found
        """
        try:
            resp = self.client.get(
                f"/players/{player_id}",
                {"include": "nationality;detailedPosition;position;metadata"},
            )
            return resp.get("data")
        except Exception as e:
            logger.warning(f"Failed to fetch player profile {player_id}: {e}")
            return None

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

    # ------------------------------------------------------------------
    # Fixture schedule + fixture box scores
    # ------------------------------------------------------------------

    def get_fixtures(self, season_id: int) -> list[dict[str, Any]]:
        """Fetch season fixtures with participant teams."""
        items = self.client.get_all_pages(
            "/fixtures",
            {
                "filters": f"fixtureSeasons:{season_id}",
                "include": "participants",
            },
        )
        fixtures: list[dict[str, Any]] = []
        for raw in items:
            fixture = _parse_fixture_stub(raw)
            if fixture:
                fixtures.append(fixture)
        return fixtures

    def get_box_score(
        self, external_fixture_id: int, fixture_id: int
    ) -> tuple[list[EventBoxScore], list[EventTeamStats]]:
        """Fetch one fixture's detailed lineups + scores as event-level lines."""
        resp = self.client.get(
            f"/fixtures/{external_fixture_id}",
            {
                "include": "lineups.details.type;events;scores;participants",
            },
        )
        data = resp.get("data", {})

        players: list[EventBoxScore] = []
        team_stats_acc: dict[int, dict[str, float]] = {}
        team_scores: dict[int, int] = _extract_fixture_scores(data)

        lineups = data.get("lineups", [])
        if isinstance(lineups, list):
            for entry in lineups:
                team_id = entry.get("team_id") or _team_id_from_relation(
                    entry.get("team")
                )
                player_raw = entry.get("player")
                player_id = entry.get("player_id")
                if not isinstance(player_id, int) and isinstance(player_raw, dict):
                    player_id = player_raw.get("id")
                if not isinstance(team_id, int) or not isinstance(player_id, int):
                    continue

                details = entry.get("details", [])
                stats = _normalize_player_stats(
                    details if isinstance(details, list) else []
                )
                minutes_played = _extract_value(entry.get("minutes"))
                if minutes_played is None:
                    minutes_played = stats.get("minutes-played") or stats.get("minutes_played")
                if minutes_played is not None:
                    stats["minutes_played"] = minutes_played

                player = (
                    _parse_player(player_raw) if isinstance(player_raw, dict) else None
                )
                if player and player.team_id is None:
                    player.team_id = team_id

                players.append(
                    EventBoxScore(
                        fixture_id=fixture_id,
                        player_id=player_id,
                        team_id=team_id,
                        player=player,
                        minutes_played=minutes_played,
                        stats=stats,
                        raw=entry,
                    )
                )

                acc = team_stats_acc.setdefault(team_id, {})
                for key, value in stats.items():
                    if isinstance(value, (int, float)):
                        acc[key] = acc.get(key, 0.0) + float(value)

        teams: list[EventTeamStats] = []
        participants = data.get("participants", [])
        if isinstance(participants, list):
            for p in participants:
                team_id = p.get("id")
                if isinstance(team_id, int):
                    team_stats_acc.setdefault(team_id, {})

        for team_id, agg in team_stats_acc.items():
            teams.append(
                EventTeamStats(
                    fixture_id=fixture_id,
                    team_id=team_id,
                    score=team_scores.get(team_id),
                    stats=agg,
                    raw={
                        "provider": "sportmonks",
                        "external_fixture_id": external_fixture_id,
                    },
                )
            )

        return players, teams


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
        val = _extract_value(detail.get("data", {}).get("value"))
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
        val = _extract_value(detail.get("data", {}).get("value"))
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


def _team_id_from_relation(raw: Any) -> int | None:
    if isinstance(raw, dict):
        team_id = raw.get("id")
        if isinstance(team_id, int):
            return team_id
    return None


def _extract_fixture_scores(raw: dict[str, Any]) -> dict[int, int]:
    out: dict[int, int] = {}
    for score_block in (
        raw.get("scores", []) if isinstance(raw.get("scores"), list) else []
    ):
        participant_id = score_block.get("participant_id")
        if not isinstance(participant_id, int):
            continue
        score_val = _extract_value(score_block.get("score"))
        if score_val is None:
            score_val = _extract_value(score_block.get("goals"))
        if score_val is None:
            continue
        out[participant_id] = int(score_val)

    # Fallback: if no scores include participant IDs, map home/away values.
    if not out:
        home_team_id = raw.get("localteam_id")
        away_team_id = raw.get("visitorteam_id")
        if isinstance(home_team_id, int):
            home_val = _extract_value(raw.get("scores_home") or raw.get("home_score"))
            if home_val is not None:
                out[home_team_id] = int(home_val)
        if isinstance(away_team_id, int):
            away_val = _extract_value(raw.get("scores_away") or raw.get("away_score"))
            if away_val is not None:
                out[away_team_id] = int(away_val)

    return out


def _parse_fixture_stub(raw: dict[str, Any]) -> dict[str, Any] | None:
    external_id = raw.get("id")
    if not isinstance(external_id, int):
        return None

    participants = raw.get("participants", [])
    home_participant: dict[str, Any] | None = None
    away_participant: dict[str, Any] | None = None
    home_team_id: int | None = None
    away_team_id: int | None = None
    if isinstance(participants, list):
        for p in participants:
            if not isinstance(p, dict):
                continue
            pid = p.get("id")
            if not isinstance(pid, int):
                continue
            loc = (p.get("meta") or {}).get("location")
            if isinstance(loc, str):
                loc_l = loc.lower()
                if loc_l == "home":
                    home_team_id = pid
                    home_participant = p
                elif loc_l == "away":
                    away_team_id = pid
                    away_participant = p

    if home_team_id is None:
        home_team_id = raw.get("localteam_id")
    if away_team_id is None:
        away_team_id = raw.get("visitorteam_id")
    if home_team_id is None:
        home_team_id = raw.get("home_team_id")
    if away_team_id is None:
        away_team_id = raw.get("away_team_id")

    if not isinstance(home_team_id, int) or not isinstance(away_team_id, int):
        return None

    start_time = (
        raw.get("starting_at")
        or raw.get("starting_at_timestamp")
        or raw.get("starting_at_utc")
        or raw.get("starting_at_iso")
        or raw.get("time")
        or raw.get("date")
    )
    if not isinstance(start_time, str):
        return None

    season = None
    season_raw = raw.get("season")
    if isinstance(season_raw, dict):
        season_name = season_raw.get("name")
        if isinstance(season_name, str):
            try:
                season = int(season_name.split("/")[0])
            except Exception:
                season = None

    round_name = None
    round_raw = raw.get("round")
    if isinstance(round_raw, dict):
        round_name = round_raw.get("name")
    if not isinstance(round_name, str):
        round_name = raw.get("stage_name")

    return {
        "external_id": external_id,
        "home_team_id": home_team_id,
        "away_team_id": away_team_id,
        "home_team": _parse_team(home_participant) if home_participant else None,
        "away_team": _parse_team(away_participant) if away_participant else None,
        "start_time": start_time,
        "season": season,
        "round": round_name,
        "raw": raw,
    }
