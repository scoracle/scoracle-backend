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

from ..cache import get_cache
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ..types import EntityType
from ...core.types import PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

logger = logging.getLogger(__name__)


def _find_player(db: DBDependency, player_id: int) -> tuple | None:
    """Look up a player across all sport-specific profile tables.

    Returns (id, name, sport, position, team_id, team_name) or None.
    Uses sport-specific tables instead of the deprecated cross-sport UNION views.
    """
    for sport_id, profile_table in PLAYER_PROFILE_TABLES.items():
        team_table = TEAM_PROFILE_TABLES[sport_id]
        row = db.fetch_one(
            f"""
            SELECT p.id,
                   COALESCE(p.full_name, p.first_name || ' ' || p.last_name) as name,
                   '{sport_id}' as sport,
                   p.position,
                   t.id as team_id,
                   t.name as team_name
            FROM {profile_table} p
            LEFT JOIN {team_table} t ON t.id = p.team_id
            WHERE p.id = %s
            """,
            (player_id,),
        )
        if row:
            return row
    return None


def _find_team(db: DBDependency, team_id: int) -> tuple | None:
    """Look up a team across all sport-specific profile tables.

    Returns (id, name, sport) or None.
    Uses sport-specific tables instead of the deprecated cross-sport UNION views.
    """
    for sport_id, team_table in TEAM_PROFILE_TABLES.items():
        row = db.fetch_one(
            f"""
            SELECT t.id, t.name, '{sport_id}' as sport
            FROM {team_table} t
            WHERE t.id = %s
            """,
            (team_id,),
        )
        if row:
            return row
    return None

router = APIRouter()

