"""NFL seeder — seeds NFL data from BallDontLie API into PostgreSQL.

Uses the canonical BallDontLieNFL provider client and centralized table
names from core.types.SPORT_REGISTRY.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

import json
import logging
from typing import Any, TYPE_CHECKING

from ..core.types import get_sport_config
from ..providers.balldontlie_nfl import BallDontLieNFL
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

_cfg = get_sport_config("NFL")
TEAM_TABLE = _cfg.team_profile_table
PLAYER_TABLE = _cfg.player_profile_table
PLAYER_STATS_TABLE = _cfg.player_stats_table
TEAM_STATS_TABLE = _cfg.team_stats_table


class NFLSeedRunner:
    """Seeds NFL data from BallDontLie into PostgreSQL via psycopg."""

    def __init__(self, db: "PostgresDB", client: BallDontLieNFL):
        self.db = db
        self.client = client

    # -- Teams ----------------------------------------------------------------

    async def seed_teams(self) -> SeedResult:
        logger.info("Seeding NFL teams...")
        result = SeedResult()
        try:
            teams = await self.client.get_teams()
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info(f"Upserted {result.teams_upserted} teams")
        except Exception as e:
            result.errors.append(f"Error seeding teams: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_team(self, team: dict[str, Any]) -> None:
        self.db.execute(f"""
            INSERT INTO {TEAM_TABLE} (
                id, name, full_name, abbreviation, location, conference, division
            ) VALUES (%s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, full_name = EXCLUDED.full_name,
                abbreviation = EXCLUDED.abbreviation, location = EXCLUDED.location,
                conference = EXCLUDED.conference, division = EXCLUDED.division,
                updated_at = NOW()
        """, (
            team["id"], team.get("name"), team.get("full_name"),
            team.get("abbreviation"), team.get("location"),
            team.get("conference"), team.get("division"),
        ))

    # -- Players --------------------------------------------------------------

    async def seed_players(self) -> SeedResult:
        logger.info("Seeding NFL players...")
        result = SeedResult()
        try:
            count = 0
            async for player in self.client.get_players():
                try:
                    self._upsert_player(player)
                    result.players_upserted += 1
                    count += 1
                    if count % 100 == 0:
                        logger.info(f"Processed {count} players...")
                except Exception as e:
                    result.errors.append(f"Error upserting player {player.get('id')}: {e}")
            logger.info(f"Upserted {result.players_upserted} players")
        except Exception as e:
            result.errors.append(f"Error seeding players: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_player(self, player: dict[str, Any]) -> None:
        team = player.get("team") or {}
        team_id = team.get("id") if team else None
        self.db.execute(f"""
            INSERT INTO {PLAYER_TABLE} (
                id, first_name, last_name, position, position_abbreviation,
                height, weight, jersey_number, college, experience, age, team_id
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (id) DO UPDATE SET
                first_name = EXCLUDED.first_name, last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                position_abbreviation = EXCLUDED.position_abbreviation,
                height = EXCLUDED.height, weight = EXCLUDED.weight,
                jersey_number = EXCLUDED.jersey_number, college = EXCLUDED.college,
                experience = EXCLUDED.experience, age = EXCLUDED.age,
                team_id = EXCLUDED.team_id, updated_at = NOW()
        """, (
            player["id"], player.get("first_name"), player.get("last_name"),
            player.get("position"), player.get("position_abbreviation"),
            player.get("height"), player.get("weight"),
            player.get("jersey_number"), player.get("college"),
            player.get("experience"), player.get("age"), team_id,
        ))

    # -- Player Stats ---------------------------------------------------------

    async def seed_player_stats(self, season: int, postseason: bool = False) -> SeedResult:
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Seeding NFL player stats for {season} {ps_label}...")
        result = SeedResult()
        try:
            count = 0
            async for stats in self.client.get_season_stats(season, postseason):
                try:
                    self._upsert_player_stats(stats, season, postseason)
                    result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info(f"Processed {count} player stats...")
                except Exception as e:
                    pid = stats.get("player", {}).get("id")
                    result.errors.append(f"Error upserting stats for player {pid}: {e}")
            logger.info(f"Upserted {result.player_stats_upserted} player stats")
        except Exception as e:
            result.errors.append(f"Error seeding player stats: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_player_stats(
        self, stats_data: dict[str, Any], season: int, postseason: bool,
    ) -> None:
        player = stats_data.get("player", {})
        player_id = player.get("id")
        if not player_id:
            return

        exists = self.db.fetchone(
            f"SELECT 1 FROM {PLAYER_TABLE} WHERE id = %s", (player_id,),
        )
        if not exists:
            self._upsert_player(player)

        raw_json = json.dumps(stats_data)

        self.db.execute(f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, season, postseason, games_played,
                passing_completions, passing_attempts, passing_yards,
                passing_touchdowns, passing_interceptions, passing_yards_per_game,
                passing_completion_pct, qbr,
                rushing_attempts, rushing_yards, rushing_touchdowns,
                rushing_yards_per_game, yards_per_rush_attempt, rushing_first_downs,
                receptions, receiving_yards, receiving_touchdowns,
                receiving_targets, receiving_yards_per_game, yards_per_reception,
                receiving_first_downs,
                total_tackles, solo_tackles, assist_tackles,
                defensive_sacks, defensive_sack_yards, defensive_interceptions,
                interception_touchdowns, fumbles_forced, fumbles_recovered,
                field_goal_attempts, field_goals_made, field_goal_pct,
                punts, punt_yards,
                kick_returns, kick_return_yards, kick_return_touchdowns,
                punt_returner_returns, punt_returner_return_yards, punt_return_touchdowns,
                raw_json
            ) VALUES (
                %s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,
                %s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,
                %s,%s,%s,%s,%s,%s,%s,%s,%s,
                %s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s
            )
            ON CONFLICT (player_id, season, postseason) DO UPDATE SET
                games_played = EXCLUDED.games_played,
                passing_completions = EXCLUDED.passing_completions,
                passing_attempts = EXCLUDED.passing_attempts,
                passing_yards = EXCLUDED.passing_yards,
                passing_touchdowns = EXCLUDED.passing_touchdowns,
                passing_interceptions = EXCLUDED.passing_interceptions,
                passing_yards_per_game = EXCLUDED.passing_yards_per_game,
                passing_completion_pct = EXCLUDED.passing_completion_pct,
                qbr = EXCLUDED.qbr,
                rushing_attempts = EXCLUDED.rushing_attempts,
                rushing_yards = EXCLUDED.rushing_yards,
                rushing_touchdowns = EXCLUDED.rushing_touchdowns,
                rushing_yards_per_game = EXCLUDED.rushing_yards_per_game,
                yards_per_rush_attempt = EXCLUDED.yards_per_rush_attempt,
                rushing_first_downs = EXCLUDED.rushing_first_downs,
                receptions = EXCLUDED.receptions,
                receiving_yards = EXCLUDED.receiving_yards,
                receiving_touchdowns = EXCLUDED.receiving_touchdowns,
                receiving_targets = EXCLUDED.receiving_targets,
                receiving_yards_per_game = EXCLUDED.receiving_yards_per_game,
                yards_per_reception = EXCLUDED.yards_per_reception,
                receiving_first_downs = EXCLUDED.receiving_first_downs,
                total_tackles = EXCLUDED.total_tackles,
                solo_tackles = EXCLUDED.solo_tackles,
                assist_tackles = EXCLUDED.assist_tackles,
                defensive_sacks = EXCLUDED.defensive_sacks,
                defensive_sack_yards = EXCLUDED.defensive_sack_yards,
                defensive_interceptions = EXCLUDED.defensive_interceptions,
                interception_touchdowns = EXCLUDED.interception_touchdowns,
                fumbles_forced = EXCLUDED.fumbles_forced,
                fumbles_recovered = EXCLUDED.fumbles_recovered,
                field_goal_attempts = EXCLUDED.field_goal_attempts,
                field_goals_made = EXCLUDED.field_goals_made,
                field_goal_pct = EXCLUDED.field_goal_pct,
                punts = EXCLUDED.punts, punt_yards = EXCLUDED.punt_yards,
                kick_returns = EXCLUDED.kick_returns,
                kick_return_yards = EXCLUDED.kick_return_yards,
                kick_return_touchdowns = EXCLUDED.kick_return_touchdowns,
                punt_returner_returns = EXCLUDED.punt_returner_returns,
                punt_returner_return_yards = EXCLUDED.punt_returner_return_yards,
                punt_return_touchdowns = EXCLUDED.punt_return_touchdowns,
                raw_json = EXCLUDED.raw_json, updated_at = NOW()
        """, (
            player_id, season, postseason, stats_data.get("games_played"),
            stats_data.get("passing_completions"), stats_data.get("passing_attempts"),
            stats_data.get("passing_yards"), stats_data.get("passing_touchdowns"),
            stats_data.get("passing_interceptions"), stats_data.get("passing_yards_per_game"),
            stats_data.get("passing_completion_pct"), stats_data.get("qbr"),
            stats_data.get("rushing_attempts"), stats_data.get("rushing_yards"),
            stats_data.get("rushing_touchdowns"), stats_data.get("rushing_yards_per_game"),
            stats_data.get("yards_per_rush_attempt"), stats_data.get("rushing_first_downs"),
            stats_data.get("receptions"), stats_data.get("receiving_yards"),
            stats_data.get("receiving_touchdowns"), stats_data.get("receiving_targets"),
            stats_data.get("receiving_yards_per_game"), stats_data.get("yards_per_reception"),
            stats_data.get("receiving_first_downs"),
            stats_data.get("total_tackles"), stats_data.get("solo_tackles"),
            stats_data.get("assist_tackles"), stats_data.get("defensive_sacks"),
            stats_data.get("defensive_sack_yards"), stats_data.get("defensive_interceptions"),
            stats_data.get("interception_touchdowns"), stats_data.get("fumbles_forced"),
            stats_data.get("fumbles_recovered"),
            stats_data.get("field_goal_attempts"), stats_data.get("field_goals_made"),
            stats_data.get("field_goal_pct"), stats_data.get("punts"), stats_data.get("punt_yards"),
            stats_data.get("kick_returns"), stats_data.get("kick_return_yards"),
            stats_data.get("kick_return_touchdowns"), stats_data.get("punt_returner_returns"),
            stats_data.get("punt_returner_return_yards"), stats_data.get("punt_return_touchdowns"),
            raw_json,
        ))

    # -- Team Stats -----------------------------------------------------------

    async def seed_team_stats(self, season: int, postseason: bool = False) -> SeedResult:
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Seeding NFL team stats for {season} {ps_label}...")
        result = SeedResult()
        try:
            stats_list = await self.client.get_team_season_stats(season, postseason)
            for stats in stats_list:
                try:
                    self._upsert_team_stats(stats, season, postseason)
                    result.team_stats_upserted += 1
                except Exception as e:
                    tid = stats.get("team", {}).get("id")
                    result.errors.append(f"Error upserting team stats for {tid}: {e}")
            logger.info(f"Upserted {result.team_stats_upserted} team stats")
        except Exception as e:
            result.errors.append(f"Error seeding team stats: {e}")
            logger.error(result.errors[-1])
        return result

    def _upsert_team_stats(
        self, stats_data: dict[str, Any], season: int, postseason: bool,
    ) -> None:
        team = stats_data.get("team", {})
        team_id = team.get("id")
        if not team_id:
            return
        raw_json = json.dumps(stats_data)
        self.db.execute(f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, season, postseason, wins, losses, ties,
                points_for, points_against, point_differential, raw_json
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (team_id, season, postseason) DO UPDATE SET
                wins = EXCLUDED.wins, losses = EXCLUDED.losses, ties = EXCLUDED.ties,
                points_for = EXCLUDED.points_for, points_against = EXCLUDED.points_against,
                point_differential = EXCLUDED.point_differential,
                raw_json = EXCLUDED.raw_json, updated_at = NOW()
        """, (
            team_id, season, postseason,
            stats_data.get("wins"), stats_data.get("losses"), stats_data.get("ties"),
            stats_data.get("points_for"), stats_data.get("points_against"),
            stats_data.get("point_differential"), raw_json,
        ))

    # -- Full Seed ------------------------------------------------------------

    async def seed_all(self, season: int, postseason: bool = False) -> SeedResult:
        ps_label = "postseason" if postseason else "regular"
        logger.info(f"Starting full NFL seed for {season} {ps_label}")
        result = SeedResult()
        result = result + await self.seed_teams()
        result = result + await self.seed_players()
        result = result + await self.seed_player_stats(season, postseason)
        result = result + await self.seed_team_stats(season, postseason)
        logger.info(
            f"NFL seed complete: {result.teams_upserted} teams, "
            f"{result.players_upserted} players, "
            f"{result.player_stats_upserted} player stats, "
            f"{result.team_stats_upserted} team stats, "
            f"{len(result.errors)} errors"
        )
        return result
