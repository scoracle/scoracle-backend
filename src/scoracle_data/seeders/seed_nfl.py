"""NFL seeder — seeds NFL data from BallDontLie API into PostgreSQL.

Uses the canonical BallDontLieNFL provider client and unified tables
(players, player_stats, teams, team_stats) with JSONB stats.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

import json
import logging
from typing import Any, TYPE_CHECKING

from ..core.types import (
    PLAYERS_TABLE,
    PLAYER_STATS_TABLE,
    TEAMS_TABLE,
    TEAM_STATS_TABLE,
)
from ..providers.balldontlie_nfl import BallDontLieNFL
from .base import BallDontLieSeedRunner
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

SPORT = "NFL"


class NFLSeedRunner(BallDontLieSeedRunner):
    """Seeds NFL data from BallDontLie into unified PostgreSQL tables."""

    def __init__(self, db: "PostgresDB", client: BallDontLieNFL):
        super().__init__(db, client)

    @property
    def _sport_label(self) -> str:
        return "NFL"

    # -- Upsert: Teams -------------------------------------------------------

    def _upsert_team(self, team: dict[str, Any]) -> None:
        meta = {}
        if team.get("conference"):
            meta["conference"] = team["conference"]
        if team.get("division"):
            meta["division"] = team["division"]
        if team.get("full_name"):
            meta["full_name"] = team["full_name"]

        self.db.execute(
            f"""
            INSERT INTO {TEAMS_TABLE} (
                id, sport, name, short_code, city, meta
            ) VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                short_code = EXCLUDED.short_code,
                city = EXCLUDED.city,
                meta = EXCLUDED.meta,
                updated_at = NOW()
        """,
            (
                team["id"],
                SPORT,
                team.get("name"),
                team.get("abbreviation"),
                team.get("location"),
                json.dumps(meta),
            ),
        )

    # -- Upsert: Players -----------------------------------------------------

    def _upsert_player(self, player: dict[str, Any]) -> None:
        team = player.get("team") or {}
        team_id = team.get("id") if team else None

        name = f"{player.get('first_name', '')} {player.get('last_name', '')}".strip()
        if not name:
            name = f"Player {player['id']}"

        meta = {}
        if player.get("position_abbreviation"):
            meta["position_abbreviation"] = player["position_abbreviation"]
        if player.get("jersey_number"):
            meta["jersey_number"] = player["jersey_number"]
        if player.get("college"):
            meta["college"] = player["college"]
        if player.get("experience") is not None:
            meta["experience"] = player["experience"]
        if player.get("age") is not None:
            meta["age"] = player["age"]

        self.db.execute(
            f"""
            INSERT INTO {PLAYERS_TABLE} (
                id, sport, name, first_name, last_name, position,
                height_cm, weight_kg, team_id, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                height_cm = EXCLUDED.height_cm,
                weight_kg = EXCLUDED.weight_kg,
                team_id = EXCLUDED.team_id,
                meta = EXCLUDED.meta,
                updated_at = NOW()
        """,
            (
                player["id"],
                SPORT,
                name,
                player.get("first_name"),
                player.get("last_name"),
                player.get("position"),
                player.get("height"),  # May need conversion from BDL format
                player.get("weight"),  # May need conversion from lbs
                team_id,
                json.dumps(meta) if meta else "{}",
            ),
        )

    # -- Player Stats ---------------------------------------------------------

    async def seed_player_stats(
        self, season: int, postseason: bool = False
    ) -> SeedResult:
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
        self,
        stats_data: dict[str, Any],
        season: int,
        postseason: bool,
    ) -> None:
        player = stats_data.get("player", {})
        player_id = player.get("id")
        if not player_id:
            return

        self._ensure_player_exists(player_id, player)

        # Build stats JSONB from the flat stats_data keys
        stats_json: dict[str, Any] = {"postseason": postseason}

        # Map all available stat fields into the JSONB
        stat_fields = [
            "games_played",
            # Passing
            "passing_completions",
            "passing_attempts",
            "passing_yards",
            "passing_touchdowns",
            "passing_interceptions",
            "passing_yards_per_game",
            "passing_completion_pct",
            "qbr",
            # Rushing
            "rushing_attempts",
            "rushing_yards",
            "rushing_touchdowns",
            "rushing_yards_per_game",
            "yards_per_rush_attempt",
            "rushing_first_downs",
            # Receiving
            "receptions",
            "receiving_yards",
            "receiving_touchdowns",
            "receiving_targets",
            "receiving_yards_per_game",
            "yards_per_reception",
            "receiving_first_downs",
            # Defense
            "total_tackles",
            "solo_tackles",
            "assist_tackles",
            "defensive_sacks",
            "defensive_sack_yards",
            "defensive_interceptions",
            "interception_touchdowns",
            "fumbles_forced",
            "fumbles_recovered",
            # Kicking
            "field_goal_attempts",
            "field_goals_made",
            "field_goal_pct",
            # Punting
            "punts",
            "punt_yards",
            # Returns
            "kick_returns",
            "kick_return_yards",
            "kick_return_touchdowns",
            "punt_returner_returns",
            "punt_returner_return_yards",
            "punt_return_touchdowns",
        ]

        for field in stat_fields:
            val = stats_data.get(field)
            if val is not None:
                stats_json[field] = val

        self.db.execute(
            f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, sport, season, league_id, stats, raw_response
            ) VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
                stats = EXCLUDED.stats,
                raw_response = EXCLUDED.raw_response,
                updated_at = NOW()
        """,
            (
                player_id,
                SPORT,
                season,
                0,
                json.dumps(stats_json),
                json.dumps(stats_data),
            ),
        )

    # -- Team Stats -----------------------------------------------------------

    async def seed_team_stats(
        self, season: int, postseason: bool = False
    ) -> SeedResult:
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
        self,
        stats_data: dict[str, Any],
        season: int,
        postseason: bool,
    ) -> None:
        team = stats_data.get("team", {})
        team_id = team.get("id")
        if not team_id:
            return

        stats_json = {
            "postseason": postseason,
            "wins": stats_data.get("wins"),
            "losses": stats_data.get("losses"),
            "ties": stats_data.get("ties"),
            "points_for": stats_data.get("points_for"),
            "points_against": stats_data.get("points_against"),
            "point_differential": stats_data.get("point_differential"),
        }
        stats_json = {k: v for k, v in stats_json.items() if v is not None}

        self.db.execute(
            f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, sport, season, league_id, stats, raw_response
            ) VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
                stats = EXCLUDED.stats,
                raw_response = EXCLUDED.raw_response,
                updated_at = NOW()
        """,
            (
                team_id,
                SPORT,
                season,
                0,
                json.dumps(stats_json),
                json.dumps(stats_data),
            ),
        )

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
