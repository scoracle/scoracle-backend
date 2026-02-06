"""
ML Router - Machine Learning prediction endpoints.

Endpoints:
- GET /transfers/predictions/{team_id} - Transfer predictions for a team
- GET /transfers/predictions/player/{player_id} - Transfer predictions for a player
- GET /transfers/trending - Trending transfer rumors
- GET /vibe/{entity_type}/{entity_id} - Vibe score for an entity
- GET /vibe/trending/{sport} - Trending vibe changes
- GET /predictions/{entity_type}/{entity_id}/next - Next game performance prediction
- GET /predictions/{entity_type}/{entity_id}/game/{game_id} - Specific game prediction
- GET /predictions/accuracy/{model_version} - Model accuracy metrics

NOTE: Similarity endpoints have been moved to /api/v1/similarity
      See routers/similarity.py for the new percentile-based similarity feature.

Performance Features:
- Caching with configurable TTLs
- Batch predictions for efficiency
"""

import logging
from datetime import datetime
from typing import Annotated, Any

from fastapi import APIRouter, Query
from pydantic import BaseModel, Field

from ..cache import get_cache, TTL_TRANSFER_PREDICTIONS, TTL_VIBE_SCORE, TTL_PERFORMANCE_PREDICTION
from ..dependencies import DBDependency
from ...ml.config import get_vibe_label as _get_vibe_label
from ..errors import NotFoundError
from ...core.types import EntityType, PLAYER_STATS_TABLES, TEAM_STATS_TABLES
from ...services.transfers import (
    find_player,
    find_team,
    get_team_transfer_links,
    get_transfer_headlines,
    get_player_transfer_links,
    get_trending_transfer_links,
)
from ...services.vibes import (
    get_latest_vibe,
    get_previous_vibe,
    get_trending_vibes as fetch_trending_vibes,
    get_entity_name,
)
from ...services.predictions import (
    get_next_prediction,
    get_specific_prediction,
    get_model_accuracy as fetch_model_accuracy,
    get_recent_stats,
)

logger = logging.getLogger(__name__)

router = APIRouter()

# TTL constants imported from api.cache (single source of truth)


# =============================================================================
# Response Models
# =============================================================================


class TransferTarget(BaseModel):
    """A player linked to a team for potential transfer."""

    player_id: int
    player_name: str
    current_team: str | None
    probability: float = Field(ge=0, le=1)
    confidence_interval: tuple[float, float]
    trend: str = Field(description="up, down, or stable")
    trend_change_7d: float
    top_factors: list[str]
    recent_headlines: list[str] = Field(default_factory=list)


class TeamTransferPredictions(BaseModel):
    """Transfer predictions for a team."""

    team_id: int
    team_name: str
    sport: str
    transfer_window: str = Field(description="open or closed")
    predictions: list[TransferTarget]
    last_updated: datetime


class PlayerTransferOutlook(BaseModel):
    """Transfer outlook for a player."""

    player_id: int
    player_name: str
    current_team: str | None
    sport: str
    linked_teams: list[dict[str, Any]]
    last_updated: datetime


class TrendingTransfer(BaseModel):
    """A trending transfer rumor."""

    player_name: str
    current_team: str | None
    linked_team: str
    probability: float
    trend: str
    mention_count_24h: int
    top_source: str | None


class TrendingTransfers(BaseModel):
    """Trending transfers response."""

    sport: str
    transfers: list[TrendingTransfer]
    last_updated: datetime


class VibeBreakdown(BaseModel):
    """Breakdown of vibe score by source."""

    score: float
    sample_size: int


class VibeTrend(BaseModel):
    """Vibe score trend."""

    direction: str
    change_7d: float
    change_30d: float = 0.0


class VibeScoreResponse(BaseModel):
    """Vibe score for an entity."""

    entity_id: int
    entity_name: str
    entity_type: str
    sport: str
    vibe_score: float = Field(ge=0, le=100)
    vibe_label: str
    breakdown: dict[str, VibeBreakdown]
    trend: VibeTrend
    themes: dict[str, list[str]]
    last_updated: datetime


