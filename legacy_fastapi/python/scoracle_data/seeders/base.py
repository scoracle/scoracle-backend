"""
Base seed runner — shared DB upsert methods and generic seed orchestration.

All sports write the same canonical dict shapes to the same unified tables.
Handlers normalize provider-specific API responses before they reach here.

NBA and NFL use BaseSeedRunner directly (their orchestration is identical).
Football subclasses it in football.py for league/team iteration differences.
"""

from __future__ import annotations

import json
import logging
from typing import Any, TYPE_CHECKING

from ..core.types import PLAYERS_TABLE, PLAYER_STATS_TABLE, TEAMS_TABLE, TEAM_STATS_TABLE
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


class BaseSeedRunner:
    """Shared DB write methods and generic seed flow for all sports.

    Canonical dict shapes (produced by handlers):

        Team:  {"id", "name", "short_code", "city", "country", "conference",
                "division", "venue_name", "venue_capacity", "founded",
                "logo_url", "meta"}

        Player: {"id", "name", "first_name", "last_name", "position",
                 "detailed_position", "nationality", "height", "weight",
                 "date_of_birth", "photo_url", "team_id", "meta"}

        PlayerStats: {"player_id", "player" (dict), "stats" (flat dict), "raw" (dict)}

        TeamStats: {"team_id", "team" (dict|None), "stats" (flat dict), "raw" (dict)}
    """

    def __init__(self, db: "PostgresDB", handler: Any, sport: str):
        self.db = db
        self.handler = handler
        self.sport = sport

    # =========================================================================
    # DB Upsert Methods — identical for all sports
    # =========================================================================

    def _upsert_team(self, team: dict[str, Any]) -> None:
        """Write a canonical team dict to the teams table."""
        meta = team.get("meta") or {}
        self.db.execute(
            f"""
            INSERT INTO {TEAMS_TABLE} (
                id, sport, name, short_code, city, country, conference,
                division, venue_name, venue_capacity, founded, logo_url, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                short_code = EXCLUDED.short_code,
                city = EXCLUDED.city,
                country = EXCLUDED.country,
                conference = EXCLUDED.conference,
                division = EXCLUDED.division,
                venue_name = EXCLUDED.venue_name,
                venue_capacity = EXCLUDED.venue_capacity,
                founded = EXCLUDED.founded,
                logo_url = EXCLUDED.logo_url,
                meta = EXCLUDED.meta,
                updated_at = NOW()
            """,
            (
                team["id"],
                self.sport,
                team.get("name"),
                team.get("short_code"),
                team.get("city"),
                team.get("country"),
                team.get("conference"),
                team.get("division"),
                team.get("venue_name"),
                team.get("venue_capacity"),
                team.get("founded"),
                team.get("logo_url"),
                json.dumps(meta) if meta else "{}",
            ),
        )

    def _upsert_player(self, player: dict[str, Any]) -> None:
        """Write a canonical player dict to the players table."""
        meta = player.get("meta") or {}
        self.db.execute(
            f"""
            INSERT INTO {PLAYERS_TABLE} (
                id, sport, name, first_name, last_name, position,
                detailed_position, nationality, height, weight,
                date_of_birth, photo_url, team_id, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = COALESCE(EXCLUDED.name, {PLAYERS_TABLE}.name),
                first_name = COALESCE(EXCLUDED.first_name, {PLAYERS_TABLE}.first_name),
                last_name = COALESCE(EXCLUDED.last_name, {PLAYERS_TABLE}.last_name),
                position = COALESCE(EXCLUDED.position, {PLAYERS_TABLE}.position),
                detailed_position = COALESCE(EXCLUDED.detailed_position, {PLAYERS_TABLE}.detailed_position),
                nationality = COALESCE(EXCLUDED.nationality, {PLAYERS_TABLE}.nationality),
                height = COALESCE(EXCLUDED.height, {PLAYERS_TABLE}.height),
                weight = COALESCE(EXCLUDED.weight, {PLAYERS_TABLE}.weight),
                date_of_birth = COALESCE(EXCLUDED.date_of_birth, {PLAYERS_TABLE}.date_of_birth),
                photo_url = COALESCE(EXCLUDED.photo_url, {PLAYERS_TABLE}.photo_url),
                team_id = COALESCE(EXCLUDED.team_id, {PLAYERS_TABLE}.team_id),
                meta = COALESCE(EXCLUDED.meta, {PLAYERS_TABLE}.meta),
                updated_at = NOW()
            """,
            (
                player["id"],
                self.sport,
                player.get("name"),
                player.get("first_name"),
                player.get("last_name"),
                player.get("position"),
                player.get("detailed_position"),
                player.get("nationality"),
                player.get("height"),
                player.get("weight"),
                player.get("date_of_birth"),
                player.get("photo_url"),
                player.get("team_id"),
                json.dumps(meta) if meta else "{}",
            ),
        )

    def _upsert_player_stats(
        self,
        data: dict[str, Any],
        season: int,
        league_id: int = 0,
    ) -> None:
        """Write canonical player stats to player_stats table.

        Postgres triggers automatically compute derived stats (per-36, per-90,
        TS%, efficiency, td_int_ratio, catch_pct, etc.) on INSERT/UPDATE.
        """
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
                data["player_id"],
                self.sport,
                season,
                league_id,
                data.get("team_id"),
                json.dumps(data.get("stats", {})),
                json.dumps(data.get("raw", {})),
            ),
        )

    def _upsert_team_stats(
        self,
        data: dict[str, Any],
        season: int,
        league_id: int = 0,
    ) -> None:
        """Write canonical team stats to team_stats table.

        Postgres triggers automatically compute derived stats (win_pct, etc.).
        """
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
                data["team_id"],
                self.sport,
                season,
                league_id,
                json.dumps(data.get("stats", {})),
                json.dumps(data.get("raw", {})),
            ),
        )

    # =========================================================================
    # Generic Seed Flow (used by NBA/NFL directly)
    # =========================================================================

    async def seed_teams(self, **handler_kw: Any) -> SeedResult:
        """Fetch teams from handler and write to DB."""
        logger.info("Seeding %s teams...", self.sport)
        result = SeedResult()
        try:
            teams = await self.handler.get_teams(**handler_kw)
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info("Upserted %d teams", result.teams_upserted)
        except Exception as e:
            error_msg = f"Error seeding teams: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    async def seed_player_stats(
        self, season: int, league_id: int = 0, **handler_kw: Any
    ) -> SeedResult:
        """Fetch player stats from handler and write to DB."""
        logger.info("Seeding %s player stats for season %d...", self.sport, season)
        result = SeedResult()
        try:
            count = 0
            async for data in self.handler.get_player_stats(season, **handler_kw):
                try:
                    # Auto-upsert player profile if handler provides it
                    if data.get("player"):
                        self._upsert_player(data["player"])
                    self._upsert_player_stats(data, season, league_id)
                    result.player_stats_upserted += 1
                    count += 1
                    if count % 50 == 0:
                        logger.info("Processed %d player stats...", count)
                except Exception as e:
                    pid = data.get("player_id")
                    result.errors.append(f"Error upserting stats for player {pid}: {e}")
            logger.info("Upserted %d player stats", result.player_stats_upserted)
        except Exception as e:
            error_msg = f"Error seeding player stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    async def seed_team_stats(
        self, season: int, league_id: int = 0, **handler_kw: Any
    ) -> SeedResult:
        """Fetch team stats from handler and write to DB."""
        logger.info("Seeding %s team stats for season %d...", self.sport, season)
        result = SeedResult()
        try:
            team_stats = await self.handler.get_team_stats(season, **handler_kw)
            for data in team_stats:
                try:
                    self._upsert_team_stats(data, season, league_id)
                    result.team_stats_upserted += 1
                except Exception as e:
                    tid = data.get("team_id")
                    result.errors.append(f"Error upserting team stats for {tid}: {e}")
            logger.info("Upserted %d team stats", result.team_stats_upserted)
        except Exception as e:
            error_msg = f"Error seeding team stats: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    async def seed_all(self, season: int, **handler_kw: Any) -> SeedResult:
        """Full seed flow: teams -> player stats (+ profiles) -> team stats.

        Player profiles are upserted automatically during the player stats
        phase — each stats response embeds full profile data (height, weight,
        college, etc.), so a standalone player fetch is unnecessary.

        Used directly by NBA and NFL. Football overrides in FootballSeedRunner.
        """
        logger.info("Starting full %s seed for season %d", self.sport, season)
        result = SeedResult()
        result = result + await self.seed_teams()
        result = result + await self.seed_player_stats(season, **handler_kw)
        result = result + await self.seed_team_stats(season, **handler_kw)
        logger.info(
            "%s seed complete: %d teams, %d player stats, %d team stats, %d errors",
            self.sport,
            result.teams_upserted,
            result.player_stats_upserted,
            result.team_stats_upserted,
            len(result.errors),
        )
        return result
