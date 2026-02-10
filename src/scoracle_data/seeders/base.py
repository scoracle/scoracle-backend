"""Base seed runner — extracts common orchestration patterns shared by sport seeders.

NBA and NFL seeders share identical control flow for seed_teams() and
seed_players().  Football (Soccer) differs enough (extra parameters, different
iteration style) that it only inherits __init__ and the _ensure_player_exists
helper.

DB writes use psycopg (sync) via PostgresDB; API calls are async via httpx.

All sports use the unified tables (players, player_stats, teams, team_stats)
with a `sport` discriminator column.
"""

import logging
from abc import ABC, abstractmethod
from typing import Any, TYPE_CHECKING

from ..core.types import PLAYERS_TABLE
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
    via ``BallDontLieSeedRunner``.
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


class BallDontLieSeedRunner(BaseSeedRunner):
    """Extended base for BallDontLie-backed sports (NBA, NFL).

    Provides the truly identical ``seed_teams`` and ``seed_players``
    orchestration that both NBA and NFL share.  Sub-classes supply the
    sport-specific ``_upsert_*`` and ``_fetch_*`` methods.
    """

    # -- Teams ---------------------------------------------------------------

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

    # -- Players -------------------------------------------------------------

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

    # -- Abstract properties / hooks -----------------------------------------

    @property
    @abstractmethod
    def _sport_label(self) -> str:
        """Short label used in log messages, e.g. ``'NBA'`` or ``'NFL'``."""
        ...
