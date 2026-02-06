"""NBA seeder — seeds NBA data from BallDontLie API into PostgreSQL.

Uses the canonical BallDontLieNBA provider client and centralized table
names from core.types.SPORT_REGISTRY.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

import json
import logging
from typing import Any, TYPE_CHECKING

from ..core.types import get_sport_config
from ..providers.balldontlie_nba import BallDontLieNBA
from .base import BallDontLieSeedRunner
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

_cfg = get_sport_config("NBA")
TEAM_TABLE = _cfg.team_profile_table
PLAYER_TABLE = _cfg.player_profile_table
PLAYER_STATS_TABLE = _cfg.player_stats_table
TEAM_STATS_TABLE = _cfg.team_stats_table


class NBASeedRunner(BallDontLieSeedRunner):
    """Seeds NBA data from BallDontLie into PostgreSQL via psycopg."""

    def __init__(self, db: "PostgresDB", client: BallDontLieNBA):
        super().__init__(db, client)

    @property
    def _sport_label(self) -> str:
        return "NBA"

    # -- Upsert: Teams -------------------------------------------------------

    def _upsert_team(self, team: dict[str, Any]) -> None:
        self.db.execute(f"""
            INSERT INTO {TEAM_TABLE} (
                id, name, full_name, abbreviation, city, conference, division
            ) VALUES (%s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                full_name = EXCLUDED.full_name,
                abbreviation = EXCLUDED.abbreviation,
                city = EXCLUDED.city,
                conference = EXCLUDED.conference,
                division = EXCLUDED.division,
                updated_at = NOW()
        """, (
            team["id"],
            team.get("name"),
            team.get("full_name"),
            team.get("abbreviation"),
            team.get("city"),
            team.get("conference"),
            team.get("division"),
        ))

    # -- Upsert: Players -----------------------------------------------------

    def _upsert_player(self, player: dict[str, Any]) -> None:
        team = player.get("team") or {}
        team_id = team.get("id") if team else None
        self.db.execute(f"""
            INSERT INTO {PLAYER_TABLE} (
                id, first_name, last_name, position, height, weight,
                jersey_number, college, country, draft_year, draft_round,
                draft_number, team_id
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id) DO UPDATE SET
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                height = EXCLUDED.height,
                weight = EXCLUDED.weight,
                jersey_number = EXCLUDED.jersey_number,
                college = EXCLUDED.college,
                country = EXCLUDED.country,
                draft_year = EXCLUDED.draft_year,
                draft_round = EXCLUDED.draft_round,
                draft_number = EXCLUDED.draft_number,
                team_id = EXCLUDED.team_id,
                updated_at = NOW()
        """, (
            player["id"],
            player.get("first_name"),
            player.get("last_name"),
            player.get("position"),
            player.get("height"),
            player.get("weight"),
            player.get("jersey_number"),
            player.get("college"),
            player.get("country"),
            player.get("draft_year"),
            player.get("draft_round"),
            player.get("draft_number"),
            team_id,
        ))

    # -- Player Stats ---------------------------------------------------------

    async def seed_player_stats(self, season: int, season_type: str = "regular") -> SeedResult:
        """Seed player season averages."""
        logger.info(f"Seeding NBA player stats for {season} {season_type}...")
        result = SeedResult()
        try:
            count = 0
            async for stats in self.client.get_all_season_averages(season, season_type):
                try:
                    self._upsert_player_stats(stats, season, season_type)
                    result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info(f"Processed {count} player stats...")
                except Exception as e:
                    pid = stats.get("player", {}).get("id")
                    result.errors.append(f"Error upserting stats for player {pid}: {e}")
            logger.info(f"Upserted {result.player_stats_upserted} player stats")
        except Exception as e:
            error_msg = f"Error seeding player stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    def _upsert_player_stats(
        self, stats_data: dict[str, Any], season: int, season_type: str,
    ) -> None:
        player = stats_data.get("player", {})
        player_id = player.get("id")
        if not player_id:
            return

        self._ensure_player_exists(PLAYER_TABLE, player_id, player)

        s = stats_data.get("stats", {})
        raw_json = json.dumps(stats_data)

        self.db.execute(f"""
            INSERT INTO {PLAYER_STATS_TABLE} (
                player_id, season, season_type, games_played, minutes,
                pts, reb, ast, stl, blk,
                fg_pct, fg3_pct, ft_pct, fgm, fga,
                fg3m, fg3a, ftm, fta, oreb, dreb,
                turnover, pf, plus_minus, raw_json
            ) VALUES (
                %s,%s,%s,%s,%s,%s,%s,%s,%s,%s,
                %s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,
                %s,%s,%s,%s
            )
            ON CONFLICT (player_id, season, season_type) DO UPDATE SET
                games_played = EXCLUDED.games_played, minutes = EXCLUDED.minutes,
                pts = EXCLUDED.pts, reb = EXCLUDED.reb, ast = EXCLUDED.ast,
                stl = EXCLUDED.stl, blk = EXCLUDED.blk,
                fg_pct = EXCLUDED.fg_pct, fg3_pct = EXCLUDED.fg3_pct, ft_pct = EXCLUDED.ft_pct,
                fgm = EXCLUDED.fgm, fga = EXCLUDED.fga,
                fg3m = EXCLUDED.fg3m, fg3a = EXCLUDED.fg3a,
                ftm = EXCLUDED.ftm, fta = EXCLUDED.fta,
                oreb = EXCLUDED.oreb, dreb = EXCLUDED.dreb,
                turnover = EXCLUDED.turnover, pf = EXCLUDED.pf,
                plus_minus = EXCLUDED.plus_minus, raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """, (
            player_id, season, season_type,
            s.get("gp"), s.get("min"),
            s.get("pts"), s.get("reb"), s.get("ast"), s.get("stl"), s.get("blk"),
            s.get("fg_pct"), s.get("fg3_pct"), s.get("ft_pct"),
            s.get("fgm"), s.get("fga"), s.get("fg3m"), s.get("fg3a"),
            s.get("ftm"), s.get("fta"), s.get("oreb"), s.get("dreb"),
            s.get("tov"), s.get("pf"), s.get("plus_minus"),
            raw_json,
        ))

    # -- Team Stats -----------------------------------------------------------

    async def seed_team_stats(self, season: int, season_type: str = "regular") -> SeedResult:
        """Seed team season stats."""
        logger.info(f"Seeding NBA team stats for {season} {season_type}...")
        result = SeedResult()
        try:
            stats_list = await self.client.get_team_season_averages(season, season_type)
            for stats in stats_list:
                try:
                    self._upsert_team_stats(stats, season, season_type)
                    result.team_stats_upserted += 1
                except Exception as e:
                    tid = stats.get("team", {}).get("id")
                    result.errors.append(f"Error upserting team stats for {tid}: {e}")
            logger.info(f"Upserted {result.team_stats_upserted} team stats")
        except Exception as e:
            error_msg = f"Error seeding team stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    def _upsert_team_stats(
        self, stats_data: dict[str, Any], season: int, season_type: str,
    ) -> None:
        team = stats_data.get("team", {})
        team_id = team.get("id")
        if not team_id:
            return
        s = stats_data.get("stats", {})
        raw_json = json.dumps(stats_data)
        self.db.execute(f"""
            INSERT INTO {TEAM_STATS_TABLE} (
                team_id, season, season_type, wins, losses, games_played,
                pts, reb, ast, fg_pct, fg3_pct, ft_pct, raw_json
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (team_id, season, season_type) DO UPDATE SET
                wins = EXCLUDED.wins, losses = EXCLUDED.losses,
                games_played = EXCLUDED.games_played,
                pts = EXCLUDED.pts, reb = EXCLUDED.reb, ast = EXCLUDED.ast,
                fg_pct = EXCLUDED.fg_pct, fg3_pct = EXCLUDED.fg3_pct,
                ft_pct = EXCLUDED.ft_pct, raw_json = EXCLUDED.raw_json,
                updated_at = NOW()
        """, (
            team_id, season, season_type,
            s.get("w"), s.get("l"), s.get("gp"),
            s.get("pts"), s.get("reb"), s.get("ast"),
            s.get("fg_pct"), s.get("fg3_pct"), s.get("ft_pct"),
            raw_json,
        ))

    # -- Full Seed ------------------------------------------------------------

    async def seed_all(self, season: int, season_type: str = "regular") -> SeedResult:
        """Full seeding workflow for a season."""
        logger.info(f"Starting full NBA seed for {season} {season_type}")
        result = SeedResult()
        result = result + await self.seed_teams()
        result = result + await self.seed_players()
        result = result + await self.seed_player_stats(season, season_type)
        result = result + await self.seed_team_stats(season, season_type)
        logger.info(
            f"NBA seed complete: {result.teams_upserted} teams, "
            f"{result.players_upserted} players, "
            f"{result.player_stats_upserted} player stats, "
            f"{result.team_stats_upserted} team stats, "
            f"{len(result.errors)} errors"
        )
        return result
