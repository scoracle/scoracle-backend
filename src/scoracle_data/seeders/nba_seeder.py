"""
NBA-specific seeder for stats database.

Handles fetching and transforming NBA player and team statistics
from the API-Sports NBA API.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Optional

from .base import BaseSeeder
from .utils import DataParsers, NameBuilder, StatCalculators, PositionMappers
from ..query_builder import (
    query_cache,
    NBA_PLAYER_STATS_COLUMNS,
    NBA_TEAM_STATS_COLUMNS,
)

logger = logging.getLogger(__name__)


class NBASeeder(BaseSeeder):
    """Seeder for NBA statistics."""

    sport_id = "NBA"

    def _get_season_label(self, season_year: int) -> str:
        """NBA seasons span two years (e.g., 2024-25)."""
        next_year = (season_year + 1) % 100
        return f"{season_year}-{next_year:02d}"

    # =========================================================================
    # Data Fetching
    # =========================================================================

    async def fetch_teams(
        self,
        season: int,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """Fetch NBA teams from API-Sports.

        Note: NBA doesn't use league_id, parameter included for interface compliance.
        """
        teams = await self.api.list_teams("NBA")

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
        """Fetch NBA players from API-Sports.

        Note: NBA doesn't use league_id, parameter included for interface compliance.
        Uses /players endpoint with team parameter to get players per team.
        """
        all_players = []
        seen_ids = set()

        # Get all teams first
        teams = await self.fetch_teams(season)
        logger.info("Fetching players for %d NBA teams...", len(teams))

        for team_data in teams:
            tid = team_data["id"]

            try:
                # Use /players endpoint with team and season params
                players = await self.api.list_players(
                    "NBA",
                    season=str(season),
                    team_id=tid,
                )

                for player in players:
                    player_id = player.get("id")
                    if not player_id or player_id in seen_ids:
                        continue

                    seen_ids.add(player_id)

                    all_players.append({
                        "id": player_id,
                        "first_name": player.get("first_name") or player.get("firstname"),
                        "last_name": player.get("last_name") or player.get("lastname"),
                        "full_name": player.get("name") or NameBuilder.build_full_name(player),
                        "position": player.get("position"),
                        "position_group": PositionMappers.get_nba_position_group(player.get("position")),
                        "nationality": player.get("nationality") or player.get("birth_country"),
                        "birth_date": player.get("birth_date"),
                        "height_inches": DataParsers.parse_height_to_inches(player.get("height")),
                        "weight_lbs": DataParsers.parse_weight_to_lbs(player.get("weight")),
                        "photo_url": player.get("photo_url"),
                        "current_team_id": tid,
                        "jersey_number": player.get("jersey"),
                        "college": player.get("college"),
                    })

                logger.debug("Fetched %d players from team %d", len(players), tid)

            except Exception as e:
                logger.warning("Failed to fetch players for team %d: %s", tid, e)

        logger.info("Total NBA players fetched: %d", len(all_players))
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
                "NBA",
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
    ) -> Optional[dict[str, Any]]:
        """Fetch team statistics from API-Sports."""
        try:
            stats = await self.api.get_team_statistics(
                str(team_id),
                "NBA",
                str(season),
            )
            return stats
        except Exception as e:
            logger.warning("Failed to fetch stats for team %d: %s", team_id, e)
            return None

    async def fetch_team_profile(self, team_id: int) -> Optional[dict[str, Any]]:
        """Fetch detailed team profile from API-Sports.

        Returns extended team info including venue details.
        API: GET /teams?id={team_id}
        """
        try:
            response = await self.api.get_team_profile(str(team_id), "NBA")

            if not response:
                return None

            team = response

            # Handle venue - may be dict or string
            venue = team.get("venue")
            if isinstance(venue, dict):
                venue_name = venue.get("name") or team.get("arena")
                venue_city = venue.get("city") or team.get("city")
                venue_capacity = venue.get("capacity")
            else:
                venue_name = team.get("arena")
                venue_city = team.get("city")
                venue_capacity = None

            # Handle leagues nested structure
            leagues = team.get("leagues")
            conference = team.get("conference")
            division = team.get("division")
            if isinstance(leagues, dict):
                standard = leagues.get("standard", {})
                if isinstance(standard, dict):
                    conference = conference or standard.get("conference")
                    division = division or standard.get("division")

            return {
                "id": team["id"],
                "name": team["name"],
                "abbreviation": team.get("abbreviation") or team.get("code") or team.get("nickname"),
                "logo_url": team.get("logo_url") or team.get("logo"),
                "conference": conference,
                "division": division,
                "city": team.get("city"),
                "founded": team.get("founded"),
                "venue_name": venue_name,
                "venue_city": venue_city,
                "venue_capacity": venue_capacity,
            }
        except Exception as e:
            logger.warning("Failed to fetch profile for team %d: %s", team_id, e)
            return None

    async def fetch_player_profile(self, player_id: int) -> Optional[dict[str, Any]]:
        """Fetch detailed player profile from API-Sports.

        Returns extended player info including full biographical data.
        API: GET /players?id={player_id}

        NBA player response structure:
        - id, firstname, lastname
        - birth: {date, country}
        - nba: {start, pro}
        - height: {feets, inches, meters}
        - weight: {pounds, kilograms}
        - college, affiliation
        - leagues: {standard: {jersey, active, pos}}
        """
        try:
            response = await self.api.get_player_profile(str(player_id), "NBA")

            if not response:
                return None

            player = response

            # Extract team from nested structure
            team = player.get("team") or {}
            current_team_id = team.get("id") if isinstance(team, dict) else None

            # NBA API returns position in leagues.standard.pos
            leagues = player.get("leagues") or {}
            standard = leagues.get("standard", {}) if isinstance(leagues, dict) else {}
            position = standard.get("pos") if isinstance(standard, dict) else None

            # Birth info
            birth = player.get("birth", {}) or {}

            return {
                "id": player["id"],
                "first_name": player.get("first_name") or player.get("firstname"),
                "last_name": player.get("last_name") or player.get("lastname"),
                "full_name": NameBuilder.build_full_name(player),
                "position": position,
                "position_group": PositionMappers.get_nba_position_group(position),
                "nationality": player.get("nationality") or birth.get("country"),
                "birth_date": player.get("birth_date") or birth.get("date"),
                "birth_place": birth.get("place") if isinstance(birth, dict) else None,
                "height_inches": DataParsers.parse_height_to_inches(player.get("height")),
                "weight_lbs": DataParsers.parse_weight_to_lbs(player.get("weight")),
                "photo_url": player.get("photo_url"),
                "current_team_id": current_team_id,
                "jersey_number": player.get("jersey") or (standard.get("jersey") if isinstance(standard, dict) else None),
                "college": player.get("college"),
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
        """Transform API stats to database schema."""
        # API-Sports NBA stats structure varies, handle common patterns
        stats = raw_stats if isinstance(raw_stats, dict) else {}

        # Try to extract from nested structure
        if "response" in stats and stats["response"]:
            stats = stats["response"][0] if isinstance(stats["response"], list) else stats["response"]

        # Extract games/minutes
        games = stats.get("games", {}) if isinstance(stats.get("games"), dict) else {}
        games_played = games.get("played") or stats.get("games_played", 0)
        games_started = games.get("started") or stats.get("games_started", 0)
        minutes = stats.get("min") or stats.get("minutes", 0)

        # Points
        points = stats.get("points", {}) if isinstance(stats.get("points"), dict) else {}
        points_total = points.get("total") or stats.get("points_total", 0) or 0

        # Field goals
        fg = stats.get("fgm", {}) if isinstance(stats.get("fgm"), dict) else {}
        fgm = fg.get("made") or stats.get("fgm", 0) or 0
        fga = fg.get("attempted") or stats.get("fga", 0) or 0
        fg_pct = DataParsers.safe_percentage(fgm, fga) or stats.get("fgp", 0) or 0

        # Three pointers
        tp = stats.get("tpm", {}) if isinstance(stats.get("tpm"), dict) else {}
        tpm = tp.get("made") or stats.get("tpm", 0) or 0
        tpa = tp.get("attempted") or stats.get("tpa", 0) or 0
        tp_pct = DataParsers.safe_percentage(tpm, tpa) or stats.get("tpp", 0) or 0

        # Free throws
        ft = stats.get("ftm", {}) if isinstance(stats.get("ftm"), dict) else {}
        ftm = ft.get("made") or stats.get("ftm", 0) or 0
        fta = ft.get("attempted") or stats.get("fta", 0) or 0
        ft_pct = DataParsers.safe_percentage(ftm, fta) or stats.get("ftp", 0) or 0

        # Rebounds
        rebounds = stats.get("rebounds", {}) if isinstance(stats.get("rebounds"), dict) else {}
        off_reb = rebounds.get("offensive") or stats.get("offReb", 0) or 0
        def_reb = rebounds.get("defensive") or stats.get("defReb", 0) or 0
        tot_reb = rebounds.get("total") or stats.get("totReb", 0) or off_reb + def_reb

        # Other stats
        assists = stats.get("assists") or stats.get("assists", 0) or 0
        turnovers = stats.get("turnovers") or stats.get("turnovers", 0) or 0
        steals = stats.get("steals") or stats.get("steals", 0) or 0
        blocks = stats.get("blocks") or stats.get("blocks", 0) or 0
        fouls = stats.get("pFouls") or stats.get("personal_fouls", 0) or 0
        pm_raw = stats.get("plusMinus") or stats.get("plus_minus", 0) or 0
        plus_minus = int(pm_raw) if isinstance(pm_raw, (int, float, str)) and str(pm_raw).lstrip("+-").isdigit() else 0

        # Calculate per-game averages
        gp = max(games_played, 1)  # Avoid division by zero

        return {
            "player_id": player_id,
            "season_id": season_id,
            "team_id": team_id,
            "games_played": games_played,
            "games_started": games_started,
            "minutes_total": self._parse_minutes_total(minutes),
            "minutes_per_game": self._parse_minutes_total(minutes) / gp if isinstance(minutes, (int, float)) else 0,
            "points_total": points_total,
            "points_per_game": round(points_total / gp, 1) if points_total else 0,
            "fgm": fgm,
            "fga": fga,
            "fg_pct": fg_pct,
            "tpm": tpm,
            "tpa": tpa,
            "tp_pct": tp_pct,
            "ftm": ftm,
            "fta": fta,
            "ft_pct": ft_pct,
            "offensive_rebounds": off_reb,
            "defensive_rebounds": def_reb,
            "total_rebounds": tot_reb,
            "rebounds_per_game": round(tot_reb / gp, 1) if tot_reb else 0,
            "assists": assists,
            "assists_per_game": round(assists / gp, 1) if assists else 0,
            "turnovers": turnovers,
            "turnovers_per_game": round(turnovers / gp, 1) if turnovers else 0,
            "steals": steals,
            "steals_per_game": round(steals / gp, 1) if steals else 0,
            "blocks": blocks,
            "blocks_per_game": round(blocks / gp, 1) if blocks else 0,
            "personal_fouls": fouls,
            "fouls_per_game": round(fouls / gp, 1) if fouls else 0,
            "plus_minus": plus_minus,
            "plus_minus_per_game": round(plus_minus / gp, 1) if plus_minus else 0,
            "efficiency": StatCalculators.calculate_nba_efficiency(
                points_total, tot_reb, assists, steals, blocks, fgm, fga, ftm, fta, turnovers, gp
            ),
            "true_shooting_pct": StatCalculators.calculate_true_shooting_pct(points_total, fga, fta),
            "effective_fg_pct": StatCalculators.calculate_effective_fg_pct(fgm, tpm, fga),
            "assist_turnover_ratio": round(assists / max(turnovers, 1), 2) if assists else 0,
            "updated_at": int(time.time()),
        }

    def transform_team_stats(
        self,
        raw_stats: dict[str, Any],
        team_id: int,
        season_id: int,
    ) -> dict[str, Any]:
        """Transform API team stats to database schema."""
        stats = raw_stats if isinstance(raw_stats, dict) else {}

        if "response" in stats and stats["response"]:
            stats = stats["response"][0] if isinstance(stats["response"], list) else stats["response"]

        # Defensive helper for nested dict access
        def safe_nested_get(d, *keys, default=0):
            """Safely traverse nested dicts, returning default if any key fails."""
            for key in keys:
                if isinstance(d, dict):
                    d = d.get(key, {})
                else:
                    return default
            return d if d else default

        # Extract record - handle both nested and flat structures
        games = stats.get("games", {}) if isinstance(stats.get("games"), dict) else {}
        wins = games.get("wins", {}) if isinstance(games.get("wins"), dict) else {}
        losses = games.get("losses", {}) if isinstance(games.get("losses"), dict) else {}

        # Try nested structure first, fall back to flat
        total_wins = safe_nested_get(wins, "all", "total") or stats.get("wins", 0) or 0
        total_losses = safe_nested_get(losses, "all", "total") or stats.get("losses", 0) or 0
        games_played = total_wins + total_losses

        # Points - safely handle different structures
        points_for_ppg = safe_nested_get(stats, "points", "for", "average", "all")
        points_against_ppg = safe_nested_get(stats, "points", "against", "average", "all")

        return {
            "team_id": team_id,
            "season_id": season_id,
            "games_played": games_played,
            "wins": total_wins,
            "losses": total_losses,
            "win_pct": DataParsers.safe_percentage(total_wins, games_played),
            "home_wins": safe_nested_get(wins, "home", "total"),
            "home_losses": safe_nested_get(losses, "home", "total"),
            "away_wins": safe_nested_get(wins, "away", "total"),
            "away_losses": safe_nested_get(losses, "away", "total"),
            "points_per_game": points_for_ppg if isinstance(points_for_ppg, (int, float)) else 0,
            "opponent_ppg": points_against_ppg if isinstance(points_against_ppg, (int, float)) else 0,
            "updated_at": int(time.time()),
        }

    # =========================================================================
    # Database Operations
    # =========================================================================

    def upsert_player_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update NBA player statistics."""
        query = query_cache.get_or_build_upsert(
            table="nba_player_stats",
            columns=NBA_PLAYER_STATS_COLUMNS,
            conflict_keys=["player_id", "season_id", "team_id"],
        )
        self.db.execute(query, stats)

    def upsert_team_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update NBA team statistics."""
        query = query_cache.get_or_build_upsert(
            table="nba_team_stats",
            columns=NBA_TEAM_STATS_COLUMNS,
            conflict_keys=["team_id", "season_id"],
        )
        self.db.execute(query, stats)

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _parse_minutes_total(self, minutes: Any) -> int:
        """Parse minutes to integer total.

        Note: This is NBA-specific for handling MM:SS format from game logs.
        """
        if isinstance(minutes, (int, float)):
            return int(minutes)
        if isinstance(minutes, str):
            try:
                return int(minutes)
            except ValueError:
                # Try parsing "MM:SS" format from game logs
                if ":" in minutes:
                    parts = minutes.split(":")
                    return int(parts[0]) * 60 + int(parts[1])
        return 0
