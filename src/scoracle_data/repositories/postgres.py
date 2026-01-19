"""
PostgreSQL repository implementations.

Provides PostgreSQL-specific implementations of the repository interfaces,
with support for JSONB raw_response storage and dynamic column handling.
"""

from __future__ import annotations

import json
import logging
from datetime import datetime
from typing import TYPE_CHECKING, Any

from psycopg.types.json import Json

from .base import (
    PlayerRepository,
    TeamRepository,
    PlayerStatsRepository,
    TeamStatsRepository,
)

if TYPE_CHECKING:
    from ..connection import StatsDB
    from ..sport_configs import ConfigLoader

logger = logging.getLogger(__name__)


# Table name mappings
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

PLAYER_STATS_TABLES = {
    "NBA": "nba_player_stats",
    "NFL": "nfl_player_stats",
    "FOOTBALL": "football_player_stats",
}

TEAM_STATS_TABLES = {
    "NBA": "nba_team_stats",
    "NFL": "nfl_team_stats",
    "FOOTBALL": "football_team_stats",
}


class PostgresPlayerRepository(PlayerRepository):
    """PostgreSQL implementation for player data access."""
    
    def __init__(self, db: "StatsDB", config: "ConfigLoader"):
        self.db = db
        self.config = config
    
    def _get_table(self, sport: str) -> str:
        """Get the player profile table for a sport."""
        sport_upper = sport.upper()
        if sport_upper not in PLAYER_PROFILE_TABLES:
            raise ValueError(f"Unknown sport: {sport}")
        return PLAYER_PROFILE_TABLES[sport_upper]
    
    def _get_columns(self, sport: str) -> list[str]:
        """Get player profile columns for a sport from config."""
        try:
            columns = self.config.get_player_columns(sport)
            if columns:
                return columns
        except KeyError:
            pass
        
        # Fallback to default columns if config not loaded
        return [
            "id", "first_name", "last_name", "full_name",
            "position", "position_group", "nationality",
            "birth_date", "birth_place", "birth_country",
            "height_inches", "weight_lbs", "photo_url",
            "current_team_id", "jersey_number",
            "college", "experience_years",
            "profile_fetched_at", "updated_at", "raw_response",
        ]
    
    def upsert(
        self,
        sport: str,
        player: dict[str, Any],
        raw_response: dict[str, Any],
        *,
        mark_profile_fetched: bool = False,
    ) -> int:
        """Insert or update a player record with raw_response JSONB."""
        table = self._get_table(sport)
        
        # Ensure required fields
        if "id" not in player:
            raise ValueError("Player data must include 'id'")
        
        # Add raw_response
        player_data = dict(player)
        player_data["raw_response"] = Json(raw_response)
        player_data["updated_at"] = datetime.now()
        
        if mark_profile_fetched:
            player_data["profile_fetched_at"] = datetime.now()
        
        # Build dynamic upsert query
        columns = [k for k in player_data.keys() if k != "id"]
        columns_str = ", ".join(["id"] + columns)
        placeholders = ", ".join(["%s"] * (len(columns) + 1))
        
        update_parts = [f"{col} = COALESCE(excluded.{col}, {table}.{col})" 
                       for col in columns if col not in ("profile_fetched_at",)]
        
        # profile_fetched_at should only update if mark_profile_fetched is True
        if mark_profile_fetched:
            update_parts.append(f"profile_fetched_at = excluded.profile_fetched_at")
        
        update_str = ", ".join(update_parts)
        
        query = f"""
            INSERT INTO {table} (
                {columns_str}
            )
            VALUES ({placeholders})
            ON CONFLICT(id) DO UPDATE SET
                {update_str}
        """
        
        # Build params tuple in column order
        params = tuple([player_data["id"]] + [player_data.get(col) for col in columns])
        
        self.db.execute(query, params)
        return player_data["id"]
    
    def batch_upsert(
        self,
        sport: str,
        players: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update player records."""
        if not players:
            return 0
        
        if len(players) != len(raw_responses):
            raise ValueError("players and raw_responses must have same length")
        
        table = self._get_table(sport)
        
        # Use multi-row INSERT
        # Determine columns from first player
        sample = players[0]
        base_columns = ["id", "first_name", "last_name", "full_name",
                       "position", "position_group", "current_team_id",
                       "jersey_number", "photo_url", "updated_at", "raw_response"]
        
        columns_str = ", ".join(base_columns)
        
        # Build placeholders for all rows
        row_placeholder = "(" + ", ".join(["%s"] * len(base_columns)) + ")"
        all_placeholders = ", ".join([row_placeholder] * len(players))
        
        # Build update clause
        update_cols = [c for c in base_columns if c not in ("id", "updated_at", "raw_response")]
        update_str = ", ".join([
            f"{col} = COALESCE(excluded.{col}, {table}.{col})"
            for col in update_cols
        ])
        update_str += f", updated_at = excluded.updated_at, raw_response = excluded.raw_response"
        
        query = f"""
            INSERT INTO {table} ({columns_str})
            VALUES {all_placeholders}
            ON CONFLICT(id) DO UPDATE SET {update_str}
        """
        
        # Build params for all rows
        params = []
        for player, raw in zip(players, raw_responses):
            params.extend([
                player.get("id"),
                player.get("first_name"),
                player.get("last_name"),
                player.get("full_name"),
                player.get("position"),
                player.get("position_group"),
                player.get("current_team_id"),
                player.get("jersey_number"),
                player.get("photo_url"),
                datetime.now(),
                Json(raw),
            ])
        
        self.db.execute(query, tuple(params))
        return len(players)
    
    def find_by_id(self, sport: str, player_id: int) -> dict[str, Any] | None:
        """Find a player by ID."""
        table = self._get_table(sport)
        result = self.db.fetchone(
            f"SELECT * FROM {table} WHERE id = %s",
            (player_id,)
        )
        return dict(result) if result else None
    
    def find_needing_profiles(self, sport: str) -> list[int]:
        """Find players that need profile fetch."""
        table = self._get_table(sport)
        results = self.db.fetchall(
            f"SELECT id FROM {table} WHERE profile_fetched_at IS NULL",
            ()
        )
        return [r["id"] for r in results]
    
    def mark_profile_fetched(self, sport: str, player_id: int) -> None:
        """Mark a player's profile as fetched."""
        table = self._get_table(sport)
        self.db.execute(
            f"UPDATE {table} SET profile_fetched_at = NOW() WHERE id = %s",
            (player_id,)
        )
    
    def get_all_ids(self, sport: str) -> list[int]:
        """Get all player IDs for a sport."""
        table = self._get_table(sport)
        results = self.db.fetchall(f"SELECT id FROM {table}", ())
        return [r["id"] for r in results]
    
    def extract_raw_field(
        self,
        sport: str,
        player_id: int,
        json_path: str,
    ) -> Any:
        """Extract a field from stored raw_response JSONB."""
        table = self._get_table(sport)
        result = self.db.fetchone(
            f"SELECT raw_response{json_path} as value FROM {table} WHERE id = %s",
            (player_id,)
        )
        return result["value"] if result else None


class PostgresTeamRepository(TeamRepository):
    """PostgreSQL implementation for team data access."""
    
    def __init__(self, db: "StatsDB", config: "ConfigLoader"):
        self.db = db
        self.config = config
    
    def _get_table(self, sport: str) -> str:
        """Get the team profile table for a sport."""
        sport_upper = sport.upper()
        if sport_upper not in TEAM_PROFILE_TABLES:
            raise ValueError(f"Unknown sport: {sport}")
        return TEAM_PROFILE_TABLES[sport_upper]
    
    def upsert(
        self,
        sport: str,
        team: dict[str, Any],
        raw_response: dict[str, Any],
        *,
        mark_profile_fetched: bool = False,
    ) -> int:
        """Insert or update a team record with raw_response JSONB."""
        table = self._get_table(sport)
        sport_upper = sport.upper()
        
        if "id" not in team:
            raise ValueError("Team data must include 'id'")
        
        team_data = dict(team)
        team_data["raw_response"] = Json(raw_response)
        team_data["updated_at"] = datetime.now()
        
        if mark_profile_fetched:
            team_data["profile_fetched_at"] = datetime.now()
        
        # Sport-specific columns
        if sport_upper == "FOOTBALL":
            base_cols = ["id", "name", "abbreviation", "country", "city", "league_id",
                        "logo_url", "founded", "is_national",
                        "venue_name", "venue_address", "venue_city", 
                        "venue_capacity", "venue_surface", "venue_image",
                        "profile_fetched_at", "updated_at", "raw_response"]
        else:
            # NBA/NFL have conference/division
            base_cols = ["id", "name", "abbreviation", "conference", "division",
                        "city", "country", "logo_url", "founded",
                        "venue_name", "venue_address", "venue_city",
                        "venue_capacity", "venue_surface", "venue_image",
                        "profile_fetched_at", "updated_at", "raw_response"]
        
        # Filter to columns we have data for
        columns = [c for c in base_cols if c in team_data or c in ("updated_at", "raw_response")]
        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(columns))
        
        update_cols = [c for c in columns if c not in ("id",)]
        update_str = ", ".join([
            f"{col} = COALESCE(excluded.{col}, {table}.{col})"
            for col in update_cols
        ])
        
        query = f"""
            INSERT INTO {table} ({columns_str})
            VALUES ({placeholders})
            ON CONFLICT(id) DO UPDATE SET {update_str}
        """
        
        params = tuple(team_data.get(col) for col in columns)
        self.db.execute(query, params)
        return team_data["id"]
    
    def batch_upsert(
        self,
        sport: str,
        teams: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update team records."""
        if not teams:
            return 0
        
        if len(teams) != len(raw_responses):
            raise ValueError("teams and raw_responses must have same length")
        
        table = self._get_table(sport)
        sport_upper = sport.upper()
        
        # Sport-specific columns for batch
        if sport_upper == "FOOTBALL":
            base_columns = ["id", "name", "abbreviation", "country", "city",
                          "league_id", "logo_url", "updated_at", "raw_response"]
        else:
            base_columns = ["id", "name", "abbreviation", "conference", "division",
                          "city", "country", "logo_url", "updated_at", "raw_response"]
        
        columns_str = ", ".join(base_columns)
        row_placeholder = "(" + ", ".join(["%s"] * len(base_columns)) + ")"
        all_placeholders = ", ".join([row_placeholder] * len(teams))
        
        update_cols = [c for c in base_columns if c not in ("id", "updated_at", "raw_response")]
        update_str = ", ".join([
            f"{col} = COALESCE(excluded.{col}, {table}.{col})"
            for col in update_cols
        ])
        update_str += f", updated_at = excluded.updated_at, raw_response = excluded.raw_response"
        
        query = f"""
            INSERT INTO {table} ({columns_str})
            VALUES {all_placeholders}
            ON CONFLICT(id) DO UPDATE SET {update_str}
        """
        
        params = []
        for team, raw in zip(teams, raw_responses):
            if sport_upper == "FOOTBALL":
                params.extend([
                    team.get("id"),
                    team.get("name"),
                    team.get("abbreviation"),
                    team.get("country"),
                    team.get("city"),
                    team.get("league_id"),
                    team.get("logo_url"),
                    datetime.now(),
                    Json(raw),
                ])
            else:
                params.extend([
                    team.get("id"),
                    team.get("name"),
                    team.get("abbreviation"),
                    team.get("conference"),
                    team.get("division"),
                    team.get("city"),
                    team.get("country"),
                    team.get("logo_url"),
                    datetime.now(),
                    Json(raw),
                ])
        
        self.db.execute(query, tuple(params))
        return len(teams)
    
    def find_by_id(self, sport: str, team_id: int) -> dict[str, Any] | None:
        """Find a team by ID."""
        table = self._get_table(sport)
        result = self.db.fetchone(
            f"SELECT * FROM {table} WHERE id = %s",
            (team_id,)
        )
        return dict(result) if result else None
    
    def find_needing_profiles(self, sport: str) -> list[int]:
        """Find teams that need profile fetch."""
        table = self._get_table(sport)
        results = self.db.fetchall(
            f"SELECT id FROM {table} WHERE profile_fetched_at IS NULL",
            ()
        )
        return [r["id"] for r in results]
    
    def mark_profile_fetched(self, sport: str, team_id: int) -> None:
        """Mark a team's profile as fetched."""
        table = self._get_table(sport)
        self.db.execute(
            f"UPDATE {table} SET profile_fetched_at = NOW() WHERE id = %s",
            (team_id,)
        )
    
    def get_all_ids(self, sport: str) -> list[int]:
        """Get all team IDs for a sport."""
        table = self._get_table(sport)
        results = self.db.fetchall(f"SELECT id FROM {table}", ())
        return [r["id"] for r in results]


class PostgresPlayerStatsRepository(PlayerStatsRepository):
    """PostgreSQL implementation for player stats data access."""
    
    def __init__(self, db: "StatsDB", config: "ConfigLoader"):
        self.db = db
        self.config = config
    
    def _get_table(self, sport: str) -> str:
        """Get the player stats table for a sport."""
        sport_upper = sport.upper()
        if sport_upper not in PLAYER_STATS_TABLES:
            raise ValueError(f"Unknown sport: {sport}")
        return PLAYER_STATS_TABLES[sport_upper]
    
    def upsert(
        self,
        sport: str,
        stats: dict[str, Any],
        raw_response: dict[str, Any],
    ) -> None:
        """Insert or update player stats with raw_response JSONB."""
        table = self._get_table(sport)
        
        # Add metadata
        stats_data = dict(stats)
        stats_data["raw_response"] = Json(raw_response)
        stats_data["updated_at"] = datetime.now()
        
        # Get conflict keys based on sport
        if sport.upper() == "FOOTBALL":
            conflict_keys = ["player_id", "season_id", "team_id", "league_id"]
        else:
            conflict_keys = ["player_id", "season_id", "team_id"]
        
        # Build query dynamically
        columns = list(stats_data.keys())
        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(columns))
        conflict_str = ", ".join(conflict_keys)
        
        update_cols = [c for c in columns if c not in conflict_keys]
        update_str = ", ".join([f"{col} = excluded.{col}" for col in update_cols])
        
        query = f"""
            INSERT INTO {table} ({columns_str})
            VALUES ({placeholders})
            ON CONFLICT({conflict_str}) DO UPDATE SET {update_str}
        """
        
        params = tuple(stats_data.get(col) for col in columns)
        self.db.execute(query, params)
    
    def batch_upsert(
        self,
        sport: str,
        stats_list: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update player stats."""
        if not stats_list:
            return 0
        
        # For simplicity, use individual upserts
        # Could be optimized with multi-row INSERT
        for stats, raw in zip(stats_list, raw_responses):
            self.upsert(sport, stats, raw)
        
        return len(stats_list)
    
    def find_by_player_season(
        self,
        sport: str,
        player_id: int,
        season_id: int,
        team_id: int | None = None,
    ) -> dict[str, Any] | None:
        """Find player stats for a specific season."""
        table = self._get_table(sport)
        
        if team_id:
            result = self.db.fetchone(
                f"SELECT * FROM {table} WHERE player_id = %s AND season_id = %s AND team_id = %s",
                (player_id, season_id, team_id)
            )
        else:
            result = self.db.fetchone(
                f"SELECT * FROM {table} WHERE player_id = %s AND season_id = %s",
                (player_id, season_id)
            )
        
        return dict(result) if result else None
    
    def get_season_stats(
        self,
        sport: str,
        season_id: int,
        *,
        min_games: int = 0,
    ) -> list[dict[str, Any]]:
        """Get all player stats for a season."""
        table = self._get_table(sport)
        
        query = f"SELECT * FROM {table} WHERE season_id = %s"
        params: list[Any] = [season_id]
        
        if min_games > 0:
            query += " AND games_played >= %s"
            params.append(min_games)
        
        results = self.db.fetchall(query, tuple(params))
        return [dict(r) for r in results]


class PostgresTeamStatsRepository(TeamStatsRepository):
    """PostgreSQL implementation for team stats data access."""
    
    def __init__(self, db: "StatsDB", config: "ConfigLoader"):
        self.db = db
        self.config = config
    
    def _get_table(self, sport: str) -> str:
        """Get the team stats table for a sport."""
        sport_upper = sport.upper()
        if sport_upper not in TEAM_STATS_TABLES:
            raise ValueError(f"Unknown sport: {sport}")
        return TEAM_STATS_TABLES[sport_upper]
    
    def upsert(
        self,
        sport: str,
        stats: dict[str, Any],
        raw_response: dict[str, Any],
    ) -> None:
        """Insert or update team stats with raw_response JSONB."""
        table = self._get_table(sport)
        
        stats_data = dict(stats)
        stats_data["raw_response"] = Json(raw_response)
        stats_data["updated_at"] = datetime.now()
        
        # Conflict keys
        if sport.upper() == "FOOTBALL":
            conflict_keys = ["team_id", "season_id", "league_id"]
        else:
            conflict_keys = ["team_id", "season_id"]
        
        columns = list(stats_data.keys())
        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(columns))
        conflict_str = ", ".join(conflict_keys)
        
        update_cols = [c for c in columns if c not in conflict_keys]
        update_str = ", ".join([f"{col} = excluded.{col}" for col in update_cols])
        
        query = f"""
            INSERT INTO {table} ({columns_str})
            VALUES ({placeholders})
            ON CONFLICT({conflict_str}) DO UPDATE SET {update_str}
        """
        
        params = tuple(stats_data.get(col) for col in columns)
        self.db.execute(query, params)
    
    def batch_upsert(
        self,
        sport: str,
        stats_list: list[dict[str, Any]],
        raw_responses: list[dict[str, Any]],
    ) -> int:
        """Batch insert or update team stats."""
        if not stats_list:
            return 0
        
        for stats, raw in zip(stats_list, raw_responses):
            self.upsert(sport, stats, raw)
        
        return len(stats_list)
    
    def find_by_team_season(
        self,
        sport: str,
        team_id: int,
        season_id: int,
    ) -> dict[str, Any] | None:
        """Find team stats for a specific season."""
        table = self._get_table(sport)
        result = self.db.fetchone(
            f"SELECT * FROM {table} WHERE team_id = %s AND season_id = %s",
            (team_id, season_id)
        )
        return dict(result) if result else None
    
    def get_season_stats(
        self,
        sport: str,
        season_id: int,
    ) -> list[dict[str, Any]]:
        """Get all team stats for a season."""
        table = self._get_table(sport)
        results = self.db.fetchall(
            f"SELECT * FROM {table} WHERE season_id = %s",
            (season_id,)
        )
        return [dict(r) for r in results]
