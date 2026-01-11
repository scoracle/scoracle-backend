"""
Football (Soccer) specific seeder for stats database.

Handles fetching and transforming Football player and team statistics
from the API-Sports Football API. Supports tiered league coverage.

Priority Leagues (Full Data + Stats):
  - Top 5 European: Premier League, La Liga, Bundesliga, Serie A, Ligue 1
  - MLS (USA market, but excluded from percentile calculations)

Non-Priority Leagues:
  - Minimal data for autocomplete only
"""

from __future__ import annotations

import logging
import re
import time
from datetime import datetime
from typing import Any, Optional

from .base import BaseSeeder
from .utils import DataParsers, NameBuilder, StatCalculators, PositionMappers
from ..query_builder import (
    query_cache,
    FOOTBALL_PLAYER_STATS_COLUMNS,
    FOOTBALL_TEAM_STATS_COLUMNS,
)

logger = logging.getLogger(__name__)


# Priority leagues with full data coverage
# priority_tier: 1 = full data, 0 = minimal
# include_in_percentiles: 1 = used for percentile calcs (Top 5 European), 0 = excluded
PRIORITY_LEAGUES = [
    {"id": 39, "name": "Premier League", "country": "England", "priority_tier": 1, "include_in_percentiles": 1},
    {"id": 140, "name": "La Liga", "country": "Spain", "priority_tier": 1, "include_in_percentiles": 1},
    {"id": 78, "name": "Bundesliga", "country": "Germany", "priority_tier": 1, "include_in_percentiles": 1},
    {"id": 135, "name": "Serie A", "country": "Italy", "priority_tier": 1, "include_in_percentiles": 1},
    {"id": 61, "name": "Ligue 1", "country": "France", "priority_tier": 1, "include_in_percentiles": 1},
    {"id": 253, "name": "MLS", "country": "USA", "priority_tier": 1, "include_in_percentiles": 0},  # Full data, no percentiles
]

# Non-priority leagues (minimal data, for reference)
NON_PRIORITY_LEAGUES = [
    {"id": 94, "name": "Primeira Liga", "country": "Portugal", "priority_tier": 0, "include_in_percentiles": 0},
    {"id": 88, "name": "Eredivisie", "country": "Netherlands", "priority_tier": 0, "include_in_percentiles": 0},
    {"id": 144, "name": "Belgian Pro League", "country": "Belgium", "priority_tier": 0, "include_in_percentiles": 0},
    {"id": 203, "name": "Super Lig", "country": "Turkey", "priority_tier": 0, "include_in_percentiles": 0},
    {"id": 2, "name": "Champions League", "country": "Europe", "priority_tier": 0, "include_in_percentiles": 0},
    {"id": 3, "name": "Europa League", "country": "Europe", "priority_tier": 0, "include_in_percentiles": 0},
]

# Default to priority leagues only
DEFAULT_LEAGUES = PRIORITY_LEAGUES