class TrendingVibe(BaseModel):
    """An entity with notable vibe change."""

    entity_id: int
    entity_name: str
    entity_type: str
    current_score: float
    change_7d: float
    direction: str


class TrendingVibes(BaseModel):
    """Trending vibe changes."""

    sport: str
    trending: list[TrendingVibe]
    last_updated: datetime


class StatPredictionResponse(BaseModel):
    """Prediction for a single statistic."""

    stat_name: str
    predicted_value: float
    confidence_lower: float
    confidence_upper: float
    historical_avg: float


class PerformancePredictionResponse(BaseModel):
    """Performance prediction for an upcoming game."""

    entity_id: int
    entity_name: str
    entity_type: str
    opponent_id: int | None
    opponent_name: str | None
    game_date: str
    sport: str
    predictions: dict[str, StatPredictionResponse]
    confidence_score: float = Field(ge=0, le=1)
    context_factors: dict[str, Any]
    key_factors: list[str]
    model_version: str
    last_updated: datetime


class ModelAccuracyResponse(BaseModel):
    """Model accuracy metrics."""

    model_type: str
    model_version: str
    sport: str | None
    metrics: dict[str, float]
    sample_size: int
    period_start: str | None
    period_end: str | None


# =============================================================================
# Transfer Prediction Endpoints
# =============================================================================


