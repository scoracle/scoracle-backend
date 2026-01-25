"""
RosterDiffEngine: Detects roster changes (trades, transfers, new players).

This engine runs daily during the season to catch player movements between
teams, ensuring the local database stays in sync with the latest rosters.

Features:
- Compares API roster snapshot with local database
- Detects new players (need profile fetch)
- Detects team changes (trades/transfers)
- Detects departures (players no longer on any roster)
- Records transfer history in player_teams table
- Minimal API calls (one roster fetch per league/sport)
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from ..connection import StatsDB
    from ..providers import DataProviderProtocol

logger = logging.getLogger(__name__)

# Sport-specific table mappings to avoid cross-sport data contamination
PLAYER_PROFILE_TABLES = {
    "NBA": "nba_player_profiles",
    "NFL": "nfl_player_profiles",
    "FOOTBALL": "football_player_profiles",
}

TEAM_PROFILE_TABLES = {
    "NBA": "nba_team_profiles",
    "NFL": "nfl_team_profiles",
    "FOOTBALL": "football_team_profiles",
}


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
        engine = RosterDiffEngine(db, api_service)
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
        db: "StatsDB",
        api_service: "ApiSportsService",
    ):
        """
        Initialize the RosterDiffEngine.

        Args:
            db: StatsDB connection
            api_service: API-Sports service for fetching rosters
        """
        self.db = db
        self.api = api_service

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
            api_players = await self._fetch_api_roster(sport_id, season, league_id)
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
                        # Update player's current team and record transfer
                        self._record_transfer(
                            player_id,
                            db_team_id,
                            api_team_id,
                            season_id,
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
            api_teams = await self._fetch_api_teams(sport_id, season, league_id)
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
        results = []

        # Get priority Football leagues
        football_leagues = self.db.fetchall(
            "SELECT id FROM leagues WHERE sport_id = 'FOOTBALL' AND priority_tier = 1"
        )

        for league in football_leagues:
            try:
                result = await self.run_diff("FOOTBALL", season, league["id"])
                results.append(result)
            except Exception as e:
                logger.error("Diff failed for FOOTBALL league %d: %s", league["id"], e)

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

        return results

    # =========================================================================
    # API Fetching
    # =========================================================================

    async def _fetch_api_roster(
        self,
        sport_id: str,
        season: int,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Fetch current roster from API."""
        if sport_id == "FOOTBALL":
            if not league_id:
                raise ValueError("league_id required for FOOTBALL")
            return await self._fetch_football_roster(league_id, season)
        elif sport_id == "NBA":
            return await self._fetch_nba_roster(season)
        elif sport_id == "NFL":
            return await self._fetch_nfl_roster(season)
        else:
            raise ValueError(f"Unknown sport: {sport_id}")

    async def _fetch_football_roster(
        self,
        league_id: int,
        season: int,
    ) -> list[dict[str, Any]]:
        """Fetch Football roster for a league."""
        all_players = []
        page = 1
        max_pages = 100

        while page <= max_pages:
            players = await self.api.list_players(
                sport="FOOTBALL",
                league=league_id,
                season=season,
                page=page,
            )

            if not players:
                break

            for player in players:
                team_data = player.get("team") or player.get("statistics", [{}])[0].get("team") or {}
                all_players.append({
                    "id": player["id"],
                    "full_name": self._build_full_name(player),
                    "current_team_id": team_data.get("id") if isinstance(team_data, dict) else None,
                    "position": player.get("position"),
                })

            if len(players) < 20:
                break

            page += 1

        return all_players

    async def _fetch_nba_roster(self, season: int) -> list[dict[str, Any]]:
        """Fetch NBA roster."""
        all_players = []
        page = 1
        max_pages = 50

        while page <= max_pages:
            players = await self.api.list_players(
                sport="NBA",
                season=season,
                page=page,
            )

            if not players:
                break

            for player in players:
                team = player.get("team") or {}
                all_players.append({
                    "id": player["id"],
                    "full_name": self._build_full_name(player),
                    "current_team_id": team.get("id") if isinstance(team, dict) else None,
                    "position": player.get("position"),
                })

            page += 1

            if page > max_pages:
                break

        return all_players

    async def _fetch_nfl_roster(self, season: int) -> list[dict[str, Any]]:
        """Fetch NFL roster."""
        all_players = []
        page = 1
        max_pages = 100

        while page <= max_pages:
            players = await self.api.list_players(
                sport="NFL",
                season=season,
                page=page,
            )

            if not players:
                break

            for player in players:
                team = player.get("team") or {}
                all_players.append({
                    "id": player["id"],
                    "full_name": self._build_full_name(player),
                    "current_team_id": team.get("id") if isinstance(team, dict) else None,
                    "position": player.get("position"),
                })

            page += 1

            if page > max_pages:
                break

        return all_players

    async def _fetch_api_teams(
        self,
        sport_id: str,
        season: int,
        league_id: Optional[int],
    ) -> list[dict[str, Any]]:
        """Fetch teams from API."""
        if sport_id == "FOOTBALL":
            teams = await self.api.list_teams(
                sport="FOOTBALL",
                league=league_id,
                season=season,
            )
        elif sport_id == "NBA":
            teams = await self.api.list_teams(sport="NBA", season=season)
        elif sport_id == "NFL":
            teams = await self.api.list_teams(sport="NFL", season=season)
        else:
            return []

        return [
            {"id": t["id"], "name": t["name"]}
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
            return self.db.fetchall(
                f"SELECT id, current_team_id, is_active FROM {table} WHERE current_league_id = ?",
                (league_id,),
            )
        return self.db.fetchall(
            f"SELECT id, current_team_id, is_active FROM {table}",
        )

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
            return self.db.fetchall(
                f"SELECT id FROM {table} WHERE league_id = ?",
                (league_id,),
            )
        return self.db.fetchall(
            f"SELECT id FROM {table}",
        )

    def _insert_player(
        self,
        player_data: dict[str, Any],
        sport_id: str,
        league_id: Optional[int],
    ) -> None:
        """Insert a new player record."""
        self.db.execute(
            """
            INSERT OR IGNORE INTO players (
                id, sport_id, full_name, current_team_id, current_league_id,
                position, is_active, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)
            """,
            (
                player_data["id"],
                sport_id,
                player_data.get("full_name", "Unknown"),
                player_data.get("current_team_id"),
                league_id,
                player_data.get("position"),
                int(time.time()),
                int(time.time()),
            ),
        )

    def _insert_team(
        self,
        team_data: dict[str, Any],
        sport_id: str,
        league_id: Optional[int],
    ) -> None:
        """Insert a new team record."""
        self.db.execute(
            """
            INSERT OR IGNORE INTO teams (
                id, sport_id, league_id, name, is_active, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, 1, ?, ?)
            """,
            (
                team_data["id"],
                sport_id,
                league_id,
                team_data.get("name", "Unknown"),
                int(time.time()),
                int(time.time()),
            ),
        )

    def _record_transfer(
        self,
        player_id: int,
        from_team_id: Optional[int],
        to_team_id: Optional[int],
        season_id: int,
    ) -> None:
        """Record a player transfer in the database."""
        now = int(time.time())

        # Close previous team assignment
        if from_team_id:
            self.db.execute(
                """
                UPDATE player_teams
                SET end_date = date('now'), is_current = 0
                WHERE player_id = ? AND team_id = ? AND is_current = 1
                """,
                (player_id, from_team_id),
            )

        # Create new team assignment
        if to_team_id:
            self.db.execute(
                """
                INSERT OR IGNORE INTO player_teams
                (player_id, team_id, season_id, start_date, is_current, detected_at)
                VALUES (?, ?, ?, date('now'), 1, ?)
                """,
                (player_id, to_team_id, season_id, now),
            )

        # Update player's current team
        self.db.execute(
            """
            UPDATE players
            SET current_team_id = ?, updated_at = ?
            WHERE id = ?
            """,
            (to_team_id, now, player_id),
        )

        logger.info(
            "Recorded transfer: player %d from team %s to team %s",
            player_id,
            from_team_id,
            to_team_id,
        )

    # =========================================================================
    # Helpers
    # =========================================================================

    def _build_full_name(self, player: dict) -> str:
        """Build full name from player data."""
        first = player.get("first_name") or player.get("firstname") or ""
        last = player.get("last_name") or player.get("lastname") or ""
        return f"{first} {last}".strip() or player.get("name", "Unknown")
