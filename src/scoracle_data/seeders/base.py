"""Base seed runner — extracts common orchestration patterns shared by sport seeders.

NBA and NFL seeders share identical control flow for seed_teams() and
seed_players().  Football (Soccer) differs enough (extra parameters, different
iteration style) that it only inherits __init__ and the _ensure_player_exists
helper.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.

All sports use the unified tables (players, player_stats, teams, team_stats)
with a `sport` discriminator column.
"""

import json
import logging
from abc import ABC, abstractmethod
from typing import Any, TYPE_CHECKING

from ..core.types import PLAYERS_TABLE, TEAMS_TABLE
from .common import SeedResult

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


class BaseSeedRunner(ABC):
    """Shared base for all sport-specific seed runners.

    Provides:
    * ``__init__`` storing *db* and *client*
    * ``_ensure_player_exists`` — check-then-upsert guard used before
      writing player-stats rows.

    Sub-classes *must* define ``_upsert_player`` at minimum (called by
    the guard). NBA/NFL sub-classes get additional shared orchestration
    via ``AmericanSportsSeedRunner``.
    """

    def __init__(self, db: "PostgresDB", client: Any):
        self.db = db
        self.client = client

    # -- Abstract hooks subclasses must implement ----------------------------

    @abstractmethod
    def _upsert_team(self, team: dict[str, Any]) -> None: ...

    @abstractmethod
    def _upsert_player(self, player: dict[str, Any]) -> None: ...

    # -- Shared helpers ------------------------------------------------------

    def _ensure_player_exists(
        self,
        player_id: int | str,
        player_data: dict[str, Any],
    ) -> None:
        """Insert the player profile row if it doesn't already exist.

        Called from ``_upsert_player_stats`` in NBA / NFL / Football
        before writing a stats row that has a FK to the player table.
        Uses the unified players table with sport discriminator from the
        subclass's SPORT constant.
        """
        if not player_data:
            return
        # Get the sport from the subclass (SPORT module-level constant)
        sport = getattr(self, "_sport_label", None) or "UNKNOWN"
        exists = self.db.fetchone(
            f"SELECT 1 FROM {PLAYERS_TABLE} WHERE id = %s AND sport = %s",
            (player_id, sport),
        )
        if not exists:
            self._upsert_player(player_data)


class AmericanSportsSeedRunner(BaseSeedRunner):
    """Extended base for American sports (NBA, NFL).

    Provides shared ``_upsert_team``, ``_upsert_player``, ``seed_teams``,
    and ``seed_players`` implementations.  Subclasses only need to supply:

    * ``_sport_label`` property (e.g. ``"NBA"`` / ``"NFL"``)
    * ``_city_field`` — API key for team city (``"city"`` for NBA, ``"location"`` for NFL)
    * ``_player_meta_fields`` — list of (api_key, meta_key) tuples for sport-specific meta
    * Sport-specific stats methods (``seed_player_stats``, etc.)
    """

    # -- Abstract properties subclasses must implement -----------------------

    @property
    @abstractmethod
    def _sport_label(self) -> str:
        """Short label used in log messages and as sport discriminator."""
        ...

    @property
    def _city_field(self) -> str:
        """API response key for team city. Override in NFL ('location')."""
        return "city"

    @property
    def _player_meta_fields(self) -> list[tuple[str, str]]:
        """List of (api_key, meta_key) for sport-specific player meta.

        Override in subclass. Default is empty.
        """
        return []

    # -- Shared upsert: Teams ------------------------------------------------

    def _upsert_team(self, team: dict[str, Any]) -> None:
        """Upsert a BallDontLie team into the unified teams table.

        Writes conference/division to the typed columns (not meta JSONB).
        Only stores full_name in meta for display purposes.
        """
        meta = {}
        if team.get("full_name"):
            meta["full_name"] = team["full_name"]

        self.db.execute(
            f"""
            INSERT INTO {TEAMS_TABLE} (
                id, sport, name, short_code, city, conference, division, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                short_code = EXCLUDED.short_code,
                city = EXCLUDED.city,
                conference = EXCLUDED.conference,
                division = EXCLUDED.division,
                meta = EXCLUDED.meta,
                updated_at = NOW()
        """,
            (
                team["id"],
                self._sport_label,
                team.get("name"),
                team.get("abbreviation"),
                team.get(self._city_field),
                team.get("conference"),
                team.get("division"),
                json.dumps(meta) if meta else "{}",
            ),
        )

    # -- Shared upsert: Players ----------------------------------------------

    def _upsert_player(self, player: dict[str, Any]) -> None:
        """Upsert a BallDontLie player into the unified players table."""
        team = player.get("team") or {}
        team_id = team.get("id") if team else None

        name = f"{player.get('first_name', '')} {player.get('last_name', '')}".strip()
        if not name:
            name = f"Player {player['id']}"

        meta = {}
        for api_key, meta_key in self._player_meta_fields:
            val = player.get(api_key)
            if val is not None:
                meta[meta_key] = val

        self.db.execute(
            f"""
            INSERT INTO {PLAYERS_TABLE} (
                id, sport, name, first_name, last_name, position,
                height_cm, weight_kg, nationality, team_id, meta
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (id, sport) DO UPDATE SET
                name = EXCLUDED.name,
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                position = EXCLUDED.position,
                height_cm = EXCLUDED.height_cm,
                weight_kg = EXCLUDED.weight_kg,
                nationality = EXCLUDED.nationality,
                team_id = EXCLUDED.team_id,
                meta = EXCLUDED.meta,
                updated_at = NOW()
        """,
            (
                player["id"],
                self._sport_label,
                name,
                player.get("first_name"),
                player.get("last_name"),
                player.get("position"),
                player.get("height"),
                player.get("weight"),
                player.get("country"),
                team_id,
                json.dumps(meta) if meta else "{}",
            ),
        )

    # -- Teams orchestration -------------------------------------------------

    async def seed_teams(self) -> SeedResult:
        """Seed all teams — shared flow for BallDontLie sports."""
        sport = self._sport_label
        logger.info(f"Seeding {sport} teams...")
        result = SeedResult()
        try:
            teams = await self.client.get_teams()
            for team in teams:
                self._upsert_team(team)
                result.teams_upserted += 1
            logger.info(f"Upserted {result.teams_upserted} teams")
        except Exception as e:
            error_msg = f"Error seeding teams: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result

    # -- Players orchestration -----------------------------------------------

    async def seed_players(self) -> SeedResult:
        """Seed all players — shared flow for BallDontLie sports."""
        sport = self._sport_label
        logger.info(f"Seeding {sport} players...")
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
                    result.errors.append(
                        f"Error upserting player {player.get('id')}: {e}",
                    )
            logger.info(f"Upserted {result.players_upserted} players")
        except Exception as e:
            error_msg = f"Error seeding players: {e}"
            logger.error(error_msg)
            result.errors.append(error_msg)
        return result