@router.get("/transfers/predictions/{team_id}", response_model=TeamTransferPredictions)
async def get_team_transfer_predictions(
    team_id: int,
    db: DBDependency,
    sport: Annotated[str | None, Query(description="Sport filter")] = None,
) -> TeamTransferPredictions:
    """
    Get transfer predictions for players linked to a team.

    Returns all players currently linked to this team with their
    transfer likelihood probabilities, trends, and key factors.
    """
    cache = get_cache()
    cache_key = f"ml:transfers:team:{team_id}:{sport or 'all'}"

    cached = cache.get(cache_key)
    if cached:
        return TeamTransferPredictions(**cached)

    team_row = find_team(db, team_id)
    if not team_row:
        raise NotFoundError("Team", team_id)

    team_name = team_row["name"]
    sport_name = sport or team_row["sport"]

    links = get_team_transfer_links(db, team_id)

    predictions = []
    for link in links:
        headlines = get_transfer_headlines(db, link["id"])

        probability = link["current_probability"] or 0.0
        confidence_range = 0.1 + 0.1 * (1 - probability)

        predictions.append(
            TransferTarget(
                player_id=link["player_id"],
                player_name=link["player_name"],
                current_team=link["player_current_team"],
                probability=probability,
                confidence_interval=(
                    max(0, probability - confidence_range),
                    min(1, probability + confidence_range),
                ),
                trend=link["trend_direction"] or "stable",
                trend_change_7d=link["trend_change_7d"] or 0.0,
                top_factors=_get_top_factors(link),
                recent_headlines=[h["headline"] for h in headlines],
            )
        )

    # Determine transfer window status
    now = datetime.now()
    window = "closed"
    if sport_name.lower() == "football":
        if now.month == 1 or now.month in [7, 8]:
            window = "open"

    result = TeamTransferPredictions(
        team_id=team_id,
        team_name=team_name,
        sport=sport_name,
        transfer_window=window,
        predictions=predictions,
        last_updated=datetime.now(),
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
    return result


@router.get(
    "/transfers/predictions/player/{player_id}", response_model=PlayerTransferOutlook
)
async def get_player_transfer_predictions(
    player_id: int,
    db: DBDependency,
) -> PlayerTransferOutlook:
    """
    Get transfer outlook for a specific player.

    Returns all teams the player is linked to with probabilities.
    """
    cache = get_cache()
    cache_key = f"ml:transfers:player:{player_id}"

    cached = cache.get(cache_key)
    if cached:
        return PlayerTransferOutlook(**cached)

    player_row = find_player(db, player_id)
    if not player_row:
        raise NotFoundError("Player", player_id)

    links = get_player_transfer_links(db, player_id)

    linked_teams = [
        {
            "team_id": link["team_id"],
            "team_name": link["team_name"],
            "probability": link["current_probability"] or 0.0,
            "trend": link["trend_direction"] or "stable",
            "trend_change_7d": link["trend_change_7d"] or 0.0,
            "total_mentions": link["total_mentions"],
        }
        for link in links
    ]

    result = PlayerTransferOutlook(
        player_id=player_id,
        player_name=player_row["name"],
        current_team=player_row["position"],
        sport=player_row["sport"],
        linked_teams=linked_teams,
        last_updated=datetime.now(),
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
    return result


@router.get("/transfers/trending", response_model=TrendingTransfers)
async def get_trending_transfers(
    db: DBDependency,
    sport: Annotated[str, Query(description="Sport to filter by")],
    limit: Annotated[int, Query(ge=1, le=50)] = 10,
) -> TrendingTransfers:
    """
    Get trending transfer rumors for a sport.

    Returns the hottest transfer rumors based on recent
    mention activity and source quality.
    """
    cache = get_cache()
    cache_key = f"ml:transfers:trending:{sport}:{limit}"

    cached = cache.get(cache_key)
    if cached:
        return TrendingTransfers(**cached)

    links = get_trending_transfer_links(db, sport, limit)

    transfers = [
        TrendingTransfer(
            player_name=link["player_name"],
            current_team=link["player_current_team"],
            linked_team=link["team_name"],
            probability=link["current_probability"] or 0.0,
            trend=link["trend_direction"] or "stable",
            mention_count_24h=link["mentions_24h"] or 0,
            top_source=link["top_source"],
        )
        for link in links
    ]

    result = TrendingTransfers(
        sport=sport,
        transfers=transfers,
        last_updated=datetime.now(),
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
    return result


# =============================================================================
# Vibe Score Endpoints
# =============================================================================


@router.get("/vibe/{entity_type}/{entity_id}", response_model=VibeScoreResponse)
async def get_vibe_score(
    entity_type: EntityType,
    entity_id: int,
    db: DBDependency,
) -> VibeScoreResponse:
    """
    Get current vibe score for an entity (player or team).

    Returns the aggregated sentiment score across Twitter,
    news, and Reddit along with trend data.
    """

    cache = get_cache()
    cache_key = f"ml:vibe:{entity_type}:{entity_id}"

    cached = cache.get(cache_key)
    if cached:
        return VibeScoreResponse(**cached)

    vibe_row = get_latest_vibe(db, entity_type, entity_id)

    if not vibe_row:
        # Return neutral score if no data
        entity_name = get_entity_name(db, entity_type, entity_id)
        return VibeScoreResponse(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            sport="unknown",
            vibe_score=50.0,
            vibe_label="Neutral",
            breakdown={},
            trend=VibeTrend(direction="stable", change_7d=0.0),
            themes={"positive": [], "negative": []},
            last_updated=datetime.now(),
        )

    # Get previous score for trend
    prev_row = get_previous_vibe(
        db, entity_type, entity_id, vibe_row["calculated_at"]
    )

    change_7d = 0.0
    direction = "stable"
    if prev_row:
        change_7d = vibe_row["overall_score"] - prev_row["overall_score"]
        if change_7d > 3:
            direction = "up"
        elif change_7d < -3:
            direction = "down"

    breakdown = {}
    if vibe_row["twitter_score"] is not None:
        breakdown["twitter"] = VibeBreakdown(
            score=vibe_row["twitter_score"],
            sample_size=vibe_row["twitter_sample_size"] or 0,
        )
    if vibe_row["news_score"] is not None:
        breakdown["news"] = VibeBreakdown(
            score=vibe_row["news_score"],
            sample_size=vibe_row["news_sample_size"] or 0,
        )
    if vibe_row["reddit_score"] is not None:
        breakdown["reddit"] = VibeBreakdown(
            score=vibe_row["reddit_score"],
            sample_size=vibe_row["reddit_sample_size"] or 0,
        )

    result = VibeScoreResponse(
        entity_id=entity_id,
        entity_name=vibe_row["entity_name"],
        entity_type=entity_type,
        sport=vibe_row["sport"],
        vibe_score=vibe_row["overall_score"],
        vibe_label=_get_vibe_label(vibe_row["overall_score"]),
        breakdown=breakdown,
        trend=VibeTrend(direction=direction, change_7d=change_7d),
        themes={
            "positive": vibe_row["positive_themes"] or [],
            "negative": vibe_row["negative_themes"] or [],
        },
        last_updated=vibe_row["calculated_at"],
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_VIBE_SCORE)
    return result


@router.get("/vibe/trending/{sport}", response_model=TrendingVibes)
async def get_trending_vibes_endpoint(
    sport: str,
    db: DBDependency,
    limit: Annotated[int, Query(ge=1, le=50)] = 10,
) -> TrendingVibes:
    """
    Get entities with the biggest vibe score changes.

    Returns players and teams whose public perception
    has changed most significantly recently.
    """
    cache = get_cache()
    cache_key = f"ml:vibe:trending:{sport}:{limit}"

    cached = cache.get(cache_key)
    if cached:
        return TrendingVibes(**cached)

    rows = fetch_trending_vibes(db, sport, limit)

    trending = [
        TrendingVibe(
            entity_id=row["entity_id"],
            entity_name=row["entity_name"],
            entity_type=row["entity_type"],
            current_score=row["overall_score"],
            change_7d=row["change"] or 0.0,
            direction="up"
            if (row["change"] or 0) > 0
            else "down"
            if (row["change"] or 0) < 0
            else "stable",
        )
        for row in rows
    ]

    result = TrendingVibes(
        sport=sport,
        trending=trending,
        last_updated=datetime.now(),
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_VIBE_SCORE)
    return result


# =============================================================================
# Helper Functions
# =============================================================================



# _get_vibe_label is imported from ml.config at the top of this file


def _get_top_factors(link: dict[str, Any]) -> list[str]:
    """Extract top factors from a transfer link row."""
    factors = []

    tier_1 = link.get("tier_1_mentions", 0)
    if tier_1 and tier_1 > 0:
        factors.append("Tier 1 Sources")

    total = link.get("total_mentions", 0)
    if total and total > 10:
        factors.append("High Mention Volume")

    trend = link.get("trend_direction", "stable")
    if trend == "up":
        factors.append("Trending Up")

    if not factors:
        factors.append("Recent Activity")

    return factors[:3]


def _get_performance_factors(context: dict[str, Any]) -> list[str]:
    """Extract key factors from context."""
    factors = []

    rest_days = context.get("rest_days")
    if rest_days is not None:
        if rest_days == 0:
            factors.append("Back-to-back game")
        elif rest_days >= 3:
            factors.append("Well rested")

    is_home = context.get("is_home")
    if is_home is not None:
        factors.append("Home game" if is_home else "Road game")

    opp_def = context.get("opponent_defensive_rating")
    if opp_def:
        if opp_def > 115:
            factors.append("Weak opponent defense")
        elif opp_def < 105:
            factors.append("Strong opponent defense")

    return factors[:4] if factors else ["Based on season averages"]


# =============================================================================
# Performance Prediction Endpoints
# =============================================================================


@router.get(
    "/predictions/{entity_type}/{entity_id}/next",
    response_model=PerformancePredictionResponse,
)
async def get_next_game_prediction(
    entity_type: EntityType,
    entity_id: int,
    db: DBDependency,
) -> PerformancePredictionResponse:
    """
    Get performance prediction for entity's next scheduled game.

    Returns projected statistics with confidence intervals
    based on recent performance, opponent strength, and context.
    """

    cache = get_cache()
    cache_key = f"ml:performance:next:{entity_type}:{entity_id}"

    cached = cache.get(cache_key)
    if cached:
        return PerformancePredictionResponse(**cached)

    # Look up entity from sport-specific profile tables
    if entity_type == "player":
        entity_row = find_player(db, entity_id)
        if not entity_row:
            raise NotFoundError(entity_type.title(), entity_id)
        entity_name = entity_row["name"]
        sport = entity_row["sport"]
        position = entity_row["position"]
    else:
        team_row = find_team(db, entity_id)
        if not team_row:
            raise NotFoundError(entity_type.title(), entity_id)
        entity_name = team_row["name"]
        sport = team_row["sport"]
        position = None

    # Check for existing prediction in database
    pred_row = get_next_prediction(db, entity_type, entity_id)

    if pred_row:
        # Use stored prediction
        predictions_data = pred_row["predictions"] or {}
        confidence_intervals = pred_row["confidence_intervals"] or {}

        predictions = {}
        for stat_name, value in predictions_data.items():
            ci = confidence_intervals.get(stat_name, [value * 0.8, value * 1.2])
            predictions[stat_name] = StatPredictionResponse(
                stat_name=stat_name,
                predicted_value=round(value, 1),
                confidence_lower=round(ci[0], 1)
                if isinstance(ci, list)
                else round(value * 0.8, 1),
                confidence_upper=round(ci[1], 1)
                if isinstance(ci, list)
                else round(value * 1.2, 1),
                historical_avg=round(value, 1),  # Placeholder
            )

        context_factors = pred_row["context_factors"] or {}
        key_factors = _get_performance_factors(context_factors)

        result = PerformancePredictionResponse(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            opponent_id=pred_row["opponent_id"],
            opponent_name=pred_row["opponent_name"],
            game_date=str(pred_row["game_date"]),
            sport=sport,
            predictions=predictions,
            confidence_score=pred_row["confidence_score"] or 0.7,
            context_factors=context_factors,
            key_factors=key_factors,
            model_version=pred_row["model_version"] or "v1.0.0",
            last_updated=pred_row["predicted_at"],
        )
    else:
        # Generate heuristic prediction from recent stats
        predictions, confidence, context = _generate_heuristic_prediction(
            db, entity_type, entity_id, sport, position
        )

        result = PerformancePredictionResponse(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            opponent_id=None,
            opponent_name="TBD",
            game_date="TBD",
            sport=sport,
            predictions=predictions,
            confidence_score=confidence,
            context_factors=context,
            key_factors=_get_performance_factors(context),
            model_version="v1.0.0-heuristic",
            last_updated=datetime.now(),
        )

    cache.set(cache_key, result.model_dump(), ttl=TTL_PERFORMANCE_PREDICTION)
    return result


@router.get(
    "/predictions/{entity_type}/{entity_id}/game/{game_id}",
    response_model=PerformancePredictionResponse,
)
async def get_specific_game_prediction(
    entity_type: EntityType,
    entity_id: int,
    game_id: int,
    db: DBDependency,
) -> PerformancePredictionResponse:
    """
    Get performance prediction for a specific game.

    Returns projected statistics for the specified game
    with opponent-adjusted predictions.
    """

    pred_row = get_specific_prediction(db, entity_type, entity_id, game_id)

    if not pred_row:
        raise NotFoundError("Prediction", game_id)

    predictions_data = pred_row["predictions"] or {}
    confidence_intervals = pred_row["confidence_intervals"] or {}

    predictions = {}
    for stat_name, value in predictions_data.items():
        ci = confidence_intervals.get(stat_name, [value * 0.8, value * 1.2])
        predictions[stat_name] = StatPredictionResponse(
            stat_name=stat_name,
            predicted_value=round(value, 1),
            confidence_lower=round(ci[0], 1)
            if isinstance(ci, list)
            else round(value * 0.8, 1),
            confidence_upper=round(ci[1], 1)
            if isinstance(ci, list)
            else round(value * 1.2, 1),
            historical_avg=round(value, 1),
        )

    context_factors = pred_row["context_factors"] or {}

    return PerformancePredictionResponse(
        entity_id=entity_id,
        entity_name=pred_row["entity_name"],
        entity_type=entity_type,
        opponent_id=pred_row["opponent_id"],
        opponent_name=pred_row["opponent_name"],
        game_date=str(pred_row["game_date"]),
        sport=pred_row["sport"],
        predictions=predictions,
        confidence_score=pred_row["confidence_score"] or 0.7,
        context_factors=context_factors,
        key_factors=_get_performance_factors(context_factors),
        model_version=pred_row["model_version"] or "v1.0.0",
        last_updated=pred_row["predicted_at"],
    )


@router.get(
    "/predictions/accuracy/{model_version}", response_model=ModelAccuracyResponse
)
async def get_model_accuracy_endpoint(
    model_version: str,
    db: DBDependency,
    sport: Annotated[str | None, Query(description="Sport filter")] = None,
    model_type: Annotated[str, Query(description="Model type")] = "performance",
) -> ModelAccuracyResponse:
    """
    Get accuracy metrics for a model version.

    Returns MAE, RMSE, and within-range percentage
    for the specified model.
    """
    row = fetch_model_accuracy(db, model_version, model_type, sport)

    if not row:
        # Return placeholder if no accuracy data
        return ModelAccuracyResponse(
            model_type=model_type,
            model_version=model_version,
            sport=sport,
            metrics={
                "mae": 0.0,
                "rmse": 0.0,
                "mape": 0.0,
                "within_range_pct": 0.0,
            },
            sample_size=0,
            period_start=None,
            period_end=None,
        )

    return ModelAccuracyResponse(
        model_type=row["model_type"],
        model_version=row["model_version"],
        sport=row["sport"],
        metrics={
            "mae": row["mae"] or 0.0,
            "rmse": row["rmse"] or 0.0,
            "mape": row["mape"] or 0.0,
            "within_range_pct": row["within_range_pct"] or 0.0,
        },
        sample_size=row["sample_size"] or 0,
        period_start=str(row["period_start"]) if row["period_start"] else None,
        period_end=str(row["period_end"]) if row["period_end"] else None,
    )


# Sport-specific stat column definitions (display logic for heuristic predictions)
_SPORT_STAT_COLS: dict[tuple[str, str], list[str]] = {
    ("NBA", "player"): ["ppg", "rpg", "apg", "spg", "bpg"],
    ("NBA", "team"): ["ppg", "rpg", "apg", "fg_pct", "fg3_pct"],
    ("NFL", "player"): ["pass_yds", "pass_td", "rush_yds", "rec_yds"],
    ("NFL", "team"): ["points_for", "total_yards", "turnovers"],
    ("FOOTBALL", "player"): ["goals", "assists", "shots", "key_passes"],
    ("FOOTBALL", "team"): ["goals_for", "goals_against", "shots_pg"],
}


def _generate_heuristic_prediction(
    db: DBDependency,
    entity_type: str,
    entity_id: int,
    sport: str,
    position: str | None,
) -> tuple[dict[str, StatPredictionResponse], float, dict[str, Any]]:
    """Generate heuristic prediction from recent stats."""
    sport_upper = sport.upper()

    # Determine stats table from centralized registry
    if entity_type == "player":
        stats_table = PLAYER_STATS_TABLES.get(sport_upper)
    else:
        stats_table = TEAM_STATS_TABLES.get(sport_upper)

    if not stats_table:
        return {}, 0.5, {}

    stat_cols = _SPORT_STAT_COLS.get((sport_upper, entity_type))
    if not stat_cols:
        return {}, 0.5, {}

    # Get recent stats via service
    row = get_recent_stats(db, entity_type, entity_id, sport, stats_table, stat_cols)

    predictions = {}
    if row:
        for stat_name in stat_cols:
            val = row.get(stat_name)
            if val is not None:
                val = float(val)
                std = val * 0.2  # Estimate 20% variance

                predictions[stat_name] = StatPredictionResponse(
                    stat_name=stat_name,
                    predicted_value=round(val, 1),
                    confidence_lower=round(max(0, val - 1.5 * std), 1),
                    confidence_upper=round(val + 1.5 * std, 1),
                    historical_avg=round(val, 1),
                )

    context = {
        "rest_days": 2,
        "is_home": True,
        "based_on": "season_averages",
    }

    confidence = 0.6 if predictions else 0.3

    return predictions, confidence, context
