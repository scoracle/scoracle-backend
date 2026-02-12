"""
SportMonks handler — fetches Football (Soccer) data and normalizes to canonical format.

Extends BaseApiClient for HTTP infrastructure. All SportMonks-specific
parsing (type codes, nested structures, metric-to-imperial conversion)
lives here. Seeders receive clean canonical dicts.

Key design decisions:
- Player stats use type.code (human-readable) instead of type_id (magic int)
- CODE_OVERRIDES maps the ~12 SportMonks codes that don't match our canonical
  key names after a simple hyphen-to-underscore replacement
- Values like {"total": 15, "goals": 12} are flattened via extract_value()
"""

from __future__ import annotations

import logging
from datetime import datetime
from typing import Any, AsyncIterator

from ..core.http import BaseApiClient
from . import extract_value

logger = logging.getLogger(__name__)

# SportMonks type codes that don't match our canonical stat keys after
# simple replace("-", "_"). Only mismatches are listed; everything else
# flows through as code.replace("-", "_").
PLAYER_CODE_OVERRIDES: dict[str, str] = {
    "passes": "passes_total",
    "accurate-passes": "passes_accurate",
    "total-crosses": "crosses_total",
    "accurate-crosses": "crosses_accurate",
    "blocked-shots": "blocks",
    "total-duels": "duels_total",
    "dribble-attempts": "dribbles_attempts",
    "successful-dribbles": "dribbles_success",
    "yellowcards": "yellow_cards",
    "redcards": "red_cards",
    "fouls": "fouls_committed",
    "expected-goals": "expected_goals",
}

STANDING_CODE_OVERRIDES: dict[str, str] = {
    "overall-matches-played": "matches_played",
    "overall-won": "wins",
    "overall-draw": "draws",
    "overall-lost": "losses",
    "overall-goals-for": "goals_for",
    "overall-goals-against": "goals_against",
    "home-matches-played": "home_played",
    "away-matches-played": "away_played",
}


def _normalize_code(code: str, overrides: dict[str, str]) -> str:
    """Map a SportMonks type code to our canonical stat key."""
    return overrides.get(code, code.replace("-", "_"))