class FootballSeeder(BaseSeeder):
    """Seeder for Football (Soccer) statistics with tiered league support."""

    sport_id = "FOOTBALL"

    def __init__(self, *args, leagues: Optional[list[dict]] = None, priority_only: bool = True, **kwargs):
        """
        Initialize the Football seeder.

        Args:
            leagues: List of leagues to seed. Defaults to priority leagues.
            priority_only: If True, only seed priority leagues. Defaults to True.
        """
        super().__init__(*args, **kwargs)
        if leagues:
            self.leagues = leagues
        elif priority_only:
            self.leagues = PRIORITY_LEAGUES
        else:
            self.leagues = PRIORITY_LEAGUES + NON_PRIORITY_LEAGUES

    def _get_season_label(self, season_year: int) -> str:
        """Football seasons span two years (e.g., 2024-25)."""
        next_year = (season_year + 1) % 100
        return f"{season_year}-{next_year:02d}"

    # =========================================================================
    # League Management
    # =========================================================================

    def ensure_leagues(self) -> list[int]:
        """Ensure all configured leagues exist in the database with priority tiers."""
        league_ids = []

        for league in self.leagues:
            # Convert integers to booleans for PostgreSQL
            include_in_pctile = bool(league.get("include_in_percentiles", 0))
            self.db.execute(
                """
                INSERT INTO leagues (id, sport_id, name, country, priority_tier, include_in_percentiles, is_active)
                VALUES (%s, %s, %s, %s, %s, %s, true)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    country = excluded.country,
                    priority_tier = excluded.priority_tier,
                    include_in_percentiles = excluded.include_in_percentiles,
                    updated_at = NOW()
                """,
                (
                    league["id"],
                    self.sport_id,
                    league["name"],
                    league["country"],
                    league.get("priority_tier", 0),
                    include_in_pctile,
                ),
            )
            league_ids.append(league["id"])

        logger.info(
            "Ensured %d leagues (%d priority, %d with percentiles)",
            len(league_ids),
            sum(1 for lg in self.leagues if lg.get("priority_tier") == 1),
            sum(1 for lg in self.leagues if lg.get("include_in_percentiles") == 1),
        )
        return league_ids

    async def seed_two_phase(
        self,
        season: int,
        league_id: Optional[int] = None,
        skip_profiles: bool = False,
    ) -> dict[str, Any]:
        """
        Override to ensure leagues exist before discovery phase.

        Football teams have a foreign key to leagues, so we must
        insert the league records first.
        """
        # Ensure leagues exist before inserting teams
        self.ensure_leagues()

        # Call parent implementation
        return await super().seed_two_phase(season, league_id, skip_profiles)

    async def seed_player_stats(self, season: int) -> int:
        """
        Seed player statistics for a season.

        Override for Football to pass league_id to the API request.
        The Football API requires league_id to return accurate stats.

        Args:
            season: Season year

        Returns:
            Number of stat records seeded
        """
        season_id = self.ensure_season(season)
        sync_id = self._start_sync("full", "player_stats", season_id)

        try:
            # Query players with their current_league_id for API filtering
            players = self.db.fetchall(
                """
                SELECT id, current_team_id, current_league_id
                FROM players
                WHERE sport_id = %s
                """,
                (self.sport_id,),
            )

            # Build set of known league IDs
            known_league_ids = {lg["id"] for lg in self.leagues}

            processed = 0
            skipped = 0
            for player in players:
                player_id = player["id"]
                team_id = player.get("current_team_id")
                league_id = player.get("current_league_id")

                # Skip if player's league is not in our configured leagues
                if league_id not in known_league_ids:
                    skipped += 1
                    continue

                # Fetch stats with league_id filter for accurate results
                raw_stats = await self.fetch_player_stats(player_id, season, league_id)
                if raw_stats:
                    stats = self.transform_player_stats(
                        raw_stats, player_id, season_id, team_id
                    )
                    if stats:
                        self.upsert_player_stats(stats)
                        processed += 1

            self._complete_sync(sync_id, len(players), processed, 0)
            logger.info(
                "Seeded stats for %d players for %s %d (skipped %d without priority league)",
                processed,
                self.sport_id,
                season,
                skipped,
            )
            return processed

        except Exception as e:
            self._fail_sync(sync_id, str(e))
            raise

    # =========================================================================
    # Data Fetching
    # =========================================================================

    async def fetch_teams(
        self,
        season: int,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """
        Fetch Football teams from API-Sports.

        Uses: GET /teams?league={id}&season={year}

        Args:
            season: Season year
            league_id: Optional specific league. If None, fetches all configured leagues.

        Returns:
            List of team data dicts
        """
        all_teams = []
        seen_ids = set()

        # Determine which leagues to fetch
        if league_id:
            leagues_to_fetch = [lg for lg in self.leagues if lg["id"] == league_id]
        else:
            leagues_to_fetch = self.leagues

        for league in leagues_to_fetch:
            lid = league["id"]

            try:
                teams = await self.api.list_teams(
                    "FOOTBALL",
                    league=lid,
                    season=str(season),
                )

                for team in teams:
                    team_id = team["id"]
                    if team_id in seen_ids:
                        continue

                    seen_ids.add(team_id)
                    venue = team.get("venue", {}) if isinstance(team.get("venue"), dict) else {}

                    all_teams.append({
                        "id": team_id,
                        "name": team["name"],
                        "abbreviation": team.get("code") or team.get("abbreviation"),
                        "logo_url": team.get("logo_url") or team.get("logo"),
                        "country": team.get("country") or league["country"],
                        "league_id": lid,
                        "venue_name": venue.get("name"),
                        "venue_city": venue.get("city"),
                        "venue_capacity": venue.get("capacity"),
                        "venue_surface": venue.get("surface"),
                        "venue_image": venue.get("image"),
                        "founded": team.get("founded"),
                    })

                logger.debug("Fetched %d teams from league %d", len(teams), lid)

            except Exception as e:
                logger.warning("Failed to fetch teams for league %d: %s", lid, e)

        return all_teams

    async def fetch_players(
        self,
        season: int,
        team_id: Optional[int] = None,
        league_id: Optional[int] = None,
    ) -> list[dict[str, Any]]:
        """
        Fetch Football players from API-Sports using league-based pagination.

        Uses: GET /players?league={id}&season={year}&page=N (20 players per page)

        This is more efficient than team-based fetching:
        - League-based: ~40 calls for 800 players (entire league)
        - Team-based: 20 calls × 1 per team = 20 calls for same data

        Args:
            season: Season year
            team_id: Ignored for Football (use league_id instead)
            league_id: Optional specific league. If None, fetches all configured leagues.

        Returns:
            List of player data dicts with current_league_id set
        """
        all_players = []
        seen_ids = set()

        # Determine which leagues to fetch
        if league_id:
            leagues_to_fetch = [lg for lg in self.leagues if lg["id"] == league_id]
        else:
            leagues_to_fetch = self.leagues

        for league in leagues_to_fetch:
            lid = league["id"]
            page = 1
            max_pages = 100  # Safety limit (~2000 players per league)

            while page <= max_pages:
                try:
                    # League-based fetching: GET /players?league={id}&season={year}&page=N
                    players = await self.api.list_players(
                        "FOOTBALL",
                        season=str(season),
                        page=page,
                        league=lid,
                    )

                    if not players:
                        break

                    for player in players:
                        player_id = player["id"]
                        if player_id in seen_ids:
                            continue

                        seen_ids.add(player_id)
                        team_data = player.get("team") or player.get("statistics", [{}])[0].get("team") or {}
                        birth = player.get("birth", {}) or {}

                        all_players.append({
                            "id": player_id,
                            "first_name": player.get("first_name") or player.get("firstname"),
                            "last_name": player.get("last_name") or player.get("lastname"),
                            "full_name": self._build_full_name(player),
                            "position": player.get("position"),
                            "position_group": self._get_position_group(player.get("position")),
                            "nationality": player.get("nationality"),
                            "birth_date": player.get("birth_date") or birth.get("date"),
                            "birth_place": birth.get("place"),
                            "height_inches": self._parse_height(player),
                            "weight_lbs": self._parse_weight(player),
                            "photo_url": player.get("photo_url") or player.get("photo"),
                            "current_team_id": team_data.get("id") if isinstance(team_data, dict) else None,
                            "current_league_id": lid,  # Track which league this player is in
                            "jersey_number": player.get("number"),
                        })

                    # Check if we've reached the last page
                    if len(players) < 20:  # API returns 20 per page
                        break

                    page += 1

                except Exception as e:
                    logger.warning("Failed to fetch players for league %d page %d: %s", lid, page, e)
                    break

            logger.info("Fetched %d players from league %d (%d pages)", len([p for p in all_players if p.get("current_league_id") == lid]), lid, page)

        return all_players

    async def fetch_team_profile(self, team_id: int) -> Optional[dict[str, Any]]:
        """
        Fetch full team profile from API-Sports.

        Uses: GET /teams?id={id}

        The Football API returns full profile data including venue details
        in the teams endpoint.

        Args:
            team_id: Team ID

        Returns:
            Full team profile data or None
        """
        try:
            team = await self.api.get_team_profile(str(team_id), "FOOTBALL")

            if team:
                # api_client already flattens venue data with venue_ prefix
                return {
                    "id": team["id"],
                    "name": team["name"],
                    "abbreviation": team.get("code") or team.get("abbreviation"),
                    "logo_url": team.get("logo_url") or team.get("logo"),
                    "country": team.get("country"),
                    "city": team.get("venue_city"),  # Use venue city as team city
                    "founded": team.get("founded"),
                    "is_national": bool(team.get("national", False)),
                    "venue_name": team.get("venue_name"),
                    "venue_address": team.get("venue_address"),
                    "venue_city": team.get("venue_city"),
                    "venue_capacity": team.get("venue_capacity"),
                    "venue_surface": team.get("venue_surface"),
                    "venue_image": team.get("venue_image"),
                }

            return None
        except Exception as e:
            logger.warning("Failed to fetch profile for team %d: %s", team_id, e)
            return None

    async def fetch_player_profile(self, player_id: int) -> Optional[dict[str, Any]]:
        """
        Fetch full player profile from API-Sports.

        Uses: GET /players?id={id}&season={year}

        Note: The Football /players endpoint includes both profile info AND stats
        in a single call, so we get everything we need here.

        Args:
            player_id: Player ID

        Returns:
            Full player profile data or None
        """
        try:
            player = await self.api.get_player_profile(str(player_id), "FOOTBALL")

            if player:
                # Football player profile has nested structure
                player_data = player.get("player", player) or player
                birth = player_data.get("birth", {}) or {}
                team_data = player_data.get("team") or {}

                # If there's statistics data, get team from there
                stats = player.get("statistics", [])
                if stats and isinstance(stats, list) and stats[0]:
                    team_data = stats[0].get("team", team_data) or team_data
                    league_data = stats[0].get("league", {}) or {}
                else:
                    league_data = {}

                return {
                    "id": player_data.get("id") or player.get("id"),
                    "first_name": player_data.get("first_name") or player_data.get("firstname"),
                    "last_name": player_data.get("last_name") or player_data.get("lastname"),
                    "full_name": self._build_full_name(player_data),
                    "position": player_data.get("position"),
                    "position_group": self._get_position_group(player_data.get("position")),
                    "nationality": player_data.get("nationality"),
                    "birth_date": player_data.get("birth_date") or birth.get("date"),
                    "birth_place": birth.get("place"),
                    "birth_country": birth.get("country"),
                    "height_inches": self._parse_height(player_data),
                    "weight_lbs": self._parse_weight(player_data),
                    "photo_url": player_data.get("photo_url") or player_data.get("photo"),
                    "current_team_id": team_data.get("id") if isinstance(team_data, dict) else None,
                    "current_league_id": league_data.get("id"),
                    "jersey_number": player_data.get("number"),
                }

            return None
        except Exception as e:
            logger.warning("Failed to fetch profile for player %d: %s", player_id, e)
            return None

    async def fetch_player_stats(
        self,
        player_id: int,
        season: int,
        league_id: Optional[int] = None,
    ) -> Optional[dict[str, Any]]:
        """Fetch player statistics from API-Sports.

        Note: Football requires league_id for player statistics.
        """
        try:
            stats = await self.api.get_player_statistics(
                str(player_id),
                "FOOTBALL",
                str(season),
                league_id=league_id,
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
        """Fetch team statistics from API-Sports.

        Note: Football requires league_id for team statistics.
        """
        try:
            stats = await self.api.get_team_statistics(
                str(team_id),
                "FOOTBALL",
                str(season),
                league_id=league_id,
            )
            return stats
        except Exception as e:
            logger.warning("Failed to fetch stats for team %d: %s", team_id, e)
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
    ) -> Optional[dict[str, Any]]:
        """Transform API stats to database schema.

        Note: get_player_statistics() already unwraps the API response,
        so raw_stats is {"player": {...}, "statistics": [...]} not the full
        {"response": [...]} wrapper.

        Returns None if no stats from a known league are found.
        """
        stats = raw_stats if isinstance(raw_stats, dict) else {}

        # Build set of known league IDs from our configured leagues
        known_league_ids = {lg["id"] for lg in self.leagues}

        # Handle already-unwrapped format from get_player_statistics
        # Format: {"player": {...}, "statistics": [{team, league, games, ...}]}
        statistics_list = []
        if "statistics" in stats and stats["statistics"]:
            statistics_list = stats["statistics"] if isinstance(stats["statistics"], list) else []
        # Also handle full response format (for backwards compatibility)
        elif "response" in stats and stats["response"]:
            response = stats["response"]
            if isinstance(response, list) and response:
                player_data = response[0]
                statistics_list = player_data.get("statistics", [])

        # Find stats from a known league (prioritize our configured leagues)
        stats = None
        for stat_entry in statistics_list:
            league = stat_entry.get("league", {}) or {}
            if league.get("id") in known_league_ids:
                stats = stat_entry
                break

        # If no known league stats found, skip this player
        if not stats:
            return None

        # Get league ID from stats
        league = stats.get("league", {}) or {}
        league_id = league.get("id")

        # Games
        games = stats.get("games", {}) or {}
        appearances = games.get("appearences", 0) or games.get("appearances", 0) or 0
        starts = games.get("lineups", 0) or 0
        minutes = games.get("minutes", 0) or 0
        rating_str = games.get("rating")
        rating = float(rating_str) if rating_str else 0.0
        is_captain = bool(games.get("captain", False))

        # Goals & Assists
        goals_data = stats.get("goals", {}) or {}
        goals = goals_data.get("total", 0) or 0
        assists = goals_data.get("assists", 0) or 0

        # Shots
        shots = stats.get("shots", {}) or {}
        shots_total = shots.get("total", 0) or 0
        shots_on = shots.get("on", 0) or 0

        # Passing
        passes = stats.get("passes", {}) or {}
        passes_total = passes.get("total", 0) or 0
        passes_acc = passes.get("accuracy", 0) or 0
        key_passes = passes.get("key", 0) or 0

        # Dribbles
        dribbles = stats.get("dribbles", {}) or {}
        dribbles_attempted = dribbles.get("attempts", 0) or 0
        dribbles_success = dribbles.get("success", 0) or 0

        # Duels
        duels = stats.get("duels", {}) or {}
        duels_total = duels.get("total", 0) or 0
        duels_won = duels.get("won", 0) or 0

        # Tackles
        tackles = stats.get("tackles", {}) or {}
        tackles_total = tackles.get("total", 0) or 0
        interceptions = tackles.get("interceptions", 0) or 0
        blocks = tackles.get("blocks", 0) or 0

        # Fouls
        fouls = stats.get("fouls", {}) or {}
        fouls_drawn = fouls.get("drawn", 0) or 0
        fouls_committed = fouls.get("committed", 0) or 0

        # Cards
        cards = stats.get("cards", {}) or {}
        yellows = cards.get("yellow", 0) or 0
        reds = cards.get("red", 0) or 0
        second_yellows = cards.get("yellowred", 0) or 0

        # Penalties
        penalty = stats.get("penalty", {}) or {}
        pen_won = penalty.get("won", 0) or 0
        pen_scored = penalty.get("scored", 0) or 0
        pen_missed = penalty.get("missed", 0) or 0
        pen_conceded = penalty.get("commited", 0) or 0  # Note: API typo "commited"

        # Goalkeeper stats
        gk = stats.get("goals", {}) or {}
        gk_conceded = gk.get("conceded", 0) or 0
        gk_saves = gk.get("saves", 0) or 0

        # Substitutes info
        subs = stats.get("substitutes", {}) or {}
        bench_apps = subs.get("bench", 0) or 0
        subs_in = subs.get("in", 0) or 0
        subs_out = subs.get("out", 0) or 0

        # Calculate per-90 stats
        minutes_played = max(minutes, 1)
        per_90_factor = 90 / minutes_played if minutes_played > 90 else 1
        min_90_threshold = minutes >= 90

        return {
            "player_id": player_id,
            "season_id": season_id,
            "team_id": team_id,
            "league_id": league_id,
            "appearances": appearances,
            "starts": starts,
            "bench_appearances": bench_apps if bench_apps else max(0, appearances - starts),
            "minutes_played": minutes,
            "rating": rating,
            "is_captain": is_captain,
            "subs_in": subs_in,
            "subs_out": subs_out,
            "goals": goals,
            "assists": assists,
            "goals_assists": goals + assists,
            "goals_per_90": round(goals * per_90_factor, 2) if min_90_threshold else 0,
            "assists_per_90": round(assists * per_90_factor, 2) if min_90_threshold else 0,
            "shots_total": shots_total,
            "shots_on_target": shots_on,
            "shot_accuracy": self._safe_pct(shots_on, shots_total),
            "shots_per_90": round(shots_total * per_90_factor, 2) if min_90_threshold else 0,
            "goals_per_shot": self._safe_pct(goals, shots_total) / 100 if shots_total else 0,
            "goals_per_shot_on_target": self._safe_pct(goals, shots_on) / 100 if shots_on else 0,
            "passes_total": passes_total,
            "passes_accurate": int(passes_total * (passes_acc / 100)) if passes_acc else 0,
            "pass_accuracy": passes_acc,
            "passes_per_90": round(passes_total * per_90_factor, 2) if min_90_threshold else 0,
            "key_passes": key_passes,
            "key_passes_per_90": round(key_passes * per_90_factor, 2) if min_90_threshold else 0,
            "dribbles_attempted": dribbles_attempted,
            "dribbles_successful": dribbles_success,
            "dribble_success_rate": self._safe_pct(dribbles_success, dribbles_attempted),
            "dribbles_per_90": round(dribbles_success * per_90_factor, 2) if min_90_threshold else 0,
            "duels_total": duels_total,
            "duels_won": duels_won,
            "duel_success_rate": self._safe_pct(duels_won, duels_total),
            "tackles": tackles_total,
            "tackles_per_90": round(tackles_total * per_90_factor, 2) if min_90_threshold else 0,
            "interceptions": interceptions,
            "interceptions_per_90": round(interceptions * per_90_factor, 2) if min_90_threshold else 0,
            "blocks": blocks,
            "fouls_committed": fouls_committed,
            "fouls_drawn": fouls_drawn,
            "yellow_cards": yellows,
            "red_cards": reds,
            "second_yellow_cards": second_yellows,
            "penalties_won": pen_won,
            "penalties_scored": pen_scored,
            "penalties_missed": pen_missed,
            "penalties_conceded": pen_conceded,
            "saves": gk_saves,
            "save_percentage": self._safe_pct(gk_saves, gk_saves + gk_conceded) if gk_saves else 0,
            "goals_conceded": gk_conceded,
            "goals_conceded_per_90": round(gk_conceded * per_90_factor, 2) if min_90_threshold and gk_conceded else 0,
            "updated_at": datetime.now(),
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
            stats = stats["response"]

        league = stats.get("league", {}) or {}
        league_id = league.get("id")

        # Fixtures
        fixtures = stats.get("fixtures", {}) or {}
        played = fixtures.get("played", {}) or {}
        wins = fixtures.get("wins", {}) or {}
        draws = fixtures.get("draws", {}) or {}
        losses = fixtures.get("losses", {}) or {}

        home_played = played.get("home", 0) or 0
        away_played = played.get("away", 0) or 0
        total_played = played.get("total", 0) or home_played + away_played

        # Goals
        goals = stats.get("goals", {}) or {}
        goals_for = goals.get("for", {}) or {}
        goals_against = goals.get("against", {}) or {}

        gf_total = goals_for.get("total", {}).get("total", 0) or 0
        ga_total = goals_against.get("total", {}).get("total", 0) or 0

        # Clean sheets & failed to score
        clean_sheets = stats.get("clean_sheet", {}) or {}
        failed_to_score = stats.get("failed_to_score", {}) or {}

        return {
            "team_id": team_id,
            "season_id": season_id,
            "league_id": league_id,
            "matches_played": total_played,
            "wins": wins.get("total", 0) or 0,
            "draws": draws.get("total", 0) or 0,
            "losses": losses.get("total", 0) or 0,
            "points": (wins.get("total", 0) or 0) * 3 + (draws.get("total", 0) or 0),
            "home_played": home_played,
            "home_wins": wins.get("home", 0) or 0,
            "home_draws": draws.get("home", 0) or 0,
            "home_losses": losses.get("home", 0) or 0,
            "home_goals_for": goals_for.get("total", {}).get("home", 0) or 0,
            "home_goals_against": goals_against.get("total", {}).get("home", 0) or 0,
            "away_played": away_played,
            "away_wins": wins.get("away", 0) or 0,
            "away_draws": draws.get("away", 0) or 0,
            "away_losses": losses.get("away", 0) or 0,
            "away_goals_for": goals_for.get("total", {}).get("away", 0) or 0,
            "away_goals_against": goals_against.get("total", {}).get("away", 0) or 0,
            "goals_for": gf_total,
            "goals_against": ga_total,
            "goal_difference": gf_total - ga_total,
            "goals_per_game": round(gf_total / max(total_played, 1), 2),
            "goals_conceded_per_game": round(ga_total / max(total_played, 1), 2),
            "clean_sheets": clean_sheets.get("total", 0) or 0,
            "failed_to_score": failed_to_score.get("total", 0) or 0,
            "form": stats.get("form", ""),
            "avg_possession": self._extract_avg_possession(stats),
            "updated_at": datetime.now(),
        }

    # =========================================================================
    # Database Operations
    # =========================================================================

    def upsert_player_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update Football player statistics."""
        query = query_cache.get_or_build_upsert(
            table="football_player_stats",
            columns=FOOTBALL_PLAYER_STATS_COLUMNS,
            conflict_keys=["player_id", "season_id", "league_id"],
        )
        # Convert dict to tuple in column order for %s placeholders
        params = tuple(stats.get(col) for col in FOOTBALL_PLAYER_STATS_COLUMNS)
        self.db.execute(query, params)

    def upsert_team_stats(self, stats: dict[str, Any]) -> None:
        """Insert or update Football team statistics."""
        query = query_cache.get_or_build_upsert(
            table="football_team_stats",
            columns=FOOTBALL_TEAM_STATS_COLUMNS,
            conflict_keys=["team_id", "season_id", "league_id"],
        )
        # Convert dict to tuple in column order for %s placeholders
        params = tuple(stats.get(col) for col in FOOTBALL_TEAM_STATS_COLUMNS)
        self.db.execute(query, params)

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def _build_full_name(self, player: dict) -> str:
        """Build full name from first and last name."""
        first = player.get("first_name") or player.get("firstname") or ""
        last = player.get("last_name") or player.get("lastname") or ""
        name = player.get("name") or f"{first} {last}".strip()
        return name or "Unknown"

    def _get_position_group(self, position: Optional[str]) -> Optional[str]:
        """Map position to position group."""
        if not position:
            return None

        position = position.lower()

        if "goalkeeper" in position or position == "gk":
            return "Goalkeeper"
        elif "defender" in position or position in ("cb", "lb", "rb", "wb"):
            return "Defender"
        elif "midfielder" in position or position in ("cm", "dm", "am", "lm", "rm"):
            return "Midfielder"
        elif "attacker" in position or "forward" in position or position in ("cf", "lw", "rw", "st"):
            return "Forward"

        return None

    def _parse_height(self, player: dict) -> Optional[int]:
        """Parse height to inches.

        Football API returns height in cm (e.g., "180 cm" or "180").
        Convert to inches for consistent imperial storage.
        """
        height = player.get("height")
        if not height:
            return None

        if isinstance(height, str):
            # Extract numeric portion from strings like "180 cm"
            match = re.match(r"(\d+)", height)
            if match:
                try:
                    cm = int(match.group(1))
                    # Convert cm to inches
                    return int(cm / 2.54)
                except (ValueError, TypeError):
                    pass
        elif isinstance(height, (int, float)):
            # Assume cm if numeric
            return int(height / 2.54)

        return None

    def _parse_weight(self, player: dict) -> Optional[int]:
        """Parse weight to pounds.

        Football API returns weight in kg (e.g., "75 kg" or "75").
        Convert to pounds for consistent imperial storage.
        """
        weight = player.get("weight")
        if not weight:
            return None

        if isinstance(weight, str):
            # Extract numeric portion from strings like "75 kg"
            match = re.match(r"(\d+)", weight)
            if match:
                try:
                    kg = int(match.group(1))
                    # Convert kg to lbs
                    return int(kg * 2.205)
                except (ValueError, TypeError):
                    pass
        elif isinstance(weight, (int, float)):
            # Assume kg if numeric
            return int(weight * 2.205)

        return None

    def _safe_pct(self, made: int, total: int) -> float:
        """Calculate percentage safely."""
        if not total:
            return 0.0
        return round((made / total) * 100, 1)

    def _extract_avg_possession(self, stats: dict) -> float:
        """Extract average possession from stats."""
        possession = stats.get("possession", {}) or {}
        if possession:
            # Can be structured as {"0-15": "52%", ...} or {"average": 52}
            avg = possession.get("average")
            if avg:
                if isinstance(avg, str):
                    return float(avg.replace("%", ""))
                return float(avg)

            # Calculate from all values
            values = []
            for key, val in possession.items():
                if isinstance(val, str) and "%" in val:
                    try:
                        values.append(float(val.replace("%", "")))
                    except ValueError:
                        pass

            if values:
                return round(sum(values) / len(values), 1)

        return 0.0

    async def seed_all(
        self,
        seasons: list[int],
        current_season: Optional[int] = None,
    ) -> dict[str, int]:
        """
        Seed all data for multiple seasons.

        Overrides base to ensure leagues are created first.
        """
        # Ensure leagues exist
        self.ensure_leagues()

        # Call parent implementation
        return await super().seed_all(seasons, current_season)
