"""Football seeder — seeds Football (Soccer) data from SportMonks API into PostgreSQL.

Uses the canonical SportMonksClient provider and centralized table
names from core.types.SPORT_REGISTRY.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

import json
import logging
from datetime import datetime
from typing import Any, TYPE_CHECKING

from ..core.types import get_sport_config
from ..providers.sportmonks import SportMonksClient
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

_cfg = get_sport_config("FOOTBALL")
TEAM_TABLE = _cfg.team_profile_table
PLAYER_TABLE = _cfg.player_profile_table
PLAYER_STATS_TABLE = _cfg.player_stats_table
TEAM_STATS_TABLE = _cfg.team_stats_table

# Top 5 European Leagues with SportMonks IDs
LEAGUES = {
    1: {"sportmonks_id": 8, "name": "Premier League", "country": "England"},
    2: {"sportmonks_id": 564, "name": "La Liga", "country": "Spain"},
    3: {"sportmonks_id": 82, "name": "Bundesliga", "country": "Germany"},
    4: {"sportmonks_id": 384, "name": "Serie A", "country": "Italy"},
    5: {"sportmonks_id": 301, "name": "Ligue 1", "country": "France"},
}

# Season IDs for Premier League (from 2020 onwards)
PREMIER_LEAGUE_SEASONS = {
    2020: 17420,
    2021: 18378,
    2022: 19734,
    2023: 21646,
    2024: 23614,
    2025: 25583,
}


class FootballSeedRunner:
    """Seeds Football data from SportMonks into PostgreSQL via psycopg."""

    def __init__(self, db: "PostgresDB", client: SportMonksClient):
        self.db = db
        self.client = client

    # -- Teams ----------------------------------------------------------------

    async def seed_teams(self, season_id: int) -> SeedResult:
        logger.info(f"Seeding teams for season {season_id}...")
        result = SeedResult()
        try:
            teams = await self.client.get_teams_by_season(season_id)
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info(f"Upserted {result.teams_upserted} teams")
        except Exception as e:
            result.errors.append(f"Error seeding teams: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_team(self, team: dict[str, Any]) -> None:
        venue = team.get("venue") or {}
        self.db.execute(f"""
            INSERT INTO {TEAM_TABLE} (
                id, name, short_code, country, logo_url,
                venue_name, venue_capacity, founded
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, short_code = EXCLUDED.short_code,
                country = EXCLUDED.country, logo_url = EXCLUDED.logo_url,
                venue_name = EXCLUDED.venue_name, venue_capacity = EXCLUDED.venue_capacity,
                founded = EXCLUDED.founded, updated_at = NOW()
        """, (
            team["id"], team.get("name"), team.get("short_code"),
            team.get("country"), team.get("image_path"),
            venue.get("name"), venue.get("capacity"), team.get("founded"),
        ))

    # -- Players --------------------------------------------------------------

    async def seed_players(self, season_id: int, team_ids: list[int] | None = None) -> SeedResult:
        logger.info(f"Seeding players for season {season_id}...")
        result = SeedResult()
        try:
            if team_ids is None:
                teams = await self.client.get_teams_by_season(season_id)
                team_ids = [t["id"] for t in teams]

            for team_id in team_ids:
                try:
                    squad = await self.client.get_squad(season_id, team_id)
                    for player_data in squad:
                        player = player_data.get("player", player_data)
                        self._upsert_player(player)
                        result.players_upserted += 1
                except Exception as e:
                    result.errors.append(f"Error seeding squad for team {team_id}: {e}")
                    logger.warning(result.errors[-1])
            logger.info(f"Upserted {result.players_upserted} players")
        except Exception as e:
            result.errors.append(f"Error seeding players: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_player(self, player: dict[str, Any]) -> None:
        dob = None
        dob_str = player.get("date_of_birth")
        if dob_str:
            try:
                dob = datetime.strptime(dob_str, "%Y-%m-%d").date()
            except ValueError:
                pass

        self.db.execute(f"""
            INSERT INTO {PLAYER_TABLE} (
                id, common_name, first_name, last_name, display_name,
                nationality, nationality_id, position, detailed_position,
                position_id, height, weight, date_of_birth, image_url
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (id) DO UPDATE SET
                common_name = EXCLUDED.common_name, first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name, display_name = EXCLUDED.display_name,
                nationality = EXCLUDED.nationality, nationality_id = EXCLUDED.nationality_id,
                position = EXCLUDED.position, detailed_position = EXCLUDED.detailed_position,
                position_id = EXCLUDED.position_id, height = EXCLUDED.height,
                weight = EXCLUDED.weight, date_of_birth = EXCLUDED.date_of_birth,
                image_url = EXCLUDED.image_url, updated_at = NOW()
        """, (
            player["id"], player.get("common_name"), player.get("firstname"),
            player.get("lastname"), player.get("display_name"),
            player.get("nationality"), player.get("nationality_id"),
            player.get("position"), player.get("detailed_position"),
            player.get("position_id"), player.get("height"), player.get("weight"),
            dob, player.get("image_path"),
        ))

    # -- Player Stats ---------------------------------------------------------

    async def seed_player_stats(
        self, season_id: int, league_id: int, season_year: int,
    ) -> SeedResult:
        logger.info(f"Seeding player stats for season {season_id} (league {league_id})...")
        result = SeedResult()
        try:
            count = 0
            async for stats in self.client.iterate_season_player_statistics(season_id):
                try:
                    self._upsert_player_stats(stats, league_id, season_year, season_id)
                    result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info(f"Processed {count} player stats...")
                except Exception as e:
                    pid = stats.get("player_id")
                    result.errors.append(f"Error upserting stats for player {pid}: {e}")
                    logger.warning(result.errors[-1])
            logger.info(f"Upserted {result.player_stats_upserted} player stats")
        except Exception as e:
            result.errors.append(f"Error seeding player stats: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_player_stats(
        self, stats_data: dict[str, Any],
        league_id: int, season_year: int, season_id: int,
    ) -> None:
        player_id = stats_data.get("player_id")
        team_id = stats_data.get("team_id")
        if not player_id:
            return

        # Ensure player exists
        exists = self.db.fetchone(
            f"SELECT 1 FROM {PLAYER_TABLE} WHERE id = %s", (player_id,),
        )
        if not exists:
            player = stats_data.get("player", {})
            if player:
                self._upsert_player(player)

        raw_json = json.dumps(stats_data)

        # SportMonks stores stats in a "details" array keyed by type_id
        details = stats_data.get("details", [])

        def get_stat(type_id: int) -> int | None:
            for d in details:
                if d.get("type_id") == type_id:
                    val = d.get("value", {})
                    if isinstance(val, dict):
                        return val.get("total") or val.get("all")
                    return val
            return None

        appearances = get_stat(88) or get_stat(321)
        goals = get_stat(52)
        assists = get_stat(79)
        minutes = get_stat(119)

        self.db.execute(f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, team_id, league_id, season, sportmonks_season_id,
                appearances, minutes_played, goals, assists, raw_json
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (player_id, league_id, season) DO UPDATE SET
                team_id = EXCLUDED.team_id,
                sportmonks_season_id = EXCLUDED.sportmonks_season_id,
                appearances = EXCLUDED.appearances,
                minutes_played = EXCLUDED.minutes_played,
                goals = EXCLUDED.goals, assists = EXCLUDED.assists,
                raw_json = EXCLUDED.raw_json, updated_at = NOW()
        """, (
            player_id, team_id, league_id, season_year, season_id,
            appearances, minutes, goals, assists, raw_json,
        ))

    # -- Team Stats (Standings) -----------------------------------------------

    async def seed_team_stats(
        self, season_id: int, league_id: int, season_year: int,
    ) -> SeedResult:
        logger.info(f"Seeding team stats for season {season_id}...")
        result = SeedResult()
        try:
            standings = await self.client.get_standings(season_id)
            for standing in standings:
                try:
                    self._upsert_team_stats(standing, league_id, season_year, season_id)
                    result.team_stats_upserted += 1
                except Exception as e:
                    tid = standing.get("participant", {}).get("id")
                    result.errors.append(f"Error upserting team stats for {tid}: {e}")
                    logger.warning(result.errors[-1])
            logger.info(f"Upserted {result.team_stats_upserted} team stats")
        except Exception as e:
            result.errors.append(f"Error seeding team stats: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_team_stats(
        self, standing: dict[str, Any],
        league_id: int, season_year: int, season_id: int,
    ) -> None:
        participant = standing.get("participant", {})
        team_id = participant.get("id") or standing.get("participant_id")
        if not team_id:
            return
        raw_json = json.dumps(standing)
        self.db.execute(f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, league_id, season, sportmonks_season_id,
                wins, draws, losses, goals_for, goals_against,
                goal_difference, points, position, form, raw_json
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (team_id, league_id, season) DO UPDATE SET
                sportmonks_season_id = EXCLUDED.sportmonks_season_id,
                wins = EXCLUDED.wins, draws = EXCLUDED.draws, losses = EXCLUDED.losses,
                goals_for = EXCLUDED.goals_for, goals_against = EXCLUDED.goals_against,
                goal_difference = EXCLUDED.goal_difference, points = EXCLUDED.points,
                position = EXCLUDED.position, form = EXCLUDED.form,
                raw_json = EXCLUDED.raw_json, updated_at = NOW()
        """, (
            team_id, league_id, season_year, season_id,
            standing.get("won"), standing.get("draw"), standing.get("lost"),
            standing.get("goals_for"), standing.get("goals_against"),
            standing.get("goal_difference"), standing.get("points"),
            standing.get("position"), standing.get("form"), raw_json,
        ))

    # -- Full Seed ------------------------------------------------------------

    async def seed_season(
        self, season_id: int, league_id: int, season_year: int,
    ) -> SeedResult:
        logger.info(f"Seeding season {season_id} (league {league_id}, year {season_year})")
        result = SeedResult()
        result = result + await self.seed_teams(season_id)
        result = result + await self.seed_players(season_id)
        result = result + await self.seed_team_stats(season_id, league_id, season_year)
        result = result + await self.seed_player_stats(season_id, league_id, season_year)
        logger.info(
            f"Football seed complete: {result.teams_upserted} teams, "
            f"{result.players_upserted} players, "
            f"{result.team_stats_upserted} team stats, "
            f"{result.player_stats_upserted} player stats, "
            f"{len(result.errors)} errors"
        )
        return result
