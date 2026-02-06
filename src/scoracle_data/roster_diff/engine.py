"""
RosterDiffEngine: Detects roster changes (trades, transfers, new players).

This engine runs daily during the season to catch player movements between
teams, ensuring the local database stays in sync with the latest rosters.

Features:
- Compares API roster snapshot with local database
- Detects new players (need profile fetch)
- Detects team changes (trades/transfers)
- Detects departures (players no longer on any roster)
- Updates player's current team in sport-specific profile table
- Minimal API calls (one roster fetch per league/sport)
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional

from psycopg import sql

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)

from ..core.types import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

# Hardcoded priority Football leagues for run_all_priority_diffs
_PRIORITY_FOOTBALL_LEAGUES: list[int] = [
    39,   # Premier League (England)
    140,  # La Liga (Spain)
    135,  # Serie A (Italy)
    78,   # Bundesliga (Germany)
    61,   # Ligue 1 (France)
]


@dataclass
class DiffResult:
    """Result of a roster diff operation."""

    sport_id: str
    league_id: Optional[int] = None
    season_year: int = 0

    # Player changes
    new_players: list[int] = field(default_factory=list)
    transferred_players: list[tuple[int, int, int]] = field(default_factory=list)  # [(player_id, from_team, to_team)]
    departed_players: list[int] = field(default_factory=list)

    # Team changes
    new_teams: list[int] = field(default_factory=list)

    # Counts
    total_players_checked: int = 0
    total_teams_checked: int = 0

    # Timing
    started_at: int = 0
    completed_at: int = 0

    @property
    def has_changes(self) -> bool:
        """Whether any roster changes were detected."""
        return bool(
            self.new_players
            or self.transferred_players
            or self.departed_players
            or self.new_teams
        )

    @property
    def duration_seconds(self) -> float:
        """Duration of the diff operation in seconds."""
        if self.completed_at and self.started_at:
            return self.completed_at - self.started_at
        return 0.0

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for logging/serialization."""
        return {
            "sport_id": self.sport_id,
            "league_id": self.league_id,
            "season_year": self.season_year,
            "new_players": len(self.new_players),
            "transferred_players": len(self.transferred_players),
            "departed_players": len(self.departed_players),
            "new_teams": len(self.new_teams),
            "total_players_checked": self.total_players_checked,
            "total_teams_checked": self.total_teams_checked,
            "duration_seconds": self.duration_seconds,
        }


