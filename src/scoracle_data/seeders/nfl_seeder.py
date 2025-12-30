"""
NFL-specific seeder for stats database.

Handles fetching and transforming NFL player and team statistics
from the API-Sports American Football API.

Uses a unified nfl_player_stats table - position-based display is
handled by widgets at query time, not at ingestion.

NFL API Endpoints Used:
- /players?id={id}&season={year}&team={team_id} - Player profile (requires team_id)
- /players/statistics?id={id}&season={year} - Player stats (no team_id needed)
- /teams?id={id} - Team profile
- /standings?league=1&season={year}&team={id} - Team stats
"""

from __future__ import annotations

import logging
import re
import time
from typing import Any, Optional

from .base import BaseSeeder
from .utils import DataParsers, NameBuilder, PositionMappers
from ..query_builder import (
    query_cache,
    NFL_PLAYER_STATS_COLUMNS,
    NFL_TEAM_STATS_COLUMNS,
)

logger = logging.getLogger(__name__)


class NFLSeeder(BaseSeeder):
    """Seeder for NFL statistics."""

    sport_id = "NFL"

    def _get_season_label(self, season_year: int) -> str:
        """NFL seasons are single year."""
        return str(season_year)

    # =========================================================================
    # Data Fetching
    # =========================================================================

    async def fetch_teams(
        self,
        season: int,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """Fetch NFL teams from API-Sports."""
        teams = await self.api.list_teams("NFL", season=str(season))

        result = []
        for team in teams:
            result.append({
                "id": team["id"],
                "name": team["name"],
                "abbreviation": team.get("abbreviation") or team.get("code"),
                "logo_url": team.get("logo_url") or team.get("logo"),
                "conference": team.get("conference"),
                "division": team.get("division"),
                "city": team.get("city"),
            })

        return result

    async def fetch_players(
        self,
        season: int,
        team_id: Optional[int] = None,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """Fetch NFL players from API-Sports.

        NFL API requires fetching players per team, not globally.
        Iterates through all teams and fetches players for each.

        The /players endpoint returns rich profile data including:
        - id, name, age, height, weight
        - college, group (Offense/Defense/Special Teams), position
        - number, salary, experience, image
        """
        all_players = []
        seen_ids = set()

        # Get all teams first
        teams = await self.fetch_teams(season)
        logger.info("Fetching players for %d NFL teams...", len(teams))

        for team_data in teams:
            tid = team_data["id"]

            try:
                # Fetch players for this team
                players = await self.api.list_players(
                    "NFL",
                    season=str(season),
                    team_id=tid,
                )

                for player in players:
                    player_id = player.get("id")
                    if not player_id or player_id in seen_ids:
                        continue

                    seen_ids.add(player_id)
                    position = player.get("position")

                    # API returns "group" (Offense/Defense/Special Teams) - use if position_group not inferred
                    api_group = player.get("group")
                    position_group = PositionMappers.get_nfl_position_group(position) or api_group

                    all_players.append({
                        "id": player_id,
                        "first_name": player.get("first_name") or player.get("firstname"),
                        "last_name": player.get("last_name") or player.get("lastname"),
                        "full_name": NameBuilder.build_full_name(player),
                        "position": position,
                        "position_group": position_group,
                        "nationality": player.get("nationality"),
                        "birth_date": player.get("birth_date"),
                        "height_inches": DataParsers.parse_height_to_inches(player.get("height")),
                        "weight_lbs": DataParsers.parse_weight_to_lbs(player.get("weight")),
                        "photo_url": player.get("photo_url") or player.get("image"),
                        "current_team_id": tid,
                        "jersey_number": player.get("number"),
                        "college": player.get("college"),
                        "experience_years": self._parse_experience(player.get("experience")),
                    })

                logger.debug("Fetched %d players from team %d", len(players), tid)

            except Exception as e:
                logger.warning("Failed to fetch players for team %d: %s", tid, e)

        logger.info("Total NFL players fetched: %d", len(all_players))
        return all_players

    async def fetch_player_stats(
        self,
        player_id: int,
        season: int,
    ) -> Optional[dict[str, Any]]:
        """Fetch player statistics from API-Sports."""
        try:
            stats = await self.api.get_player_statistics(
                str(player_id),
                "NFL",
                str(season),
            )
            return stats
        except Exception as e:
            logger.warning("Failed to fetch stats for player %d: %s", player_id, e)
            return None

    async def fetch_team_stats(
        self,
        team_id: int,
        season: int,
        league_id: Optional[int] = None,
    ) -> Optional[dict[str, Any]]:
        """Fetch team statistics from API-Sports (uses standings endpoint)."""
        try:
            if league_id is None:
                league_id = 1
            stats = await self.api.get_team_statistics(
                str(team_id),
                "NFL",
                str(season),
                league_id=league_id,
            )
            return stats
        except Exception as e:
            logger.warning("Failed to fetch stats for team %d: %s", team_id, e)
            return None

    async def fetch_team_profile(self, team_id: int) -> Optional[dict[str, Any]]:
        """Fetch detailed team profile from API-Sports."""
        try:
            response = await self.api.get_team_profile(str(team_id), "NFL")
            if not response:
                return None

            team = response
            country = team.get("country")
            if isinstance(country, dict):
                country = country.get("name") or country.get("code") or "USA"
            elif not country:
                country = "USA"

            venue = team.get("venue")
            if isinstance(venue, dict):
                venue_name = venue.get("name") or team.get("stadium")
                venue_city = venue.get("city") or team.get("city")
                venue_capacity = venue.get("capacity")
                venue_surface = venue.get("surface")
            else:
                venue_name = team.get("stadium")
                venue_city = team.get("city")
                venue_capacity = None
                venue_surface = None

            return {
                "id": team["id"],
                "name": team["name"],
                "abbreviation": team.get("abbreviation") or team.get("code"),
                "logo_url": team.get("logo_url") or team.get("logo"),
                "conference": team.get("conference"),
                "division": team.get("division"),
                "city": team.get("city"),
                "country": country,
                "founded": team.get("founded") or team.get("established"),
                "venue_name": venue_name,
                "venue_city": venue_city,
                "venue_capacity": venue_capacity,
                "venue_surface": venue_surface,
            }
        except Exception as e:
            logger.warning("Failed to fetch profile for team %d: %s", team_id, e)
            return None

    async def fetch_player_profile(self, player_id: int) -> Optional[dict[str, Any]]:
        """Fetch detailed player profile from API-Sports.

        NFL /players endpoint returns:
        - id, name, age
        - height (e.g., "6' 2\""), weight (e.g., "238 lbs")
        - college, group (Offense/Defense/Special Teams), position
        - number, salary, experience (years), image
        """
        try:
            response = await self.api.get_player_profile(str(player_id), "NFL")
            if not response:
                return None

            player = response
            team = player.get("team") or {}
            current_team_id = team.get("id") if isinstance(team, dict) else None
            position = player.get("position")

            # API returns "group" (Offense/Defense/Special Teams) - use if position_group not inferred
            api_group = player.get("group")
            position_group = PositionMappers.get_nfl_position_group(position) or api_group

            # Handle birth data (may be nested or flat depending on API version)
            birth = player.get("birth")
            if isinstance(birth, dict):
                birth_date = birth.get("date")
                birth_place = birth.get("place")
            else:
                birth_date = player.get("birth_date")
                birth_place = None

            return {
                "id": player["id"],
                "first_name": player.get("first_name") or player.get("firstname"),
                "last_name": player.get("last_name") or player.get("lastname"),
                "full_name": NameBuilder.build_full_name(player),
                "position": position,
                "position_group": position_group,
                "nationality": player.get("nationality") or player.get("country"),
                "birth_date": birth_date,
                "birth_place": birth_place,
                "height_inches": DataParsers.parse_height_to_inches(player.get("height")),
                "weight_lbs": DataParsers.parse_weight_to_lbs(player.get("weight")),
                "photo_url": player.get("photo_url") or player.get("image"),
                "current_team_id": current_team_id,
                "jersey_number": player.get("number"),
                "college": player.get("college"),
                "experience_years": self._parse_experience(player.get("experience")),
            }
        except Exception as e:
            logger.warning("Failed to fetch profile for player %d: %s", player_id, e)
            return None

    # =========================================================================
    # Data Transformation
    # =========================================================================

    def transform_player_stats(
        self,
        raw_stats: dict[str, Any],
        player_id: int,
        season_id: int,
        team_id: Optional[int] = None,
    ) -> dict[str, Any]:
        """
        Transform API stats to unified nfl_player_stats table schema.

        NFL API returns stats in teams[0].groups[] format. We flatten
        all stat groups into a single row. Widgets handle position-based display.
        """
        raw = raw_stats if isinstance(raw_stats, dict) else {}

        if "response" in raw and raw["response"]:
            raw = raw["response"][0] if isinstance(raw["response"], list) else raw["response"]

        # Convert NFL teams[0].groups[] format to flat structure
        stats = self._convert_nfl_stats_format(raw)

        # Extract stats from each group
        passing = stats.get("passing", {}) or {}
        rushing = stats.get("rushing", {}) or {}
        receiving = stats.get("receiving", {}) or {}
        defense = stats.get("defense", {}) or {}
        kicking = stats.get("kicking", {}) or {}
        punting = stats.get("punting", {}) or {}
        returns = stats.get("returns", {}) or {}
        games_data = stats.get("games", {}) or {}

        games = games_data.get("played", 0) or 0
        games_started = games_data.get("started", 0) or 0

        # Build unified stats dict - all columns, most will be 0/NULL for any given player
        return {
            "player_id": player_id,
            "season_id": season_id,
            "team_id": team_id,

            # Games
            "games_played": games,
            "games_started": games_started,

            # Passing
            "pass_attempts": self._safe_int(passing.get("attempts")),
            "pass_completions": self._safe_int(passing.get("completions")),
            "pass_yards": self._safe_int(passing.get("yards")),
            "pass_touchdowns": self._safe_int(passing.get("touchdowns")),
            "interceptions_thrown": self._safe_int(passing.get("interceptions")),
            "passer_rating": self._safe_float(passing.get("passer_rating")),
            "completion_pct": self._safe_float(passing.get("completion_pct")),
            "yards_per_attempt": self._safe_float(passing.get("yards_per_attempt")),
            "longest_pass": self._safe_int(passing.get("longest")),
            "sacks_taken": self._safe_int(passing.get("sacks")),
            "sack_yards_lost": self._safe_int(passing.get("sack_yards")),

            # Rushing
            "rush_attempts": self._safe_int(rushing.get("attempts")),
            "rush_yards": self._safe_int(rushing.get("yards")),
            "rush_touchdowns": self._safe_int(rushing.get("touchdowns")),
            "yards_per_carry": self._safe_float(rushing.get("yards_per_carry")),
            "longest_rush": self._safe_int(rushing.get("longest")),
            "rush_fumbles": self._safe_int(rushing.get("fumbles")),
            "rush_fumbles_lost": self._safe_int(rushing.get("fumbles_lost")),

            # Receiving
            "targets": self._safe_int(receiving.get("targets")),
            "receptions": self._safe_int(receiving.get("receptions")),
            "receiving_yards": self._safe_int(receiving.get("yards")),
            "receiving_touchdowns": self._safe_int(receiving.get("touchdowns")),
            "yards_per_reception": self._safe_float(receiving.get("yards_per_reception")),
            "longest_reception": self._safe_int(receiving.get("longest")),
            "yards_after_catch": self._safe_int(receiving.get("yac")),
            "rec_fumbles": self._safe_int(receiving.get("fumbles")),
            "rec_fumbles_lost": self._safe_int(receiving.get("fumbles_lost")),

            # Defense
            "tackles_total": self._safe_int(defense.get("tackles_total")),
            "tackles_solo": self._safe_int(defense.get("tackles_solo")),
            "tackles_assist": self._safe_int(defense.get("tackles_assist")),
            "tackles_for_loss": self._safe_int(defense.get("tackles_for_loss")),
            "sacks": self._safe_float(defense.get("sacks")),
            "sack_yards": self._safe_int(defense.get("sack_yards")),
            "qb_hits": self._safe_int(defense.get("qb_hits")),
            "def_interceptions": self._safe_int(defense.get("interceptions")),
            "int_yards": self._safe_int(defense.get("int_yards")),
            "int_touchdowns": self._safe_int(defense.get("int_tds")),
            "passes_defended": self._safe_int(defense.get("passes_defended")),
            "forced_fumbles": self._safe_int(defense.get("forced_fumbles")),
            "fumble_recoveries": self._safe_int(defense.get("fumble_recoveries")),

            # Kicking
            "fg_attempts": self._safe_int(kicking.get("fg_attempts")),
            "fg_made": self._safe_int(kicking.get("fg_made")),
            "fg_pct": self._safe_float(kicking.get("fg_pct")),
            "fg_long": self._safe_int(kicking.get("fg_long")),
            "xp_attempts": self._safe_int(kicking.get("xp_attempts")),
            "xp_made": self._safe_int(kicking.get("xp_made")),
            "xp_pct": self._safe_float(kicking.get("xp_pct")),
            "kicking_points": self._safe_int(kicking.get("points")),

            # Punting
            "punts": self._safe_int(punting.get("punts")),
            "punt_yards": self._safe_int(punting.get("yards")),
            "punt_avg": self._safe_float(punting.get("average")),
            "punt_long": self._safe_int(punting.get("longest")),
            "punts_inside_20": self._safe_int(punting.get("inside_20")),
            "touchbacks": self._safe_int(punting.get("touchbacks")),

            # Returns
            "kick_returns": self._safe_int(returns.get("kick_returns")),
            "kick_return_yards": self._safe_int(returns.get("kick_return_yards")),
            "kick_return_touchdowns": self._safe_int(returns.get("kick_return_tds")),
            "punt_returns": self._safe_int(returns.get("punt_returns")),
            "punt_return_yards": self._safe_int(returns.get("punt_return_yards")),
            "punt_return_touchdowns": self._safe_int(returns.get("punt_return_tds")),

            "updated_at": int(time.time()),
        }

    def _convert_nfl_stats_format(self, raw: dict[str, Any]) -> dict[str, Any]:
        """
        Convert NFL API teams[0].groups[] format to flat structure.

        Input: {"teams": [{"groups": [{"name": "Passing", "statistics": [...]}]}]}
        Output: {"passing": {"yards": 1000, ...}, "rushing": {...}, ...}
        """
        result: dict[str, Any] = {"games": {"played": 0, "started": 0}}

        teams = raw.get("teams", [])
        if not teams:
            return result

        team_data = teams[0] if teams else {}
        groups = team_data.get("groups", [])

        # Map group names to internal keys
        group_map = {
            "passing": "passing",
            "rushing": "rushing",
            "receiving": "receiving",
            "defense": "defense",
            "kicking": "kicking",
            "punting": "punting",
            "scoring": "scoring",
            "returns": "returns",
            "kick returns": "returns",
            "punt returns": "returns",
        }

        # Stat name mappings from API to our schema
        stat_key_map = {
            # Passing
            "passing_attempts": "attempts",
            "completions": "completions",
            "completion_pct": "completion_pct",
            "yards": "yards",
            "yards_per_pass_avg": "yards_per_attempt",
            "longest_pass": "longest",
            "passing_touchdowns": "touchdowns",
            "interceptions": "interceptions",
            "sacks": "sacks",
            "sacked_yards_lost": "sack_yards",
            "quaterback_rating": "passer_rating",
            "quarterback_rating": "passer_rating",
            # Rushing
            "rushing_attempts": "attempts",
            "yards_per_rush_avg": "yards_per_carry",
            "longest_rush": "longest",
            "rushing_touchdowns": "touchdowns",
            "fumbles": "fumbles",
            "fumbles_lost": "fumbles_lost",
            # Receiving
            "targets": "targets",
            "receptions": "receptions",
            "receiving_yards": "yards",
            "yards_per_reception": "yards_per_reception",
            "longest_reception": "longest",
            "receiving_touchdowns": "touchdowns",
            # Defense
            "unassisted_tackles": "tackles_solo",
            "assisted_tackles": "tackles_assist",
            "total_tackles": "tackles_total",
            "tackles_for_loss": "tackles_for_loss",
            "passes_defended": "passes_defended",
            "forced_fumbles": "forced_fumbles",
            "fumbles_recovered": "fumble_recoveries",
            "interception_yards": "int_yards",
            "interception_touchdowns": "int_tds",
        }

        for group in groups:
            group_name = (group.get("name") or "").lower()
            internal_key = group_map.get(group_name)

            if not internal_key:
                continue

            statistics = group.get("statistics", [])
            group_stats = result.get(internal_key, {})

            for stat in statistics:
                stat_name = (stat.get("name") or "").lower().replace(" ", "_")
                stat_value = stat.get("value")

                if stat_value is None:
                    continue

                # Convert string values to numbers
                if isinstance(stat_value, str):
                    stat_value = stat_value.replace(",", "")
                    try:
                        if "." in stat_value:
                            stat_value = float(stat_value)
                        else:
                            stat_value = int(stat_value)
                    except (ValueError, TypeError):
                        pass

                # Map stat name to our expected key
                mapped_key = stat_key_map.get(stat_name, stat_name)
                group_stats[mapped_key] = stat_value

            result[internal_key] = group_stats

        return result

    def transform_team_stats(
        self,
        raw_stats: dict[str, Any],
        team_id: int,
        season_id: int,
    ) -> dict[str, Any]:
        """Transform API team stats to database schema (from standings endpoint)."""
        stats = raw_stats if isinstance(raw_stats, dict) else {}

        if "response" in stats and stats["response"]:
            stats = stats["response"][0] if isinstance(stats["response"], list) else stats["response"]

        # Standings format: {won, lost, ties, points: {for, against}, ...}
        wins = stats.get("won", 0) or 0
        losses = stats.get("lost", 0) or 0
        ties = stats.get("ties", 0) or 0
        games_played = wins + losses + ties

        points = stats.get("points", {}) or {}
        points_for = points.get("for", 0) or 0
        points_against = points.get("against", 0) or 0

        return {
            "team_id": team_id,
            "season_id": season_id,
            "games_played": games_played,
            "wins": wins,
            "losses": losses,
            "ties": ties,
            "win_pct": DataParsers.safe_percentage(wins, games_played),
            "points_for": points_for,
            "points_against": points_against,
            "point_differential": points_for - points_against,
            "total_yards": 0,
            "yards_per_game": 0.0,
            "pass_yards": 0,
            "rush_yards": 0,
            "turnovers": 0,
            "yards_allowed": 0,
            "pass_yards_allowed": 0,
            "rush_yards_allowed": 0,
            "takeaways": 0,
            "updated_at": int(time.time()),
        }

    # =========================================================================
    # Database Operations
    # =========================================================================

    def upsert_player_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update NFL player statistics into unified table."""
        query = query_cache.get_or_build_upsert(
            table="nfl_player_stats",
            columns=NFL_PLAYER_STATS_COLUMNS,
            conflict_keys=["player_id", "season_id"],
        )
        self.db.execute(query, stats)

    def upsert_team_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update NFL team statistics."""
        query = query_cache.get_or_build_upsert(
            table="nfl_team_stats",
            columns=NFL_TEAM_STATS_COLUMNS,
            conflict_keys=["team_id", "season_id"],
        )
        self.db.execute(query, stats)

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _parse_experience(self, experience: Any) -> Optional[int]:
        """Parse experience to years.

        NFL API returns experience as integer years or string.
        """
        if experience is None:
            return None

        if isinstance(experience, int):
            return experience
        elif isinstance(experience, str):
            match = re.match(r"(\d+)", experience)
            if match:
                try:
                    return int(match.group(1))
                except (ValueError, TypeError):
                    pass

        return None