class SportMonksHandler(BaseApiClient):
    """Fetches Football data from SportMonks and normalizes to canonical format."""

    BASE_URL = "https://api.sportmonks.com/v3/football"

    def __init__(self, api_token: str, requests_per_minute: int = 300):
        super().__init__(
            params={"api_token": api_token},
            requests_per_minute=requests_per_minute,
        )

    # =========================================================================
    # Pagination helper (SportMonks uses page-based, not cursor)
    # =========================================================================

    async def _get_paginated(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        per_page: int = 50,
    ) -> list[dict[str, Any]]:
        """Fetch all pages from a paginated endpoint."""
        if params is None:
            params = {}
        params["per_page"] = per_page
        all_data: list[dict[str, Any]] = []
        page = 1
        while True:
            params["page"] = page
            response = await self._get(path, params)
            data = response.get("data", [])
            all_data.extend(data)
            pagination = response.get("pagination", {})
            if not pagination.get("has_more", False):
                break
            page += 1
        return all_data

    # =========================================================================
    # Seasons
    # =========================================================================

    async def get_league_seasons(self, league_id: int) -> list[dict[str, Any]]:
        """Get all seasons for a league, sorted newest first."""
        response = await self._get(f"/leagues/{league_id}", {"include": "seasons"})
        data = response.get("data", {})
        seasons = data.get("seasons", [])
        return sorted(seasons, key=lambda s: s.get("name", ""), reverse=True)

    async def discover_season_ids(
        self, league_id: int, target_years: list[int]
    ) -> dict[int, int]:
        """Discover SportMonks season IDs for specific years.

        Returns dict mapping year -> SportMonks season_id.
        """
        seasons = await self.get_league_seasons(league_id)
        result: dict[int, int] = {}
        for season in seasons:
            season_id = season.get("id")
            name = str(season.get("name", ""))
            if not season_id or not name:
                continue
            start_year_str = name.split("/")[0].strip()
            try:
                start_year = int(start_year_str)
            except ValueError:
                continue
            if start_year in target_years and start_year not in result:
                result[start_year] = season_id
        return result

    # =========================================================================
    # Teams
    # =========================================================================

    async def get_teams(self, season_id: int) -> list[dict[str, Any]]:
        """Return all teams for a season in canonical format."""
        raw = await self._get_paginated(
            f"/teams/seasons/{season_id}",
            {"include": "venue;country"},
        )
        return [self._normalize_team(t) for t in raw]

    def _normalize_team(self, raw: dict[str, Any]) -> dict[str, Any]:
        venue = raw.get("venue") or {}
        country = raw.get("country") or {}
        country_name = country.get("name") if isinstance(country, dict) else None
        meta: dict[str, Any] = {}
        if venue.get("city"):
            meta["venue_city"] = venue["city"]
        if venue.get("surface"):
            meta["venue_surface"] = venue["surface"]
        return {
            "id": raw["id"],
            "name": raw.get("name"),
            "short_code": raw.get("short_code"),
            "country": country_name,
            "logo_url": raw.get("image_path"),
            "venue_name": venue.get("name"),
            "venue_capacity": venue.get("capacity"),
            "founded": raw.get("founded"),
            "meta": meta,
        }

    # =========================================================================
    # Players + Stats (fetched together via squad iteration)
    # =========================================================================

    async def get_players_with_stats(
        self,
        season_id: int,
        team_ids: list[int],
        sportmonks_league_id: int,
    ) -> AsyncIterator[dict[str, Any]]:
        """Iterate through squads, fetch per-player stats, yield canonical dicts.

        Each yielded dict has:
            "player_id": int
            "player": canonical player dict
            "stats": canonical flat stats dict (or empty if no stats for this league)
            "raw": full API response
        """
        for i, team_id in enumerate(team_ids):
            logger.info(
                "Fetching squad for team %d (%d/%d)...",
                team_id, i + 1, len(team_ids),
            )
            try:
                squad_response = await self._get(
                    f"/squads/seasons/{season_id}/teams/{team_id}"
                )
                squad = squad_response.get("data", [])
            except Exception as e:
                logger.warning("Failed to fetch squad for team %d: %s", team_id, e)
                continue

            player_ids = [
                entry.get("player_id") or entry.get("id")
                for entry in squad
            ]
            player_ids = [pid for pid in player_ids if pid]

            for j, player_id in enumerate(player_ids):
                try:
                    response = await self._get(
                        f"/players/{player_id}",
                        {
                            "include": "statistics.details.type;statistics.season.league;"
                                       "nationality;detailedPosition",
                            "filters": f"playerStatisticSeasons:{season_id}",
                        },
                    )
                    player_data = response.get("data", {})
                    if not player_data:
                        continue

                    # Extract stats for the target league from statistics blocks
                    stats = self._extract_league_stats(
                        player_data.get("statistics", []),
                        sportmonks_league_id,
                    )

                    yield {
                        "player_id": player_data["id"],
                        "team_id": team_id,
                        "player": self._normalize_player(player_data),
                        "stats": stats,
                        "raw": player_data,
                    }

                    if (j + 1) % 10 == 0:
                        logger.info(
                            "  Fetched %d/%d players for team %d...",
                            j + 1, len(player_ids), team_id,
                        )
                except Exception as e:
                    logger.warning("Failed to fetch player %d: %s", player_id, e)

    def _extract_league_stats(
        self, statistics: list[dict[str, Any]], sportmonks_league_id: int
    ) -> dict[str, Any]:
        """Extract stats for a specific league from a player's statistics array.

        Each statistics block has a season (with league info) and details array.
        We find the block matching our league and normalize its details.
        """
        for stat_block in statistics:
            stat_season = stat_block.get("season") or {}
            stat_league = stat_season.get("league") or {}
            if stat_league.get("id") == sportmonks_league_id:
                return self._normalize_player_stats(stat_block.get("details", []))
        return {}

    def _normalize_player_stats(self, details: list[dict[str, Any]]) -> dict[str, Any]:
        """Convert SportMonks details array to canonical flat stats dict.

        Uses type.code (from include=details.type) instead of magic type_id integers.
        """
        stats: dict[str, Any] = {}
        for detail in details:
            type_info = detail.get("type") or {}
            code = type_info.get("code", "")
            if not code:
                continue
            key = _normalize_code(code, PLAYER_CODE_OVERRIDES)
            val = extract_value(detail.get("value"))
            if val is not None:
                stats[key] = val
        return stats

    # =========================================================================
    # Team Stats (Standings)
    # =========================================================================

    async def get_team_stats(self, season_id: int) -> list[dict[str, Any]]:
        """Return standings for a season in canonical format."""
        response = await self._get(
            f"/standings/seasons/{season_id}",
            {"include": "participant;details.type"},
        )
        raw_standings = response.get("data", [])
        return [self._normalize_standing(s) for s in raw_standings]

    def _normalize_standing(self, standing: dict[str, Any]) -> dict[str, Any]:
        """Convert a SportMonks standing entry to canonical team stats dict."""
        participant = standing.get("participant") or {}
        team_id = participant.get("id") or standing.get("participant_id")

        stats: dict[str, Any] = {}

        # Extract details using type.code
        for detail in standing.get("details", []):
            type_info = detail.get("type") or {}
            code = type_info.get("code", "")
            if not code:
                continue
            key = _normalize_code(code, STANDING_CODE_OVERRIDES)
            val = extract_value(detail.get("value"))
            if val is not None:
                stats[key] = val

        # Add top-level fields
        if standing.get("points") is not None:
            stats["points"] = standing["points"]
        if standing.get("position") is not None:
            stats["position"] = standing["position"]
        if standing.get("form"):
            stats["form"] = standing["form"]

        return {
            "team_id": team_id,
            "team": self._normalize_team(participant) if participant.get("id") else None,
            "stats": stats,
            "raw": standing,
        }

    # =========================================================================
    # Player normalization
    # =========================================================================

    def _normalize_player(self, raw: dict[str, Any]) -> dict[str, Any]:
        """Convert a SportMonks player response to canonical format."""
        # Date of birth
        dob = None
        dob_str = raw.get("date_of_birth")
        if dob_str:
            try:
                dob = datetime.strptime(dob_str, "%Y-%m-%d").date()
            except ValueError:
                pass

        # Nationality
        nationality_data = raw.get("nationality")
        if isinstance(nationality_data, dict):
            nationality_name = nationality_data.get("name")
        else:
            nationality_name = nationality_data

        # Detailed position
        detailed_pos_data = raw.get("detailedposition") or raw.get("detailedPosition")
        detailed_position = None
        if isinstance(detailed_pos_data, dict):
            detailed_position = detailed_pos_data.get("name")
        elif isinstance(detailed_pos_data, str):
            detailed_position = detailed_pos_data

        # General position from position_id
        position = raw.get("position")
        pos_id = raw.get("position_id")
        if not position and pos_id is not None:
            pos_map = {24: "Goalkeeper", 25: "Defender", 26: "Midfielder", 27: "Attacker"}
            position = pos_map.get(int(pos_id))

        # Name: prefer common_name, fall back to first+last
        name = (
            raw.get("common_name")
            or raw.get("display_name")
            or f"{raw.get('firstname', '')} {raw.get('lastname', '')}".strip()
            or f"Player {raw.get('id', '?')}"
        )

        # Metric -> imperial conversion
        height = self._cm_to_feet_inches(raw.get("height"))
        weight = self._kg_to_lbs(raw.get("weight"))

        # Meta
        meta: dict[str, Any] = {}
        for key in ("nationality_id", "detailed_position_id", "position_id"):
            val = raw.get(key)
            if val is not None:
                meta[key] = val
        if raw.get("display_name"):
            meta["display_name"] = raw["display_name"]
        if raw.get("common_name"):
            meta["common_name"] = raw["common_name"]

        return {
            "id": raw.get("id"),
            "name": name,
            "first_name": raw.get("firstname"),
            "last_name": raw.get("lastname"),
            "position": position,
            "detailed_position": detailed_position,
            "nationality": nationality_name,
            "height": height,
            "weight": weight,
            "date_of_birth": dob,
            "photo_url": raw.get("image_path"),
            "meta": meta,
        }

    # =========================================================================
    # Unit conversion (SportMonks provides metric)
    # =========================================================================

    @staticmethod
    def _cm_to_feet_inches(cm: Any) -> str | None:
        """Convert height in cm to feet-inches string (e.g. 185 -> '6-1')."""
        if cm is None:
            return None
        try:
            total_inches = float(cm) / 2.54
        except (ValueError, TypeError):
            return None
        if total_inches <= 0:
            return None
        feet = int(total_inches // 12)
        inches = int(round(total_inches % 12))
        if inches == 12:
            feet += 1
            inches = 0
        return f"{feet}-{inches}"

    @staticmethod
    def _kg_to_lbs(kg: Any) -> str | None:
        """Convert weight in kg to lbs string (e.g. 80 -> '176')."""
        if kg is None:
            return None
        try:
            val = float(kg)
        except (ValueError, TypeError):
            return None
        if val <= 0:
            return None
        return str(round(val * 2.20462))
