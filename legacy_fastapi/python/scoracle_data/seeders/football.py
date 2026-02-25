"""
Football seed runner — league/team iteration for SportMonks data.

NBA/NFL use BaseSeedRunner directly (identical flow: teams -> players -> stats).
Football differs because:
  - Data is organized per-league (Premier League, La Liga, etc.)
  - Player stats require squad-by-squad iteration (bulk endpoint empty on our tier)
  - Season IDs must be resolved from provider_seasons table

All API calls and normalization are handled by SportMonksHandler.
This file only orchestrates the iteration order and DB writes.
"""

from __future__ import annotations

import logging
from typing import Any, TYPE_CHECKING

from .base import BaseSeedRunner
from .common import SeedResult

if TYPE_CHECKING:
    from ..handlers.sportmonks import SportMonksHandler
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

SPORT = "FOOTBALL"


class FootballSeedRunner(BaseSeedRunner):
    """Seeds Football data with per-league, per-team iteration.

    Unlike NBA/NFL (which use BaseSeedRunner.seed_all directly),
    Football requires seed_season() which resolves SportMonks IDs,
    iterates teams for squad-based player stats, and handles standings.
    """

    def __init__(self, db: "PostgresDB", handler: "SportMonksHandler"):
        super().__init__(db, handler, SPORT)

    async def seed_season(
        self,
        sm_season_id: int,
        league_id: int,
        season_year: int,
        sportmonks_league_id: int | None = None,
    ) -> SeedResult:
        """Seed all data for a single Football league-season.

        Args:
            sm_season_id: SportMonks season ID (from provider_seasons table).
            league_id: Our internal league ID (1-5).
            season_year: Year (e.g. 2024 for 2024-25 season).
            sportmonks_league_id: SportMonks league ID (auto-resolved if not provided).
        """
        # Resolve sportmonks_league_id if not provided
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
            "Seeding season %d (league %d: %s, year %d)",
            sm_season_id, league_id, league_name, season_year,
        )

        result = SeedResult()
        teams: list[dict] = []

        # 1. Teams
        logger.info("Phase 1/3: Seeding teams...")
        try:
            teams = await self.handler.get_teams(sm_season_id)
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info("Upserted %d teams", result.teams_upserted)
        except Exception as e:
            error_msg = f"Error seeding teams: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        # 2. Players + Player Stats (fetched together via squad iteration)
        logger.info("Phase 2/3: Seeding players + player stats...")
        try:
            team_ids = [t["id"] for t in teams] if result.teams_upserted > 0 else []
            count = 0
            async for data in self.handler.get_players_with_stats(
                sm_season_id, team_ids, sportmonks_league_id
            ):
                try:
                    # Upsert player profile
                    if data.get("player"):
                        self._upsert_player(data["player"])
                        result.players_upserted += 1
                    # Upsert player stats
                    if data.get("stats"):
                        self._upsert_player_stats(data, season_year, league_id)
                        result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info("Processed %d players...", count)
                except Exception as e:
                    pid = data.get("player_id", "?")
                    result.errors.append(f"Error upserting player {pid}: {e}")
        except Exception as e:
            error_msg = f"Error seeding players/stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        # 3. Team Stats (Standings)
        logger.info("Phase 3/3: Seeding team stats (standings)...")
        try:
            team_stats = await self.handler.get_team_stats(sm_season_id)
            for data in team_stats:
                try:
                    # Auto-upsert team if handler provides it
                    if data.get("team"):
                        self._upsert_team(data["team"])
                    self._upsert_team_stats(data, season_year, league_id)
                    result.team_stats_upserted += 1
                except Exception as e:
                    tid = data.get("team_id", "?")
                    result.errors.append(f"Error upserting team stats for {tid}: {e}")
            logger.info("Upserted %d team stats", result.team_stats_upserted)
        except Exception as e:
            error_msg = f"Error seeding team stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)

        logger.info(
            "Season seeding complete: %d teams, %d players, "
            "%d player stats, %d team stats, %d errors",
            result.teams_upserted,
            result.players_upserted,
            result.player_stats_upserted,
            result.team_stats_upserted,
            len(result.errors),
        )

        return result
