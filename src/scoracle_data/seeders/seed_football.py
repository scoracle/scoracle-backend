"""Football seeder — seeds Football (Soccer) data from SportMonks API into PostgreSQL.

Uses the canonical SportMonksClient provider and unified tables
(players, player_stats, teams, team_stats) with JSONB stats.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.

Player stats are fetched per-team via squad iteration (the bulk
/statistics/seasons/players endpoint returns empty on our plan tier).
Full 30+ stat mapping using verified SportMonks type_ids, stored as JSONB.
"""

import json
import logging
from datetime import datetime
from decimal import Decimal
from typing import Any, TYPE_CHECKING

from ..core.types import (
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
)
from ..providers.sportmonks import SportMonksClient
from .base import BaseSeedRunner
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

SPORT = "FOOTBALL"

# Provider-specific IDs now live in the database:
#   - League -> SportMonks ID mapping: leagues.sportmonks_id column
#   - Season -> SportMonks season ID mapping: provider_seasons table
# See migrations 001_schema.sql and 006_provider_seasons.sql.

# =========================================================================
# Verified SportMonks PlayerStatisticDetail type_id mappings
# Source: https://api.sportmonks.com/v3/core/types + live API validation
# =========================================================================

PLAYER_STAT_TYPES: dict[str, int | None] = {
    # Appearances
    "appearances": 321,  # code: appearances
    "lineups": 322,  # code: lineups
    "minutes_played": 119,  # code: minutes-played
    # Goals & Assists
    "goals": 52,  # code: goals - value has {total, goals, penalties}
    "assists": 79,  # code: assists
    # Shooting
    "shots_total": 42,  # code: shots-total
    "shots_on_target": 86,  # code: shots-on-target
    # Passing
    "passes_total": 80,  # code: passes
    "passes_accurate": 116,  # code: accurate-passes
    "key_passes": 117,  # code: key-passes
    "crosses_total": 98,  # code: total-crosses
    "crosses_accurate": 99,  # code: accurate-crosses
    # Defensive
    "tackles": 78,  # code: tackles
    "interceptions": 100,  # code: interceptions
    "clearances": 101,  # code: clearances
    "blocks": 97,  # code: blocked-shots
    # Duels
    "duels_total": 105,  # code: total-duels
    "duels_won": 106,  # code: duels-won
    # Dribbles
    "dribbles_attempts": 108,  # code: dribble-attempts
    "dribbles_success": 109,  # code: successful-dribbles
    # Discipline
    "yellow_cards": 84,  # code: yellowcards - value has {total, home, away}
    "red_cards": 83,  # code: redcards
    "fouls_committed": 56,  # code: fouls
    "fouls_drawn": 96,  # code: fouls-drawn
    # Goalkeeper
    "saves": 57,  # code: saves
    "goals_conceded": 88,  # code: goals-conceded
    # Expected stats (may not be available for all players/tiers)
    "expected_goals": 5304,  # code: expected-goals (xG)
    "expected_assists": None,  # No direct xA type_id in SportMonks
}

# =========================================================================
# Verified SportMonks Standing Detail type_id mappings
# Source: standings/seasons/{id}?include=details.type
# =========================================================================

STANDING_DETAIL_TYPES = {
    # Overall
    "matches_played": 129,  # code: overall-matches-played
    "wins": 130,  # code: overall-won
    "draws": 131,  # code: overall-draw
    "losses": 132,  # code: overall-lost
    "goals_for": 133,  # code: overall-goals-for
    "goals_against": 134,  # code: overall-goals-against
    "goal_difference": 179,  # code: goal-difference
    "overall_points": 187,  # code: overall-points
    # Home
    "home_played": 135,  # code: home-matches-played
    "home_won": 136,  # code: home-won
    "home_draw": 137,  # code: home-draw
    "home_lost": 138,  # code: home-lost
    "home_scored": 139,  # code: home-scored
    "home_conceded": 140,  # code: home-conceded
    "home_points": 185,  # code: home-points
    # Away
    "away_played": 141,  # code: away-matches-played
    "away_won": 142,  # code: away-won
    "away_draw": 143,  # code: away-draw
    "away_lost": 144,  # code: away-lost
    "away_scored": 145,  # code: away-scored
    "away_conceded": 146,  # code: away-conceded
    "away_points": 186,  # code: away-points
}