# Cache TTLs
TTL_TRANSFER_PREDICTIONS = 1800  # 30 minutes
TTL_VIBE_SCORE = 3600  # 1 hour
TTL_PERFORMANCE_PREDICTION = 3600  # 1 hour


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

    # Look up team from sport-specific profile tables
    team_row = _find_team(db, team_id)

    if not team_row:
        raise NotFoundError(f"Team with ID {team_id} not found")

    team_name = team_row[1]
    sport_name = sport or team_row[2]

    # Get transfer links for this team
    links = db.fetch_all(
        """
        SELECT
            tl.id, tl.player_id, tl.player_name, tl.player_current_team,
            tl.current_probability, tl.previous_probability,
            tl.trend_direction, tl.trend_change_7d,
            tl.total_mentions, tl.tier_1_mentions
        FROM transfer_links tl
        WHERE tl.team_id = %s AND tl.is_active = TRUE
        ORDER BY tl.current_probability DESC NULLS LAST
        LIMIT 20
        """,
        (team_id,),
    )

    predictions = []
    for link in links:
        # Get recent headlines
        headlines = db.fetch_all(
            """
            SELECT headline FROM transfer_mentions
            WHERE transfer_link_id = %s
            ORDER BY mentioned_at DESC
            LIMIT 3
            """,
            (link[0],),
        )

        probability = link[4] or 0.0
        confidence_range = 0.1 + 0.1 * (1 - probability)

        predictions.append(
            TransferTarget(
                player_id=link[1],
                player_name=link[2],
                current_team=link[3],
                probability=probability,
                confidence_interval=(
                    max(0, probability - confidence_range),
                    min(1, probability + confidence_range),
                ),
                trend=link[6] or "stable",
                trend_change_7d=link[7] or 0.0,
                top_factors=_get_top_factors(link),
                recent_headlines=[h[0] for h in headlines],
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

    # Look up player from sport-specific profile tables
    player_row = _find_player(db, player_id)

    if not player_row:
        raise NotFoundError(f"Player with ID {player_id} not found")

    # Get linked teams
    links = db.fetch_all(
        """
        SELECT
            tl.team_id, tl.team_name, tl.current_probability,
            tl.trend_direction, tl.trend_change_7d, tl.total_mentions
        FROM transfer_links tl
        WHERE tl.player_id = %s AND tl.is_active = TRUE
        ORDER BY tl.current_probability DESC NULLS LAST
        """,
        (player_id,),
    )

    linked_teams = [
        {
            "team_id": link[0],
            "team_name": link[1],
            "probability": link[2] or 0.0,
            "trend": link[3] or "stable",
            "trend_change_7d": link[4] or 0.0,
            "total_mentions": link[5],
        }
        for link in links
    ]

    result = PlayerTransferOutlook(
        player_id=player_id,
        player_name=player_row[1],
        current_team=player_row[3],
        sport=player_row[2],
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

    # Get trending links
    links = db.fetch_all(
        """
        SELECT
            tl.player_name, tl.player_current_team, tl.team_name,
            tl.current_probability, tl.trend_direction,
            (SELECT COUNT(*) FROM transfer_mentions tm
             WHERE tm.transfer_link_id = tl.id
             AND tm.mentioned_at >= NOW() - INTERVAL '24 hours') as mentions_24h,
            (SELECT source_name FROM transfer_mentions tm
             WHERE tm.transfer_link_id = tl.id
             ORDER BY tm.source_tier ASC, tm.mentioned_at DESC
             LIMIT 1) as top_source
        FROM transfer_links tl
        WHERE LOWER(tl.sport) = LOWER(%s) AND tl.is_active = TRUE
        ORDER BY
            (tl.tier_1_mentions * 3 + tl.tier_2_mentions * 2 + tl.total_mentions) DESC,
            tl.current_probability DESC NULLS LAST
        LIMIT %s
        """,
        (sport, limit),
    )

    transfers = [
        TrendingTransfer(
            player_name=link[0],
            current_team=link[1],
            linked_team=link[2],
            probability=link[3] or 0.0,
            trend=link[4] or "stable",
            mention_count_24h=link[5] or 0,
            top_source=link[6],
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

    # Get latest vibe score
    vibe_row = db.fetch_one(
        """
        SELECT
            vs.entity_name, vs.sport, vs.overall_score,
            vs.twitter_score, vs.twitter_sample_size,
            vs.news_score, vs.news_sample_size,
            vs.reddit_score, vs.reddit_sample_size,
            vs.positive_themes, vs.negative_themes,
            vs.calculated_at
        FROM vibe_scores vs
        WHERE vs.entity_type = %s AND vs.entity_id = %s
        ORDER BY vs.calculated_at DESC
        LIMIT 1
        """,
        (entity_type, entity_id),
    )

    if not vibe_row:
        # Return neutral score if no data
        entity_name = _get_entity_name(db, entity_type, entity_id)
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
    prev_row = db.fetch_one(
        """
        SELECT overall_score
        FROM vibe_scores
        WHERE entity_type = %s AND entity_id = %s
        AND calculated_at < %s
        ORDER BY calculated_at DESC
        LIMIT 1
        """,
        (entity_type, entity_id, vibe_row[11]),
    )

    change_7d = 0.0
    direction = "stable"
    if prev_row:
        change_7d = vibe_row[2] - prev_row[0]
        if change_7d > 3:
            direction = "up"
        elif change_7d < -3:
            direction = "down"

    breakdown = {}
    if vibe_row[3] is not None:
        breakdown["twitter"] = VibeBreakdown(
            score=vibe_row[3], sample_size=vibe_row[4] or 0
        )
    if vibe_row[5] is not None:
        breakdown["news"] = VibeBreakdown(
            score=vibe_row[5], sample_size=vibe_row[6] or 0
        )
    if vibe_row[7] is not None:
        breakdown["reddit"] = VibeBreakdown(
            score=vibe_row[7], sample_size=vibe_row[8] or 0
        )

    result = VibeScoreResponse(
        entity_id=entity_id,
        entity_name=vibe_row[0],
        entity_type=entity_type,
        sport=vibe_row[1],
        vibe_score=vibe_row[2],
        vibe_label=_get_vibe_label(vibe_row[2]),
        breakdown=breakdown,
        trend=VibeTrend(direction=direction, change_7d=change_7d),
        themes={
            "positive": vibe_row[9] or [],
            "negative": vibe_row[10] or [],
        },
        last_updated=vibe_row[11],
    )

    cache.set(cache_key, result.model_dump(), ttl=TTL_VIBE_SCORE)
    return result


@router.get("/vibe/trending/{sport}", response_model=TrendingVibes)
async def get_trending_vibes(
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

    # Get entities with biggest changes
    rows = db.fetch_all(
        """
        WITH latest_scores AS (
            SELECT DISTINCT ON (entity_type, entity_id)
                entity_id, entity_name, entity_type, overall_score, calculated_at
            FROM vibe_scores
            WHERE LOWER(sport) = LOWER(%s)
            ORDER BY entity_type, entity_id, calculated_at DESC
        ),
        previous_scores AS (
            SELECT DISTINCT ON (entity_type, entity_id)
                entity_id, entity_type, overall_score
            FROM vibe_scores
            WHERE LOWER(sport) = LOWER(%s)
            AND calculated_at < NOW() - INTERVAL '7 days'
            ORDER BY entity_type, entity_id, calculated_at DESC
        )
        SELECT
            l.entity_id, l.entity_name, l.entity_type,
            l.overall_score, (l.overall_score - COALESCE(p.overall_score, l.overall_score)) as change
        FROM latest_scores l
        LEFT JOIN previous_scores p ON l.entity_id = p.entity_id AND l.entity_type = p.entity_type
        ORDER BY ABS(l.overall_score - COALESCE(p.overall_score, l.overall_score)) DESC
        LIMIT %s
        """,
        (sport, sport, limit),
    )

    trending = [
        TrendingVibe(
            entity_id=row[0],
            entity_name=row[1],
            entity_type=row[2],
            current_score=row[3],
            change_7d=row[4] or 0.0,
            direction="up"
            if (row[4] or 0) > 0
            else "down"
            if (row[4] or 0) < 0
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


def _get_entity_name(db: DBDependency, entity_type: str, entity_id: int) -> str:
    """Get entity name from database."""
    table = "players" if entity_type == "player" else "teams"
    row = db.fetch_one(f"SELECT name FROM {table} WHERE id = %s", (entity_id,))
    return row[0] if row else "Unknown"


def _get_vibe_label(score: float) -> str:
    """Convert vibe score to label."""
    if score >= 90:
        return "Elite"
    elif score >= 75:
        return "Positive"
    elif score >= 60:
        return "Neutral-Positive"
    elif score >= 40:
        return "Neutral"
    elif score >= 25:
        return "Neutral-Negative"
    elif score >= 10:
        return "Negative"
    else:
        return "Crisis"


def _get_top_factors(link: tuple) -> list[str]:
    """Extract top factors from a transfer link row."""
    factors = []

    # link indices: tier_1_mentions=8
    tier_1 = link[9] if len(link) > 9 else 0

    if tier_1 and tier_1 > 0:
        factors.append("Tier 1 Sources")

    total = link[8] if len(link) > 8 else 0
    if total and total > 10:
        factors.append("High Mention Volume")

    trend = link[6] if len(link) > 6 else "stable"
    if trend == "up":
        factors.append("Trending Up")

    if not factors:
        factors.append("Recent Activity")

    return factors[:3]


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
        entity_row = _find_player(db, entity_id)
    else:
        team_row = _find_team(db, entity_id)
        # Reshape to match expected (id, name, sport, position, team_id, team_name)
        entity_row = (*team_row, None, team_row[0], team_row[1]) if team_row else None

    if not entity_row:
        raise NotFoundError(f"{entity_type.title()} with ID {entity_id} not found")

    entity_name = entity_row[1]
    sport = entity_row[2]
    position = entity_row[3]

    # Check for existing prediction in database
    pred_row = db.fetch_one(
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

    if pred_row:
        # Use stored prediction
        predictions_data = pred_row[3] or {}
        confidence_intervals = pred_row[4] or {}

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

        context_factors = pred_row[6] or {}
        key_factors = _get_performance_factors(context_factors)

        result = PerformancePredictionResponse(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            opponent_id=pred_row[0],
            opponent_name=pred_row[1],
            game_date=str(pred_row[2]),
            sport=sport,
            predictions=predictions,
            confidence_score=pred_row[5] or 0.7,
            context_factors=context_factors,
            key_factors=key_factors,
            model_version=pred_row[7] or "v1.0.0",
            last_updated=pred_row[8],
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

    # Get prediction for specific game
    pred_row = db.fetch_one(
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

    if not pred_row:
        raise NotFoundError(f"Prediction for game {game_id} not found")

    predictions_data = pred_row[5] or {}
    confidence_intervals = pred_row[6] or {}

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

    context_factors = pred_row[8] or {}

    return PerformancePredictionResponse(
        entity_id=entity_id,
        entity_name=pred_row[0],
        entity_type=entity_type,
        opponent_id=pred_row[1],
        opponent_name=pred_row[2],
        game_date=str(pred_row[3]),
        sport=pred_row[4],
        predictions=predictions,
        confidence_score=pred_row[7] or 0.7,
        context_factors=context_factors,
        key_factors=_get_performance_factors(context_factors),
        model_version=pred_row[9] or "v1.0.0",
        last_updated=pred_row[10],
    )


@router.get(
    "/predictions/accuracy/{model_version}", response_model=ModelAccuracyResponse
)
async def get_model_accuracy(
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

    row = db.fetch_one(query, tuple(params))

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
        model_type=row[0],
        model_version=row[1],
        sport=row[2],
        metrics={
            "mae": row[3] or 0.0,
            "rmse": row[4] or 0.0,
            "mape": row[5] or 0.0,
            "within_range_pct": row[6] or 0.0,
        },
        sample_size=row[7] or 0,
        period_start=str(row[8]) if row[8] else None,
        period_end=str(row[9]) if row[9] else None,
    )


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


def _generate_heuristic_prediction(
    db: DBDependency,
    entity_type: str,
    entity_id: int,
    sport: str,
    position: str | None,
) -> tuple[dict[str, StatPredictionResponse], float, dict[str, Any]]:
    """Generate heuristic prediction from recent stats."""
    sport_upper = sport.upper()
    from ...core.types import PLAYER_STATS_TABLES, TEAM_STATS_TABLES

    # Determine stats table from centralized registry
    if entity_type == "player":
        stats_table = PLAYER_STATS_TABLES.get(sport_upper)
    else:
        stats_table = TEAM_STATS_TABLES.get(sport_upper)

    if not stats_table:
        return {}, 0.5, {}

    # Determine sport-specific stat columns
    _SPORT_STAT_COLS = {
        ("NBA", "player"): ["ppg", "rpg", "apg", "spg", "bpg"],
        ("NBA", "team"): ["ppg", "rpg", "apg", "fg_pct", "fg3_pct"],
        ("NFL", "player"): ["pass_yds", "pass_td", "rush_yds", "rec_yds"],
        ("NFL", "team"): ["points_for", "total_yards", "turnovers"],
        ("FOOTBALL", "player"): ["goals", "assists", "shots", "key_passes"],
        ("FOOTBALL", "team"): ["goals_for", "goals_against", "shots_pg"],
    }
    stat_cols = _SPORT_STAT_COLS.get((sport_upper, entity_type))
    if not stat_cols:
        return {}, 0.5, {}

    # Get recent stats
    id_col = "player_id" if entity_type == "player" else "team_id"

    # Build query for available columns
    col_list = ", ".join(stat_cols)
    row = db.fetch_one(
        f"""
        SELECT {col_list}
        FROM {stats_table}
        WHERE {id_col} = %s
        ORDER BY season_id DESC
        LIMIT 1
        """,
        (entity_id,),
    )

    predictions = {}
    if row:
        for i, stat_name in enumerate(stat_cols):
            if i < len(row) and row[i] is not None:
                val = float(row[i])
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
