"""
SQL query builder for statsdb operations.

Generates UPSERT queries dynamically from column definitions,
eliminating the need for manually maintaining 100+ line SQL strings.

Design: Supports both SQLite and PostgreSQL (uses ON CONFLICT syntax).
This module is designed to be extracted to scoracle-data repo.
"""

from __future__ import annotations

import os
from typing import Optional


def _use_postgres() -> bool:
    """Check if PostgreSQL mode is enabled."""
    return os.environ.get("USE_POSTGRESQL", "true").lower() in ("true", "1", "yes")


class UpsertQueryBuilder:
    """Build UPSERT queries dynamically from column definitions.

    Eliminates the need for massive hand-written SQL strings in seeders.
    All queries use ON CONFLICT ... DO UPDATE SET pattern.
    Supports both SQLite (named :param) and PostgreSQL (%s) placeholders.
    """

    @staticmethod
    def build_upsert(
        table: str,
        columns: list[str],
        conflict_keys: list[str],
        exclude_from_update: Optional[list[str]] = None,
        use_coalesce: bool = False,
        use_postgres: Optional[bool] = None,
    ) -> str:
        """Generate UPSERT query from column list.

        Args:
            table: Table name
            columns: List of all column names (including conflict keys)
            conflict_keys: List of columns that define uniqueness (for ON CONFLICT)
            exclude_from_update: Columns to NOT update on conflict (default: conflict_keys + updated_at)
            use_coalesce: If True, use COALESCE to preserve existing values when new is NULL
            use_postgres: If True, use PostgreSQL %s placeholders. If None, auto-detect.

        Returns:
            Complete SQL UPSERT query string

        Example:
            >>> builder = UpsertQueryBuilder()
            >>> query = builder.build_upsert(
            ...     table="nba_player_stats",
            ...     columns=["player_id", "season_id", "team_id", "points", "rebounds", "updated_at"],
            ...     conflict_keys=["player_id", "season_id", "team_id"],
            ... )
            # Returns SQL with INSERT ... ON CONFLICT DO UPDATE
        """
        # Default exclusions: conflict keys and updated_at
        if exclude_from_update is None:
            exclude_from_update = conflict_keys.copy()
            if "updated_at" not in exclude_from_update:
                exclude_from_update.append("updated_at")

        # Determine placeholder style
        if use_postgres is None:
            use_postgres = _use_postgres()

        # Build placeholders for VALUES clause
        if use_postgres:
            placeholders = ["%s" for _ in columns]
        else:
            placeholders = [f":{col}" for col in columns]

        # Build UPDATE SET clause
        update_assignments = []
        for col in columns:
            if col in exclude_from_update:
                continue

            if use_coalesce:
                # COALESCE preserves existing value if new value is NULL
                update_assignments.append(
                    f"{col} = COALESCE(excluded.{col}, {table}.{col})"
                )
            else:
                # Simple replacement
                update_assignments.append(f"{col} = excluded.{col}")

        # Build complete query
        columns_str = ', '.join(columns)
        placeholders_str = ', '.join(placeholders)
        conflict_keys_str = ', '.join(conflict_keys)
        update_str = ',\n                '.join(update_assignments)

        query = f"""
            INSERT INTO {table} (
                {columns_str}
            )
            VALUES (
                {placeholders_str}
            )
            ON CONFLICT({conflict_keys_str}) DO UPDATE SET
                {update_str}
        """

        return query.strip()

    @staticmethod
    def build_bulk_upsert(
        table: str,
        columns: list[str],
        conflict_keys: list[str],
        row_count: int,
        exclude_from_update: Optional[list[str]] = None,
        use_coalesce: bool = False,
        use_postgres: Optional[bool] = None,
    ) -> str:
        """Generate bulk UPSERT query for multiple rows.

        Similar to build_upsert, but generates placeholders for N rows.
        Useful for batch inserts.

        Args:
            table: Table name
            columns: List of all column names
            conflict_keys: List of columns that define uniqueness
            row_count: Number of rows to insert
            exclude_from_update: Columns to NOT update on conflict
            use_coalesce: If True, preserve existing NULL values
            use_postgres: If True, use PostgreSQL %s placeholders. If None, auto-detect.

        Returns:
            SQL query with multiple value sets
        """
        if exclude_from_update is None:
            exclude_from_update = conflict_keys.copy()
            if "updated_at" not in exclude_from_update:
                exclude_from_update.append("updated_at")

        # Determine placeholder style
        if use_postgres is None:
            use_postgres = _use_postgres()

        # Build multiple value sets
        value_sets = []
        for i in range(row_count):
            if use_postgres:
                placeholders = ["%s" for _ in columns]
            else:
                placeholders = [f":{col}_{i}" for col in columns]
            value_sets.append(f"({', '.join(placeholders)})")

        # Build UPDATE SET clause
        update_assignments = []
        for col in columns:
            if col in exclude_from_update:
                continue

            if use_coalesce:
                update_assignments.append(
                    f"{col} = COALESCE(excluded.{col}, {table}.{col})"
                )
            else:
                update_assignments.append(f"{col} = excluded.{col}")

        columns_str = ', '.join(columns)
        value_sets_str = ',\n                '.join(value_sets)
        conflict_keys_str = ', '.join(conflict_keys)
        update_str = ',\n                '.join(update_assignments)

        query = f"""
            INSERT INTO {table} (
                {columns_str}
            )
            VALUES
                {value_sets_str}
            ON CONFLICT({conflict_keys_str}) DO UPDATE SET
                {update_str}
        """

        return query.strip()


