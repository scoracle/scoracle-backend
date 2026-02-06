"""
Prediction service — SQL queries for performance prediction endpoints.

Encapsulates all raw SQL for prediction-related queries.
Routers call these functions instead of building SQL inline.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from ..core.types import PLAYER_STATS_TABLES, TEAM_STATS_TABLES

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


def get_next_prediction(
    db: "PostgresDB", entity_type: str, entity_id: int
) -> dict[str, Any] | None:
    """Get the next upcoming performance prediction for an entity.

    Selects the nearest future prediction (game_date >= today) for the
    given entity.

    Returns:
        A dict with keys: opponent_id, opponent_name, game_date,
        predictions, confidence_intervals, confidence_score,
        context_factors, model_version, predicted_at.
        Returns None if no upcoming prediction exists.
    """
    return db.fetchone(
        """
        SELECT
            pp.opponent_id, pp.opponent_name, pp.game_date,
            pp.predictions, pp.confidence_intervals, pp.confidence_score,
            pp.context_factors, pp.model_version, pp.predicted_at
        FROM performance_predictions pp
        WHERE pp.entity_type = %s AND pp.entity_id = %s
        AND pp.game_date >= CURRENT_DATE
        ORDER BY pp.game_date ASC
        LIMIT 1
        """,
        (entity_type, entity_id),
    )


def get_specific_prediction(
    db: "PostgresDB", entity_type: str, entity_id: int, game_id: int
) -> dict[str, Any] | None:
    """Get a performance prediction for a specific game.

    Returns:
        A dict with keys: entity_name, opponent_id, opponent_name, game_date,
        sport, predictions, confidence_intervals, confidence_score,
        context_factors, model_version, predicted_at.
        Returns None if no matching prediction exists.
    """
    return db.fetchone(
        """
        SELECT
            pp.entity_name, pp.opponent_id, pp.opponent_name, pp.game_date,
            pp.sport, pp.predictions, pp.confidence_intervals, pp.confidence_score,
            pp.context_factors, pp.model_version, pp.predicted_at
        FROM performance_predictions pp
        WHERE pp.entity_type = %s AND pp.entity_id = %s AND pp.id = %s
        """,
        (entity_type, entity_id, game_id),
    )


def get_model_accuracy(
    db: "PostgresDB",
    model_version: str,
    model_type: str = "performance",
    sport: str | None = None,
) -> dict[str, Any] | None:
    """Get accuracy metrics for a specific model version.

    Optionally filters by sport.

    Returns:
        A dict with keys: model_type, model_version, sport, mae, rmse, mape,
        within_range_pct, sample_size, period_start, period_end.
        Returns None if no accuracy data is recorded.
    """
    query = """
        SELECT
            model_type, model_version, sport,
            mae, rmse, mape, within_range_pct,
            sample_size, period_start, period_end
        FROM prediction_accuracy
        WHERE model_version = %s AND model_type = %s
    """
    params: list[Any] = [model_version, model_type]

    if sport:
        query += " AND LOWER(sport) = LOWER(%s)"
        params.append(sport)

    query += " ORDER BY calculated_at DESC LIMIT 1"

    return db.fetchone(query, tuple(params))


def get_recent_stats(
    db: "PostgresDB",
    entity_type: str,
    entity_id: int,
    sport: str,
    stats_table: str,
    stat_cols: list[str],
) -> dict[str, Any] | None:
    """Get the most recent season stats for an entity.

    Used by the heuristic prediction path when no ML prediction exists.
    The caller determines the correct ``stats_table`` and ``stat_cols``
    from PLAYER_STATS_TABLES / TEAM_STATS_TABLES and the sport-specific
    column registry.

    Args:
        db: Database connection.
        entity_type: ``"player"`` or ``"team"``.
        entity_id: Entity primary key.
        sport: Sport identifier (used for logging only here).
        stats_table: Fully-qualified stats table name.
        stat_cols: Column names to select.

    Returns:
        A dict mapping each stat column to its value, or None.
    """
    id_col = "player_id" if entity_type == "player" else "team_id"
    col_list = ", ".join(stat_cols)

    return db.fetchone(
        f"""
        SELECT {col_list}
        FROM {stats_table}
        WHERE {id_col} = %s
        ORDER BY season_id DESC
        LIMIT 1
        """,
        (entity_id,),
    )