class FootballSeedRunner(BaseSeedRunner):
    """Seeds Football data from SportMonks into unified PostgreSQL tables.

    Uses squad-by-squad iteration to fetch player stats individually,
    since the bulk season stats endpoint is empty on our plan tier.
    All stats are stored as JSONB in the unified player_stats/team_stats tables.
    """

    _sport_label = SPORT  # Used by BaseSeedRunner._ensure_player_exists

    def __init__(self, db: "PostgresDB", client: SportMonksClient):
        super().__init__(db, client)

    # =========================================================================
    # Stat Extraction Helpers
    # =========================================================================

    @staticmethod
    def _extract_stat_value(details: list[dict], type_id: int | None) -> int | None:
        """Extract a stat value from a details array by type_id.

        Handles various value formats:
        - {"total": N}
        - {"total": N, "goals": N, "penalties": N}
        - plain integer
        - None if type_id not found
        """
        if type_id is None:
            return None
        for d in details:
            if d.get("type_id") == type_id:
                val = d.get("value")
                if val is None:
                    return None
                if isinstance(val, (int, float)):
                    return int(val)
                if isinstance(val, dict):
                    for key in ("total", "all", "count"):
                        if key in val:
                            v = val[key]
                            if isinstance(v, (int, float)):
                                return int(v)
                    return None
                if isinstance(val, str):
                    try:
                        return int(float(val))
                    except (ValueError, TypeError):
                        return None
        return None

    @staticmethod
    def _extract_stat_decimal(
        details: list[dict], type_id: int | None
    ) -> Decimal | None:
        """Extract a decimal stat value (for xG, per-90 metrics, etc.)."""
        if type_id is None:
            return None
        for d in details:
            if d.get("type_id") == type_id:
                val = d.get("value")
                if val is None:
                    return None
                if isinstance(val, (int, float)):
                    return Decimal(str(val))
                if isinstance(val, dict):
                    for key in ("total", "all", "average"):
                        if key in val:
                            try:
                                return Decimal(str(val[key]))
                            except Exception:
                                return None
                if isinstance(val, str):
                    try:
                        return Decimal(val)
                    except Exception:
                        return None
        return None

    # =========================================================================
    # Build JSONB stats dict from SportMonks details
    # =========================================================================

    def _build_player_stats_json(self, details: list[dict]) -> dict:
        """Extract all stat values into a flat dict for JSONB storage.

        Only extracts raw stat values from the API response. Derived metrics
        (per-90, accuracy rates) are computed by the Postgres trigger
        ``compute_football_derived_stats()`` on INSERT/UPDATE.
        """
        stats: dict[str, Any] = {}

        for stat_name, type_id in PLAYER_STAT_TYPES.items():
            if stat_name == "expected_goals":
                val = self._extract_stat_decimal(details, type_id)
                if val is not None:
                    stats[stat_name] = float(val)
            elif stat_name == "expected_assists":
                continue  # Not available
            else:
                val = self._extract_stat_value(details, type_id)
                if val is not None:
                    stats[stat_name] = val

        return stats

    # =========================================================================
    # Upsert: Teams
    # =========================================================================

    def _upsert_team(self, team: dict[str, Any]) -> None:
        venue = team.get("venue") or {}
        country = team.get("country") or {}

        country_name = country.get("name") if isinstance(country, dict) else None

        self.db.execute(
            f"""
            INSERT INTO {TEAMS_TABLE} (
                id, sport, name, short_code, country, logo_url,
                venue_name, venue_capacity, founded, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                short_code = EXCLUDED.short_code,
                country = EXCLUDED.country,
                logo_url = EXCLUDED.logo_url,
                venue_name = EXCLUDED.venue_name,
                venue_capacity = EXCLUDED.venue_capacity,
                founded = EXCLUDED.founded,
                meta = EXCLUDED.meta,
                updated_at = NOW()
        """,
            (
                team["id"],
                SPORT,
                team.get("name"),
                team.get("short_code"),
                country_name,
                team.get("image_path"),
                venue.get("name"),
                venue.get("capacity"),
                team.get("founded"),
                json.dumps(
                    {
                        "venue_city": venue.get("city"),
                        "venue_surface": venue.get("surface"),
                    }
                ),
            ),
        )

    # =========================================================================
    # Upsert: Players
    # =========================================================================

    def _upsert_player(self, player: dict[str, Any]) -> None:
        """Upsert a single player profile into the unified players table."""
        # Parse date of birth
        dob = None
        dob_str = player.get("date_of_birth")
        if dob_str:
            try:
                dob = datetime.strptime(dob_str, "%Y-%m-%d").date()
            except ValueError:
                pass

        # Extract nationality
        nationality_data = player.get("nationality")
        if isinstance(nationality_data, dict):
            nationality_name = nationality_data.get("name")
            nationality_id = nationality_data.get("id") or player.get("nationality_id")
        else:
            nationality_name = nationality_data
            nationality_id = player.get("nationality_id")

        # Extract detailed position
        detailed_pos_data = player.get("detailedposition") or player.get(
            "detailedPosition"
        )
        if isinstance(detailed_pos_data, dict):
            detailed_position = detailed_pos_data.get("name")
            position_id = detailed_pos_data.get("id") or player.get(
                "detailed_position_id"
            )
        else:
            detailed_position = detailed_pos_data
            position_id = player.get("detailed_position_id") or player.get(
                "position_id"
            )

        # Position: map position_id to general position name if available
        position = player.get("position")
        if not position and player.get("position_id"):
            pos_map = {
                24: "Goalkeeper",
                25: "Defender",
                26: "Midfielder",
                27: "Attacker",
            }
            position = pos_map.get(player.get("position_id"))

        # Build the name — prefer common_name, fall back to first+last
        name = (
            player.get("common_name")
            or player.get("display_name")
            or f"{player.get('firstname', '')} {player.get('lastname', '')}".strip()
            or f"Player {player['id']}"
        )

        # Sport-specific meta
        meta = {}
        if nationality_id:
            meta["nationality_id"] = nationality_id
        if position_id:
            meta["position_id"] = position_id
        display_name = player.get("display_name")
        if display_name:
            meta["display_name"] = display_name
        common_name = player.get("common_name")
        if common_name:
            meta["common_name"] = common_name

        self.db.execute(
            f"""
            INSERT INTO {PLAYERS_TABLE} (
                id, sport, name, first_name, last_name,
                position, detailed_position, nationality,
                height_cm, weight_kg, date_of_birth, photo_url, meta
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = COALESCE(EXCLUDED.name, {PLAYERS_TABLE}.name),
                first_name = COALESCE(EXCLUDED.first_name, {PLAYERS_TABLE}.first_name),
                last_name = COALESCE(EXCLUDED.last_name, {PLAYERS_TABLE}.last_name),
                position = COALESCE(EXCLUDED.position, {PLAYERS_TABLE}.position),
                detailed_position = COALESCE(EXCLUDED.detailed_position, {PLAYERS_TABLE}.detailed_position),
                nationality = COALESCE(EXCLUDED.nationality, {PLAYERS_TABLE}.nationality),
                height_cm = COALESCE(EXCLUDED.height_cm, {PLAYERS_TABLE}.height_cm),
                weight_kg = COALESCE(EXCLUDED.weight_kg, {PLAYERS_TABLE}.weight_kg),
                date_of_birth = COALESCE(EXCLUDED.date_of_birth, {PLAYERS_TABLE}.date_of_birth),
                photo_url = COALESCE(EXCLUDED.photo_url, {PLAYERS_TABLE}.photo_url),
                meta = COALESCE(EXCLUDED.meta, {PLAYERS_TABLE}.meta),
                updated_at = NOW()
        """,
            (
                player["id"],
                SPORT,
                name,
                player.get("firstname"),
                player.get("lastname"),
                position,
                detailed_position,
                nationality_name,
                player.get("height"),  # SportMonks returns cm
                player.get("weight"),  # SportMonks returns kg
                dob,
                player.get("image_path"),
                json.dumps(meta) if meta else "{}",
            ),
        )

    # =========================================================================
    # Teams (orchestration)
    # =========================================================================

    async def seed_teams(self, season_id: int) -> SeedResult:
        """Seed teams for a season."""
        logger.info(f"Seeding teams for season {season_id}...")
        result = SeedResult()

        try:
            teams = await self.client.get_teams_by_season(season_id)
            logger.info(f"Fetched {len(teams)} teams from API")
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info(f"Upserted {result.teams_upserted} teams")
        except Exception as e:
            error_msg = f"Error seeding teams: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        return result

    # =========================================================================
    # Players (orchestration)
    # =========================================================================

    async def seed_players_for_team(
        self,
        season_id: int,
        team_id: int,
    ) -> SeedResult:
        """Seed players for a single team using squad + individual enrichment."""
        result = SeedResult()

        try:
            players = await self.client.get_squad_player_stats(season_id, team_id)
            for player in players:
                try:
                    self._upsert_player(player)
                    result.players_upserted += 1
                except Exception as e:
                    pid = player.get("id", "?")
                    error_msg = f"Error upserting player {pid}: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)
        except Exception as e:
            error_msg = f"Error seeding players for team {team_id}: {e}"
            logger.warning(error_msg)
            result.errors.append(error_msg)

        return result

    async def seed_players(
        self,
        season_id: int,
        team_ids: list[int] | None = None,
    ) -> SeedResult:
        """Seed players for all teams in a season."""
        logger.info(f"Seeding players for season {season_id}...")
        result = SeedResult()

        try:
            if team_ids is None:
                teams = await self.client.get_teams_by_season(season_id)
                team_ids = [t["id"] for t in teams]

            for i, team_id in enumerate(team_ids):
                logger.info(
                    f"Seeding players for team {team_id} ({i + 1}/{len(team_ids)})..."
                )
                team_result = await self.seed_players_for_team(season_id, team_id)
                result = result + team_result

            logger.info(f"Upserted {result.players_upserted} players total")
        except Exception as e:
            error_msg = f"Error seeding players: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        return result

    # =========================================================================
    # Player Stats
    # =========================================================================

    async def seed_player_stats(
        self,
        season_id: int,
        league_id: int,
        season_year: int,
        sportmonks_league_id: int,
    ) -> SeedResult:
        """Seed player statistics for a season.

        Iterates through each team's squad, fetches individual player stats,
        and extracts the stats for the target league/season as JSONB.
        """
        logger.info(
            f"Seeding player stats for season {season_id} (league {league_id})..."
        )
        result = SeedResult()

        try:
            teams = await self.client.get_teams_by_season(season_id)
            team_ids = [t["id"] for t in teams]

            for i, team_id in enumerate(team_ids):
                logger.info(
                    f"Fetching player stats for team {team_id} "
                    f"({i + 1}/{len(team_ids)})..."
                )
                try:
                    players = await self.client.get_squad_player_stats(
                        season_id, team_id
                    )

                    for player_data in players:
                        try:
                            # Guard: ensure player profile exists (FK constraint)
                            # Full upsert already ran in seed_players phase;
                            # this is a lightweight SELECT check for edge cases.
                            self._ensure_player_exists(player_data["id"], player_data)

                            # Extract stats for the target league/season
                            stats_list = player_data.get("statistics", [])
                            for stats in stats_list:
                                stat_season = stats.get("season", {})
                                stat_league = (
                                    stat_season.get("league", {}) if stat_season else {}
                                )

                                # Only process stats for our target league
                                if stat_league.get("id") != sportmonks_league_id:
                                    continue

                                self._upsert_player_stats(
                                    player_id=player_data["id"],
                                    team_id=stats.get("team_id") or team_id,
                                    details=stats.get("details", []),
                                    league_id=league_id,
                                    season_year=season_year,
                                    raw_data=stats,
                                )
                                result.player_stats_upserted += 1

                        except Exception as e:
                            pid = player_data.get("id", "?")
                            error_msg = f"Error upserting player stats for {pid}: {e}"
                            logger.warning(error_msg)
                            result.errors.append(error_msg)

                except Exception as e:
                    error_msg = f"Error fetching team {team_id} squad stats: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)

            logger.info(f"Upserted {result.player_stats_upserted} player stats")
        except Exception as e:
            error_msg = f"Error seeding player stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        return result

    def _upsert_player_stats(
        self,
        player_id: int,
        team_id: int,
        details: list[dict[str, Any]],
        league_id: int,
        season_year: int,
        raw_data: dict[str, Any] | None = None,
    ) -> None:
        """Upsert player statistics as JSONB into unified player_stats table."""
        stats_json = self._build_player_stats_json(details)

        self.db.execute(
            f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, sport, season, league_id, team_id,
                stats, raw_response
            ) VALUES (%s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
                team_id = EXCLUDED.team_id,
                stats = EXCLUDED.stats,
                raw_response = EXCLUDED.raw_response,
                updated_at = NOW()
        """,
            (
                player_id,
                SPORT,
                season_year,
                league_id,
                team_id,
                json.dumps(stats_json),
                json.dumps(raw_data) if raw_data else None,
            ),
        )

    # =========================================================================
    # Team Stats (Standings)
    # =========================================================================

    async def seed_team_stats(
        self,
        season_id: int,
        league_id: int,
        season_year: int,
    ) -> SeedResult:
        """Seed team standings/stats for a season."""
        logger.info(f"Seeding team stats for season {season_id}...")
        result = SeedResult()

        try:
            standings = await self.client.get_standings(season_id)
            logger.info(f"Fetched {len(standings)} standings entries")

            for standing in standings:
                try:
                    self._upsert_team_stats(
                        standing,
                        league_id,
                        season_year,
                    )
                    result.team_stats_upserted += 1
                except Exception as e:
                    participant = standing.get("participant", {})
                    name = participant.get("name", participant.get("id", "?"))
                    error_msg = f"Error upserting team stats for {name}: {e}"
                    logger.warning(error_msg)
                    result.errors.append(error_msg)

            logger.info(f"Upserted {result.team_stats_upserted} team stats")
        except Exception as e:
            error_msg = f"Error seeding team stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        return result

    def _upsert_team_stats(
        self,
        standing: dict[str, Any],
        league_id: int,
        season_year: int,
    ) -> None:
        """Upsert team standings as JSONB into unified team_stats table."""
        participant = standing.get("participant", {})
        team_id = participant.get("id") or standing.get("participant_id")

        if not team_id:
            return

        # Ensure team exists
        exists = self.db.fetchone(
            f"SELECT 1 FROM {TEAMS_TABLE} WHERE id = %s AND sport = %s",
            (team_id, SPORT),
        )
        if not exists:
            self._upsert_team(participant)

        # Extract W/D/L/GF/GA from standing details
        details = standing.get("details", [])
        detail_map: dict[int, Any] = {}
        for d in details:
            tid = d.get("type_id")
            if tid is not None:
                detail_map[tid] = d.get("value")

        stats_json: dict[str, Any] = {}

        # Map standing details to stats
        for stat_name, type_id in STANDING_DETAIL_TYPES.items():
            val = detail_map.get(type_id)
            if val is not None:
                stats_json[stat_name] = val

        # Add top-level fields from standing
        if standing.get("points") is not None:
            stats_json["points"] = standing["points"]
        if standing.get("position") is not None:
            stats_json["position"] = standing["position"]
        if standing.get("form"):
            stats_json["form"] = standing["form"]

        self.db.execute(
            f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, sport, season, league_id,
                stats, raw_response
            ) VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
                stats = EXCLUDED.stats,
                raw_response = EXCLUDED.raw_response,
                updated_at = NOW()
        """,
            (
                team_id,
                SPORT,
                season_year,
                league_id,
                json.dumps(stats_json),
                json.dumps(standing),
            ),
        )

    # =========================================================================
    # Full Season Seeding
    # =========================================================================

    async def seed_season(
        self,
        season_id: int,
        league_id: int,
        season_year: int,
        sportmonks_league_id: int | None = None,
    ) -> SeedResult:
        """Seed all data for a single season.

        Args:
            season_id: SportMonks season ID
            league_id: Our internal league ID (1-5)
            season_year: Year (e.g. 2024 for 2024-25 season)
            sportmonks_league_id: SportMonks league ID (auto-resolved if not provided)
        """
        if sportmonks_league_id is None:
            league_row = self.db.fetchone(
                "SELECT sportmonks_id, name FROM leagues WHERE id = %s",
                (league_id,),
            )
            if not league_row or not league_row["sportmonks_id"]:
                raise ValueError(f"No sportmonks_id found for league {league_id}")
            sportmonks_league_id = int(league_row["sportmonks_id"])
            league_name = league_row["name"]
        else:
            league_row = self.db.fetchone(
                "SELECT name FROM leagues WHERE id = %s",
                (league_id,),
            )
            league_name = league_row["name"] if league_row else f"League {league_id}"

        logger.info(
            f"Seeding season {season_id} "
            f"(league {league_id}: {league_name}, year {season_year})"
        )

        result = SeedResult()

        # 1. Seed teams
        logger.info("Phase 1/4: Seeding teams...")
        result = result + await self.seed_teams(season_id)

        # 2. Seed team stats (standings)
        logger.info("Phase 2/4: Seeding team stats (standings)...")
        result = result + await self.seed_team_stats(
            season_id,
            league_id,
            season_year,
        )

        # 3. Seed players (profiles via squad fetch + individual enrichment)
        logger.info("Phase 3/4: Seeding player profiles...")
        result = result + await self.seed_players(season_id)

        # 4. Seed player stats (full stat extraction)
        logger.info("Phase 4/4: Seeding player stats...")
        result = result + await self.seed_player_stats(
            season_id,
            league_id,
            season_year,
            sportmonks_league_id,
        )

        logger.info(
            f"Season seeding complete: "
            f"{result.teams_upserted} teams, "
            f"{result.players_upserted} players, "
            f"{result.team_stats_upserted} team stats, "
            f"{result.player_stats_upserted} player stats, "
            f"{len(result.errors)} errors"
        )

        return result