class QueryCache:
    """Cache generated queries to avoid rebuilding on every call.

    Since queries are deterministic based on inputs, we can cache them.
    """

    def __init__(self):
        self._cache: dict[str, str] = {}

    def get_or_build_upsert(
        self,
        table: str,
        columns: list[str],
        conflict_keys: list[str],
        exclude_from_update: Optional[list[str]] = None,
        use_coalesce: bool = False,
        use_postgres: Optional[bool] = None,
    ) -> str:
        """Get cached query or build and cache it.

        Args:
            Same as UpsertQueryBuilder.build_upsert

        Returns:
            SQL query string
        """
        # Determine placeholder style for cache key
        if use_postgres is None:
            use_postgres = _use_postgres()

        # Create cache key from inputs
        cache_key = (
            f"{table}:{','.join(columns)}:{','.join(conflict_keys)}:"
            f"{','.join(exclude_from_update or [])}:{use_coalesce}:{use_postgres}"
        )

        if cache_key not in self._cache:
            self._cache[cache_key] = UpsertQueryBuilder.build_upsert(
                table=table,
                columns=columns,
                conflict_keys=conflict_keys,
                exclude_from_update=exclude_from_update,
                use_coalesce=use_coalesce,
                use_postgres=use_postgres,
            )

        return self._cache[cache_key]


# Global query cache instance for reuse across seeders
query_cache = QueryCache()


def get_placeholder(use_postgres: Optional[bool] = None) -> str:
    """
    Get the appropriate placeholder for the current database type.

    Args:
        use_postgres: If True, return PostgreSQL placeholder. If None, auto-detect.

    Returns:
        '%s' for PostgreSQL, '?' for SQLite
    """
    if use_postgres is None:
        use_postgres = _use_postgres()
    return "%s" if use_postgres else "?"


def convert_placeholders(query: str, use_postgres: Optional[bool] = None) -> str:
    """
    Convert SQL query placeholders based on database type.

    Converts between SQLite (?) and PostgreSQL (%s) placeholder styles.

    Args:
        query: SQL query with ? placeholders
        use_postgres: If True, convert to %s. If False, keep as ?. If None, auto-detect.

    Returns:
        Query with appropriate placeholders

    Example:
        >>> convert_placeholders("SELECT * FROM users WHERE id = ?", use_postgres=True)
        "SELECT * FROM users WHERE id = %s"
    """
    if use_postgres is None:
        use_postgres = _use_postgres()

    if use_postgres:
        # Convert ? to %s
        return query.replace("?", "%s")
    else:
        # Keep as-is (already using ?)
        return query


# Pre-defined column sets for common tables
# This makes seeder code even cleaner - just reference the constant