class RosterDiffEngine:
    """
    Engine for detecting roster changes between API and local database.

    Usage:
        engine = RosterDiffEngine(db, provider_client)
        result = await engine.run_diff("FOOTBALL", league_id=39, season=2024)

        if result.new_players:
            # Queue profile fetches for new players
            pass

        if result.transferred_players:
            # Log transfers
            pass
    """

    def __init__(
        self,
        db: "PostgresDB",
        api: Any,
    ):
        """
        Initialize the RosterDiffEngine.

        Args:
            db: PostgresDB connection (Neon / psycopg pool).
            api: Provider client — one of BallDontLieNBA, BallDontLieNFL,
                 or SportMonksClient.
        """
        self.db = db
        self.api = api

    async def run_diff(
        self,
        sport_id: str,
        season: int,
        league_id: Optional[int] = None,
    ) -> DiffResult:
        """
        Run a roster diff for a sport/league.

        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            season: Season year
            league_id: Optional league ID (required for FOOTBALL)

        Returns:
            DiffResult with detected changes
        """
        result = DiffResult(
            sport_id=sport_id,
            league_id=league_id,
            season_year=season,
            started_at=int(time.time()),
        )

        try:
            # Get current season ID
            season_id = self.db.get_season_id(sport_id, season)
            if not season_id:
                logger.warning("No season found for %s %d", sport_id, season)
                result.completed_at = int(time.time())
                return result

            # Fetch current roster from API
            api_players = await self._fetch_api_roster(sport_id, season_id, league_id)
            result.total_players_checked = len(api_players)

            # Build lookup map: player_id -> team_id
            api_roster_map = {
                p["id"]: p.get("current_team_id")
                for p in api_players
            }

            # Get existing players from database
            db_players = self._get_db_players(sport_id, league_id)
            db_roster_map = {
                p["id"]: p.get("current_team_id")
                for p in db_players
            }

            # Detect new players
            for player_id in api_roster_map:
                if player_id not in db_roster_map:
                    result.new_players.append(player_id)
                    # Insert basic player record (profile to be fetched later)
                    player_data = next(
                        (p for p in api_players if p["id"] == player_id),
                        None
                    )
                    if player_data:
                        self._insert_player(player_data, sport_id, league_id)

            # Detect transfers (team changes)
            for player_id, api_team_id in api_roster_map.items():
                if player_id in db_roster_map:
                    db_team_id = db_roster_map[player_id]
                    if api_team_id != db_team_id:
                        result.transferred_players.append(
                            (player_id, db_team_id or 0, api_team_id or 0)
                        )
                        # Update player's current team in profile table
                        self._record_transfer(
                            player_id,
                            to_team_id=api_team_id,
                            sport_id=sport_id,
                        )

            # Detect departures (players no longer in API roster)
            for player_id in db_roster_map:
                if player_id not in api_roster_map:
                    # Only mark as departed if they were active
                    player = self.db.get_player(player_id, sport_id)
                    if player and player.get("is_active"):
                        result.departed_players.append(player_id)
                        # Note: We don't deactivate players automatically
                        # They might just be on injury reserve or similar

            # Check for new teams as well
            api_teams = await self._fetch_api_teams(sport_id, season_id, league_id)
            result.total_teams_checked = len(api_teams)

            db_teams = self._get_db_teams(sport_id, league_id)
            db_team_ids = {t["id"] for t in db_teams}

            for team in api_teams:
                if team["id"] not in db_team_ids:
                    result.new_teams.append(team["id"])
                    self._insert_team(team, sport_id, league_id)

            result.completed_at = int(time.time())

            # Log summary
            if result.has_changes:
                logger.info(
                    "Roster diff for %s (league=%s, season=%d): "
                    "%d new players, %d transfers, %d departures, %d new teams",
                    sport_id,
                    league_id,
                    season,
                    len(result.new_players),
                    len(result.transferred_players),
                    len(result.departed_players),
                    len(result.new_teams),
                )
            else:
                logger.info(
                    "Roster diff for %s (league=%s, season=%d): no changes",
                    sport_id,
                    league_id,
                    season,
                )

            return result

        except Exception as e:
            logger.error("Roster diff failed for %s: %s", sport_id, e)
            result.completed_at = int(time.time())
            raise

    async def run_all_priority_diffs(self, season: int) -> list[DiffResult]:
        """
        Run roster diffs for all priority leagues/sports.

        Args:
            season: Season year

        Returns:
            List of DiffResults
        """
        results: list[DiffResult] = []

        # Run for NBA
        try:
            result = await self.run_diff("NBA", season)
            results.append(result)
        except Exception as e:
            logger.error("Diff failed for NBA: %s", e)

        # Run for NFL
        try:
            result = await self.run_diff("NFL", season)
            results.append(result)
        except Exception as e:
            logger.error("Diff failed for NFL: %s", e)

        # Run for priority Football leagues
        for league_id in _PRIORITY_FOOTBALL_LEAGUES:
            try:
                result = await self.run_diff("FOOTBALL", season, league_id)
                results.append(result)
            except Exception as e:
                logger.error("Diff failed for FOOTBALL league %d: %s", league_id, e)

        return results

    # =========================================================================
    # API Fetching
    # =========================================================================

    async def _fetch_api_roster(
        self,
        sport_id: str,
        season_id: int,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Fetch current roster from API."""
        if sport_id == "FOOTBALL":
            if not league_id:
                raise ValueError("league_id required for FOOTBALL")
            return await self._fetch_football_roster(season_id, league_id)
        elif sport_id == "NBA":
            return await self._fetch_nba_roster()
        elif sport_id == "NFL":
            return await self._fetch_nfl_roster()
        else:
            raise ValueError(f"Unknown sport: {sport_id}")

    async def _fetch_football_roster(
        self,
        season_id: int,
        league_id: int,
    ) -> list[dict[str, Any]]:
        """
        Fetch Football roster for a league via SportMonks.

        SportMonks exposes squads per team, so we first fetch all teams in the
        season, then fetch each team's squad.
        """
        all_players: list[dict[str, Any]] = []

        # Get all teams in this season first
        teams = await self.api.get_teams_by_season(season_id)

        for team in teams:
            team_id = team["id"]
            squad = await self.api.get_squad(season_id, team_id)

            for entry in squad:
                # Squad entries contain a player sub-object
                player = entry.get("player") or entry
                all_players.append({
                    "id": player.get("id") or entry.get("player_id"),
                    "full_name": self._build_full_name(player),
                    "current_team_id": team_id,
                    "position": player.get("position") or entry.get("position_id"),
                })

        return all_players

    async def _fetch_nba_roster(self) -> list[dict[str, Any]]:
        """Fetch NBA roster via BallDontLie (cursor-based async iterator)."""
        all_players: list[dict[str, Any]] = []

        async for player in self.api.get_players():
            team = player.get("team") or {}
            all_players.append({
                "id": player["id"],
                "full_name": self._build_full_name(player),
                "current_team_id": team.get("id") if isinstance(team, dict) else None,
                "position": player.get("position"),
            })

        return all_players

    async def _fetch_nfl_roster(self) -> list[dict[str, Any]]:
        """Fetch NFL roster via BallDontLie (cursor-based async iterator)."""
        all_players: list[dict[str, Any]] = []

        async for player in self.api.get_players():
            team = player.get("team") or {}
            all_players.append({
                "id": player["id"],
                "full_name": self._build_full_name(player),
                "current_team_id": team.get("id") if isinstance(team, dict) else None,
                "position": player.get("position"),
            })

        return all_players

    async def _fetch_api_teams(
        self,
        sport_id: str,
        season_id: int,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Fetch teams from API."""
        if sport_id == "FOOTBALL":
            teams = await self.api.get_teams_by_season(season_id)
        elif sport_id in ("NBA", "NFL"):
            # BallDontLie: get_teams() returns full list directly
            teams = await self.api.get_teams()
        else:
            return []

        return [
            {"id": t["id"], "name": t.get("name", t.get("full_name", "Unknown"))}
            for t in teams
        ]

    # =========================================================================
    # Database Operations
    # =========================================================================

    def _get_db_players(
        self,
        sport_id: str,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Get players from database using sport-specific table."""
        table = PLAYER_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning("Unknown sport_id for player lookup: %s", sport_id)
            return []

        if league_id:
            query = sql.SQL(
                "SELECT id, current_team_id, is_active FROM {} WHERE current_league_id = %s"
            ).format(sql.Identifier(table))
            return self.db.fetchall(query, (league_id,))

        query = sql.SQL(
            "SELECT id, current_team_id, is_active FROM {}"
        ).format(sql.Identifier(table))
        return self.db.fetchall(query)

    def _get_db_teams(
        self,
        sport_id: str,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Get teams from database using sport-specific table."""
        table = TEAM_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning("Unknown sport_id for team lookup: %s", sport_id)
            return []

        if league_id:
            query = sql.SQL(
                "SELECT id FROM {} WHERE league_id = %s"
            ).format(sql.Identifier(table))
            return self.db.fetchall(query, (league_id,))

        query = sql.SQL("SELECT id FROM {}").format(sql.Identifier(table))
        return self.db.fetchall(query)

    def _insert_player(
        self,
        player_data: dict[str, Any],
        sport_id: str,
        league_id: Optional[int],
    ) -> None:
        """Insert a new player record into the sport-specific profile table."""
        table = PLAYER_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning("Unknown sport_id for player insert: %s", sport_id)
            return

        query = sql.SQL(
            """
            INSERT INTO {table} (
                id, full_name, current_team_id, current_league_id,
                position, is_active, created_at, updated_at
            )
            VALUES (%s, %s, %s, %s, %s, TRUE, NOW(), NOW())
            ON CONFLICT (id) DO NOTHING
            """
        ).format(table=sql.Identifier(table))

        self.db.execute(
            query,
            (
                player_data["id"],
                player_data.get("full_name", "Unknown"),
                player_data.get("current_team_id"),
                league_id,
                player_data.get("position"),
            ),
        )

    def _insert_team(
        self,
        team_data: dict[str, Any],
        sport_id: str,
        league_id: Optional[int],
    ) -> None:
        """Insert a new team record into the sport-specific profile table."""
        table = TEAM_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning("Unknown sport_id for team insert: %s", sport_id)
            return

        query = sql.SQL(
            """
            INSERT INTO {table} (
                id, league_id, name, is_active, created_at, updated_at
            )
            VALUES (%s, %s, %s, TRUE, NOW(), NOW())
            ON CONFLICT (id) DO NOTHING
            """
        ).format(table=sql.Identifier(table))

        self.db.execute(
            query,
            (
                team_data["id"],
                league_id,
                team_data.get("name", "Unknown"),
            ),
        )

    def _record_transfer(
        self,
        player_id: int,
        to_team_id: Optional[int],
        sport_id: str,
    ) -> None:
        """
        Record a player transfer by updating their current team in the
        sport-specific profile table.
        """
        table = PLAYER_PROFILE_TABLES.get(sport_id)
        if not table:
            logger.warning("Unknown sport_id for transfer: %s", sport_id)
            return

        query = sql.SQL(
            "UPDATE {table} SET current_team_id = %s, updated_at = NOW() WHERE id = %s"
        ).format(table=sql.Identifier(table))

        self.db.execute(query, (to_team_id, player_id))

        logger.info(
            "Recorded transfer: player %d → team %s (table=%s)",
            player_id,
            to_team_id,
            table,
        )

    # =========================================================================
    # Helpers
    # =========================================================================

    def _build_full_name(self, player: dict) -> str:
        """Build full name from player data."""
        first = player.get("first_name") or player.get("firstname") or ""
        last = player.get("last_name") or player.get("lastname") or ""
        return f"{first} {last}".strip() or player.get("name", "Unknown")
