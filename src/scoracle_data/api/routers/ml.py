"""
ML Router - Machine Learning prediction endpoints.

Endpoints:
- GET /transfers/predictions/{team_id} - Transfer predictions for a team
- GET /transfers/predictions/player/{player_id} - Transfer predictions for a player
- GET /transfers/trending - Trending transfer rumors
- GET /vibe/{entity_type}/{entity_id} - Vibe score for an entity
- GET /vibe/trending/{sport} - Trending vibe changes
- GET /similar/{entity_type}/{entity_id} - Similar entities
- GET /similar/compare/{entity_type}/{id1}/{id2} - Compare two entities

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

logger = logging.getLogger(__name__)

router = APIRouter()

# Cache TTLs
TTL_TRANSFER_PREDICTIONS = 1800  # 30 minutes
TTL_VIBE_SCORE = 3600  # 1 hour
TTL_SIMILARITY = 86400  # 24 hours


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


class SimilarEntityResponse(BaseModel):
    """A similar entity."""

    entity_id: int
    entity_name: str
    similarity_score: float
    similarity_label: str
    shared_traits: list[str]
    key_differences: list[str]


class SimilarEntitiesResponse(BaseModel):
    """Similar entities for a source entity."""

    entity_id: int
    entity_name: str
    entity_type: str
    sport: str
    similar_entities: list[SimilarEntityResponse]


class EntityComparisonResponse(BaseModel):
    """Comparison between two entities."""

    entity_1: dict[str, Any]
    entity_2: dict[str, Any]
    similarity_score: float
    shared_traits: list[str]
    key_differences: list[str]


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

    cached = await cache.get(cache_key)
    if cached:
        return TeamTransferPredictions(**cached)

    # Get team info
    team_row = db.fetch_one(
        """
        SELECT t.id, t.name, s.name as sport_name
        FROM teams t
        JOIN sports s ON t.sport_id = s.id
        WHERE t.id = %s
        """,
        (team_id,),
    )

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

        predictions.append(TransferTarget(
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
        ))

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

    await cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
    return result


@router.get("/transfers/predictions/player/{player_id}", response_model=PlayerTransferOutlook)
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

    cached = await cache.get(cache_key)
    if cached:
        return PlayerTransferOutlook(**cached)

    # Get player info
    player_row = db.fetch_one(
        """
        SELECT p.id, p.name, s.name as sport,
               t.name as team_name
        FROM players p
        JOIN sports s ON p.sport_id = s.id
        LEFT JOIN player_teams pt ON p.id = pt.player_id AND pt.is_current = TRUE
        LEFT JOIN teams t ON pt.team_id = t.id
        WHERE p.id = %s
        """,
        (player_id,),
    )

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

    await cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
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

    cached = await cache.get(cache_key)
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

    await cache.set(cache_key, result.model_dump(), ttl=TTL_TRANSFER_PREDICTIONS)
    return result


# =============================================================================
# Vibe Score Endpoints
# =============================================================================


@router.get("/vibe/{entity_type}/{entity_id}", response_model=VibeScoreResponse)
async def get_vibe_score(
    entity_type: str,
    entity_id: int,
    db: DBDependency,
) -> VibeScoreResponse:
    """
    Get current vibe score for an entity (player or team).

    Returns the aggregated sentiment score across Twitter,
    news, and Reddit along with trend data.
    """
    if entity_type not in ("player", "team"):
        raise NotFoundError(f"Invalid entity type: {entity_type}")

    cache = get_cache()
    cache_key = f"ml:vibe:{entity_type}:{entity_id}"

    cached = await cache.get(cache_key)
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
        breakdown["twitter"] = VibeBreakdown(score=vibe_row[3], sample_size=vibe_row[4] or 0)
    if vibe_row[5] is not None:
        breakdown["news"] = VibeBreakdown(score=vibe_row[5], sample_size=vibe_row[6] or 0)
    if vibe_row[7] is not None:
        breakdown["reddit"] = VibeBreakdown(score=vibe_row[7], sample_size=vibe_row[8] or 0)

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

    await cache.set(cache_key, result.model_dump(), ttl=TTL_VIBE_SCORE)
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

    cached = await cache.get(cache_key)
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
            direction="up" if (row[4] or 0) > 0 else "down" if (row[4] or 0) < 0 else "stable",
        )
        for row in rows
    ]

    result = TrendingVibes(
        sport=sport,
        trending=trending,
        last_updated=datetime.now(),
    )

    await cache.set(cache_key, result.model_dump(), ttl=TTL_VIBE_SCORE)
    return result


# =============================================================================
# Similarity Endpoints
# =============================================================================


@router.get("/similar/{entity_type}/{entity_id}", response_model=SimilarEntitiesResponse)
async def get_similar_entities(
    entity_type: str,
    entity_id: int,
    db: DBDependency,
    limit: Annotated[int, Query(ge=1, le=10)] = 3,
) -> SimilarEntitiesResponse:
    """
    Get similar entities (players similar to a player, teams to a team).

    Returns the most statistically similar entities based on
    performance metrics and playing style.
    """
    if entity_type not in ("player", "team"):
        raise NotFoundError(f"Invalid entity type: {entity_type}")

    cache = get_cache()
    cache_key = f"ml:similar:{entity_type}:{entity_id}:{limit}"

    cached = await cache.get(cache_key)
    if cached:
        return SimilarEntitiesResponse(**cached)

    # Get pre-computed similarities
    rows = db.fetch_all(
        """
        SELECT
            es.entity_name, es.similar_entity_id, es.similar_entity_name,
            es.sport, es.similarity_score, es.shared_traits, es.key_differences
        FROM entity_similarities es
        WHERE es.entity_type = %s AND es.entity_id = %s
        ORDER BY es.rank
        LIMIT %s
        """,
        (entity_type, entity_id, limit),
    )

    if not rows:
        # Return empty if no pre-computed similarities
        entity_name = _get_entity_name(db, entity_type, entity_id)
        return SimilarEntitiesResponse(
            entity_id=entity_id,
            entity_name=entity_name,
            entity_type=entity_type,
            sport="unknown",
            similar_entities=[],
        )

    similar = [
        SimilarEntityResponse(
            entity_id=row[1],
            entity_name=row[2],
            similarity_score=row[4],
            similarity_label=_get_similarity_label(row[4]),
            shared_traits=row[5] or [],
            key_differences=row[6] or [],
        )
        for row in rows
    ]

    result = SimilarEntitiesResponse(
        entity_id=entity_id,
        entity_name=rows[0][0],
        entity_type=entity_type,
        sport=rows[0][3],
        similar_entities=similar,
    )

    await cache.set(cache_key, result.model_dump(), ttl=TTL_SIMILARITY)
    return result


@router.get("/similar/compare/{entity_type}/{entity_id_1}/{entity_id_2}", response_model=EntityComparisonResponse)
async def compare_entities(
    entity_type: str,
    entity_id_1: int,
    entity_id_2: int,
    db: DBDependency,
) -> EntityComparisonResponse:
    """
    Compare two specific entities directly.

    Returns detailed comparison including similarity score,
    shared traits, and key differences.
    """
    if entity_type not in ("player", "team"):
        raise NotFoundError(f"Invalid entity type: {entity_type}")

    # Get entity names
    name1 = _get_entity_name(db, entity_type, entity_id_1)
    name2 = _get_entity_name(db, entity_type, entity_id_2)

    # Check for pre-computed comparison
    row = db.fetch_one(
        """
        SELECT similarity_score, shared_traits, key_differences
        FROM entity_similarities
        WHERE entity_type = %s AND entity_id = %s AND similar_entity_id = %s
        """,
        (entity_type, entity_id_1, entity_id_2),
    )

    if row:
        return EntityComparisonResponse(
            entity_1={"id": entity_id_1, "name": name1},
            entity_2={"id": entity_id_2, "name": name2},
            similarity_score=row[0],
            shared_traits=row[1] or [],
            key_differences=row[2] or [],
        )

    # Return basic comparison if no pre-computed data
    return EntityComparisonResponse(
        entity_1={"id": entity_id_1, "name": name1},
        entity_2={"id": entity_id_2, "name": name2},
        similarity_score=0.0,
        shared_traits=[],
        key_differences=["Comparison not yet computed"],
    )


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


def _get_similarity_label(score: float) -> str:
    """Convert similarity score to label."""
    if score >= 0.9:
        return "Very Similar"
    elif score >= 0.8:
        return "Similar"
    elif score >= 0.7:
        return "Somewhat Similar"
    else:
        return "Different"


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