NBA_PLAYER_STATS_COLUMNS = [
    "player_id", "season_id", "team_id",
    "games_played", "games_started", "minutes_total", "minutes_per_game",
    "points_total", "points_per_game",
    "fgm", "fga", "fg_pct", "tpm", "tpa", "tp_pct", "ftm", "fta", "ft_pct",
    "offensive_rebounds", "defensive_rebounds", "total_rebounds", "rebounds_per_game",
    "assists", "assists_per_game", "turnovers", "turnovers_per_game",
    "steals", "steals_per_game", "blocks", "blocks_per_game",
    "personal_fouls", "fouls_per_game",
    "plus_minus", "plus_minus_per_game", "efficiency",
    "true_shooting_pct", "effective_fg_pct", "assist_turnover_ratio",
    "updated_at",
]

NBA_TEAM_STATS_COLUMNS = [
    "team_id", "season_id",
    "games_played", "wins", "losses", "win_pct",
    "home_wins", "home_losses", "away_wins", "away_losses",
    "points_per_game", "opponent_ppg",
    "updated_at",
]

NFL_PLAYER_STATS_COLUMNS = [
    "player_id", "season_id", "team_id", "games_played", "games_started",
    "pass_attempts", "pass_completions", "pass_yards", "pass_touchdowns",
    "interceptions_thrown", "passer_rating", "completion_pct", "yards_per_attempt",
    "longest_pass", "sacks_taken", "sack_yards_lost",
    "rush_attempts", "rush_yards", "rush_touchdowns", "yards_per_carry",
    "longest_rush", "rush_fumbles", "rush_fumbles_lost",
    "targets", "receptions", "receiving_yards", "receiving_touchdowns",
    "yards_per_reception", "longest_reception", "yards_after_catch",
    "rec_fumbles", "rec_fumbles_lost",
    "tackles_total", "tackles_solo", "tackles_assist", "tackles_for_loss",
    "sacks", "sack_yards", "qb_hits", "def_interceptions", "int_yards",
    "int_touchdowns", "passes_defended", "forced_fumbles", "fumble_recoveries",
    "fg_attempts", "fg_made", "fg_pct", "fg_long", "xp_attempts", "xp_made", "xp_pct",
    "kicking_points", "punts", "punt_yards", "punt_avg", "punt_long",
    "punts_inside_20", "touchbacks",
    "kick_returns", "kick_return_yards", "kick_return_touchdowns",
    "punt_returns", "punt_return_yards", "punt_return_touchdowns",
    "updated_at",
]

NFL_TEAM_STATS_COLUMNS = [
    "team_id", "season_id", "games_played", "wins", "losses", "ties", "win_pct",
    "points_for", "points_against", "point_differential",
    "total_yards", "yards_per_game", "pass_yards", "rush_yards", "turnovers",
    "yards_allowed", "pass_yards_allowed", "rush_yards_allowed", "takeaways",
    "updated_at",
]

FOOTBALL_PLAYER_STATS_COLUMNS = [
    "player_id", "season_id", "team_id", "league_id",
    "appearances", "starts", "bench_appearances", "minutes_played",
    "goals", "assists", "goals_assists", "goals_per_90", "assists_per_90",
    "shots_total", "shots_on_target", "shot_accuracy",
    "passes_total", "passes_accurate", "pass_accuracy", "key_passes",
    "dribbles_attempted", "dribbles_successful", "dribble_success_rate",
    "duels_total", "duels_won", "duel_success_rate",
    "tackles", "interceptions", "blocks",
    "fouls_committed", "fouls_drawn", "yellow_cards", "red_cards",
    "penalties_won", "penalties_scored", "penalties_missed",
    "saves", "goals_conceded",
    "updated_at",
]

FOOTBALL_TEAM_STATS_COLUMNS = [
    "team_id", "season_id", "league_id",
    "matches_played", "wins", "draws", "losses", "points",
    "home_played", "home_wins", "home_draws", "home_losses",
    "home_goals_for", "home_goals_against",
    "away_played", "away_wins", "away_draws", "away_losses",
    "away_goals_for", "away_goals_against",
    "goals_for", "goals_against", "goal_difference",
    "goals_per_game", "goals_conceded_per_game",
    "clean_sheets", "failed_to_score", "form", "avg_possession",
    "updated_at",
]
