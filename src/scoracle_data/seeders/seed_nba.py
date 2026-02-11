"""NBA seeder — seeds NBA data from BallDontLie API into PostgreSQL.

Uses the canonical BallDontLieNBA provider client and unified tables
(players, player_stats, teams, team_stats) with JSONB stats.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.
"""

import json
import logging
from typing import Any, TYPE_CHECKING

from ..core.types import PLAYER_STATS_TABLE, TEAM_STATS_TABLE
from ..providers.balldontlie_nba import BallDontLieNBA
from .base import BallDontLieSeedRunner
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

SPORT = "NBA"


class NBASeedRunner(BallDontLieSeedRunner):
    """Seeds NBA data from BallDontLie into unified PostgreSQL tables."""

    def __init__(self, db: "PostgresDB", client: BallDontLieNBA):
        super().__init__(db, client)

    @property
    def _sport_label(self) -> str:
        return "NBA"

    @property
    def _player_meta_fields(self) -> list[tuple[str, str]]:
        return [
            ("jersey_number", "jersey_number"),
            ("college", "college"),
            ("draft_year", "draft_year"),
            ("draft_round", "draft_round"),
            ("draft_number", "draft_number"),
        ]

    # -- Player Stats ---------------------------------------------------------

    async def seed_player_stats(
        self, season: int, season_type: str = "regular"
    ) -> SeedResult:
        """Seed player season averages as JSONB."""
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
        self,
        stats_data: dict[str, Any],
        season: int,
        season_type: str,
    ) -> None:
        player = stats_data.get("player", {})
        player_id = player.get("id")
        if not player_id:
            return

        self._ensure_player_exists(player_id, player)

        s = stats_data.get("stats", {})
        stats_json = {
            "season_type": season_type,
            "games_played": s.get("gp"),
            "minutes": s.get("min"),
            "pts": s.get("pts"),
            "reb": s.get("reb"),
            "ast": s.get("ast"),
            "stl": s.get("stl"),
            "blk": s.get("blk"),
            "fg_pct": s.get("fg_pct"),
            "fg3_pct": s.get("fg3_pct"),
            "ft_pct": s.get("ft_pct"),
            "fgm": s.get("fgm"),
            "fga": s.get("fga"),
            "fg3m": s.get("fg3m"),
            "fg3a": s.get("fg3a"),
            "ftm": s.get("ftm"),
            "fta": s.get("fta"),
            "oreb": s.get("oreb"),
            "dreb": s.get("dreb"),
            "turnover": s.get("tov"),
            "pf": s.get("pf"),
            "plus_minus": s.get("plus_minus"),
        }
        # Remove None values for cleaner JSONB
        stats_json = {k: v for k, v in stats_json.items() if v is not None}

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
        self, season: int, season_type: str = "regular"
    ) -> SeedResult:
        """Seed team season stats as JSONB."""
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
        self,
        stats_data: dict[str, Any],
        season: int,
        season_type: str,
    ) -> None:
        team = stats_data.get("team", {})
        team_id = team.get("id")
        if not team_id:
            return

        s = stats_data.get("stats", {})
        stats_json = {
            "season_type": season_type,
            "wins": s.get("w"),
            "losses": s.get("l"),
            "games_played": s.get("gp"),
            "pts": s.get("pts"),
            "reb": s.get("reb"),
            "ast": s.get("ast"),
            "fg_pct": s.get("fg_pct"),
            "fg3_pct": s.get("fg3_pct"),
            "ft_pct": s.get("ft_pct"),
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
