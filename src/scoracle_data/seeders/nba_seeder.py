"""
NBA-specific seeder for stats database.

Handles fetching and transforming NBA player and team statistics
from the API-Sports NBA API.
"""

from __future__ import annotations

import logging
import time
from datetime import datetime
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
        league: Optional[str] = None,
    ) -> list[dict[str, Any]]:
        """Fetch NBA teams from API-Sports.

        Args:
            season: Season year
            league: League filter - "standard" for NBA teams only (30 teams),
                   None returns all teams including G-League/historical (66+ teams)
        """
        # Use "standard" to get NBA teams, then filter by nbaFranchise=True
        teams = await self.api.list_teams("NBA", league=league or "standard")

        result = []
        for team in teams:
            # Filter to only NBA franchises (excludes international teams like Brisbane Bullets)
            if team.get("nbaFranchise") is False:
                continue

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
        """Fetch player statistics from API-Sports.

        NBA API returns per-game data, not season totals.
        This method fetches all games and returns them for aggregation.

        Returns dict with:
        - 'games': list of all game stats
        - 'player': player info
        - 'team': team info
        """
        try:
            import httpx

            # Need to call API directly to get all games
            async with httpx.AsyncClient(timeout=30.0) as client:
                r = await client.get(
                    "https://v2.nba.api-sports.io/players/statistics",
                    headers={"x-apisports-key": self.api.api_key},
                    params={"id": player_id, "season": season},
                )
                r.raise_for_status()
                data = r.json()

            games = data.get("response", [])
            if not games:
                return None

            # Return all games for aggregation
            return {
                "games": games,
                "player": games[0].get("player") if games else {},
                "team": games[0].get("team") if games else {},
            }
        except Exception as e:
            logger.warning("Failed to fetch stats for player %d: %s", player_id, e)
            return None

    async def fetch_team_stats(
        self,
        team_id: int,
        season: int,
    ) -> Optional[dict[str, Any]]:
        """Fetch team statistics from API-Sports.

        NBA win/loss records come from /standings, not /teams/statistics.
        We also fetch team statistics for PPG data.
        """
        try:
            # Get standings for win/loss records
            standings_data = await self.api.get_standings("NBA", str(season))
            standings = standings_data.get("response", [])

            # Find this team in standings
            team_standings = None
            for s in standings:
                if s.get("team", {}).get("id") == team_id:
                    team_standings = s
                    break

            if not team_standings:
                logger.warning("Team %d not found in standings", team_id)
                return None

            # Also get team statistics for PPG data
            try:
                team_stats = await self.api.get_team_statistics(
                    str(team_id), "NBA", str(season)
                )
                # Merge standings and stats
                if team_stats:
                    team_standings["_team_stats"] = team_stats
            except Exception:
                pass  # PPG data is optional

            return team_standings
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
                venue_surface = venue.get("surface")
                venue_image = venue.get("image")
            else:
                venue_name = team.get("arena")
                venue_city = team.get("city")
                venue_capacity = None
                venue_surface = None
                venue_image = None

            # Handle leagues nested structure
            leagues = team.get("leagues")
            conference = team.get("conference")
            division = team.get("division")
            if isinstance(leagues, dict):
                standard = leagues.get("standard", {})
                if isinstance(standard, dict):
                    conference = conference or standard.get("conference")
                    division = division or standard.get("division")

            # Additional metadata (may not be available for all teams)
            is_nba_franchise = team.get("nbaFranchise", True)

            return {
                "id": team["id"],
                "name": team["name"],
                "abbreviation": team.get("abbreviation") or team.get("code") or team.get("nickname"),
                "logo_url": team.get("logo_url") or team.get("logo"),
                "conference": conference,
                "division": division,
                "city": team.get("city"),
                "country": team.get("country") or "USA",  # NBA teams are in USA
                "founded": team.get("founded"),
                "venue_name": venue_name,
                "venue_city": venue_city,
                "venue_capacity": venue_capacity,
                "venue_surface": venue_surface,
                "venue_image": venue_image,
                # Note: NBA API doesn't provide venue details or founded year
                # These would need to be supplemented from another source
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

            # Experience years - calculate from NBA start year
            nba_info = player.get("nba", {}) or {}
            experience_years = None
            if isinstance(nba_info, dict) and nba_info.get("start"):
                try:
                    start_year = int(nba_info["start"])
                    current_year = datetime.now().year
                    experience_years = current_year - start_year
                except (ValueError, TypeError):
                    pass

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
                "experience_years": experience_years,
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
        """Transform API stats to database schema.

        NBA API returns per-game data which needs to be aggregated.
        Expected input structure:
        {
            "games": [list of game stat dicts],
            "player": {...},
            "team": {...}
        }

        Each game has: points, min, fgm, fga, tpm, tpa, ftm, fta,
                       offReb, defReb, totReb, assists, steals, turnovers,
                       blocks, pFouls, plusMinus
        """
        data = raw_stats if isinstance(raw_stats, dict) else {}
        games = data.get("games", [])

        if not games:
            # Return empty/zero stats if no games
            return self._empty_player_stats(player_id, season_id, team_id)

        # Aggregate totals from all games
        games_played = len(games)
        games_started = 0  # API doesn't provide this per-game, would need roster data

        # Sum all numeric stats
        minutes_total = 0
        points_total = 0
        fgm = 0
        fga = 0
        tpm = 0
        tpa = 0
        ftm = 0
        fta = 0
        off_reb = 0
        def_reb = 0
        assists = 0
        turnovers = 0
        steals = 0
        blocks = 0
        fouls = 0
        plus_minus = 0

        for game in games:
            # Parse minutes (can be "25" or "25:30")
            minutes_total += self._parse_minutes_total(game.get("min", 0))
            points_total += int(game.get("points", 0) or 0)
            fgm += int(game.get("fgm", 0) or 0)
            fga += int(game.get("fga", 0) or 0)
            tpm += int(game.get("tpm", 0) or 0)
            tpa += int(game.get("tpa", 0) or 0)
            ftm += int(game.get("ftm", 0) or 0)
            fta += int(game.get("fta", 0) or 0)
            off_reb += int(game.get("offReb", 0) or 0)
            def_reb += int(game.get("defReb", 0) or 0)
            assists += int(game.get("assists", 0) or 0)
            turnovers += int(game.get("turnovers", 0) or 0)
            steals += int(game.get("steals", 0) or 0)
            blocks += int(game.get("blocks", 0) or 0)
            fouls += int(game.get("pFouls", 0) or 0)

            # Plus/minus can be "+5" or "-3"
            pm_raw = game.get("plusMinus", 0) or 0
            if isinstance(pm_raw, str):
                pm_raw = pm_raw.replace("+", "")
                try:
                    plus_minus += int(pm_raw)
                except ValueError:
                    pass
            else:
                plus_minus += int(pm_raw)

        tot_reb = off_reb + def_reb
        gp = max(games_played, 1)

        return {
            "player_id": player_id,
            "season_id": season_id,
            "team_id": team_id,
            "games_played": games_played,
            "games_started": games_started,
            "minutes_total": minutes_total,
            "minutes_per_game": round(minutes_total / gp, 1),
            "points_total": points_total,
            "points_per_game": round(points_total / gp, 1),
            "fgm": fgm,
            "fga": fga,
            "fg_pct": DataParsers.safe_percentage(fgm, fga),
            "tpm": tpm,
            "tpa": tpa,
            "tp_pct": DataParsers.safe_percentage(tpm, tpa),
            "ftm": ftm,
            "fta": fta,
            "ft_pct": DataParsers.safe_percentage(ftm, fta),
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
            "updated_at": datetime.now(),
        }

    def transform_team_stats(
        self,
        raw_stats: dict[str, Any],
        team_id: int,
        season_id: int,
    ) -> dict[str, Any]:
        """Transform NBA standings/stats to database schema.

        NBA data comes from standings endpoint which has structure:
        {
            "win": {"home": X, "away": Y, "total": 0, ...},
            "loss": {"home": X, "away": Y, "total": 0, ...},
            "_team_stats": { optional PPG data }
        }

        Note: win.total and loss.total are often 0 (API bug), so we calculate from home+away.
        """
        stats = raw_stats if isinstance(raw_stats, dict) else {}

        # Extract win/loss from standings structure
        wins = stats.get("win", {}) if isinstance(stats.get("win"), dict) else {}
        losses = stats.get("loss", {}) if isinstance(stats.get("loss"), dict) else {}

        # Get home/away values
        home_wins = wins.get("home", 0) or 0
        away_wins = wins.get("away", 0) or 0
        home_losses = losses.get("home", 0) or 0
        away_losses = losses.get("away", 0) or 0

        # Calculate totals from home+away (since total field is often buggy/0)
        total_wins = home_wins + away_wins
        total_losses = home_losses + away_losses
        games_played = total_wins + total_losses

        # Calculate PPG from team statistics if available
        points_per_game = 0.0
        opponent_ppg = 0.0

        team_stats = stats.get("_team_stats")
        if team_stats and isinstance(team_stats, (dict, list)):
            # team_stats might be a list with one element
            if isinstance(team_stats, list) and team_stats:
                team_stats = team_stats[0]

            if isinstance(team_stats, dict):
                total_points = team_stats.get("points", 0) or 0
                total_games = team_stats.get("games", 0) or games_played
                if total_games > 0:
                    points_per_game = round(total_points / total_games, 1)

        return {
            "team_id": team_id,
            "season_id": season_id,
            "games_played": games_played,
            "wins": total_wins,
            "losses": total_losses,
            "win_pct": DataParsers.safe_percentage(total_wins, games_played),
            "home_wins": home_wins,
            "home_losses": home_losses,
            "away_wins": away_wins,
            "away_losses": away_losses,
            "points_per_game": points_per_game,
            "opponent_ppg": opponent_ppg,  # Not available from current API
            "updated_at": datetime.now(),
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
        # Convert dict to tuple in column order for %s placeholders
        params = tuple(stats.get(col) for col in NBA_PLAYER_STATS_COLUMNS)
        self.db.execute(query, params)

    def upsert_team_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update NBA team statistics."""
        query = query_cache.get_or_build_upsert(
            table="nba_team_stats",
            columns=NBA_TEAM_STATS_COLUMNS,
            conflict_keys=["team_id", "season_id"],
        )
        # Convert dict to tuple in column order for %s placeholders
        params = tuple(stats.get(col) for col in NBA_TEAM_STATS_COLUMNS)
        self.db.execute(query, params)

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _empty_player_stats(
        self, player_id: int, season_id: int, team_id: Optional[int] = None
    ) -> dict[str, Any]:
        """Return empty stats dict for player with no games."""
        from ..query_builder import NBA_PLAYER_STATS_COLUMNS

        # Create dict with all columns set to 0/None
        stats = {col: 0 for col in NBA_PLAYER_STATS_COLUMNS}
        stats["player_id"] = player_id
        stats["season_id"] = season_id
        stats["team_id"] = team_id
        stats["updated_at"] = datetime.now()
        return stats

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
