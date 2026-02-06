"""
Similarity router - serves entity similarity based on percentile vectors.

Endpoints:
- GET /{entity_type}/{entity_id} - Get similar entities (players/teams)

Data:
- Reads pre-computed similarities from entity_similarities table
- Similarities are computed nightly after percentile calculation

Performance Features:
- 1-hour caching (data updates at most once daily)
- ETag support for conditional requests (304 Not Modified)
"""

import logging
from typing import Annotated, Any

from fastapi import APIRouter, Header, Query, Response
from pydantic import BaseModel, Field
from starlette.status import HTTP_304_NOT_MODIFIED

from ..cache import get_cache, TTL_SIMILARITY
from ..dependencies import DBDependency
from ..errors import NotFoundError
from ...core.types import EntityType, Sport
from ...services.vibes import get_entity_name
from ._utils import (
    set_cache_headers,
    compute_etag,
    check_etag_match,
    set_etag_headers,
    validate_season,
)

logger = logging.getLogger(__name__)

router = APIRouter()

# TTL_SIMILARITY imported from api.cache (single source of truth)


# =============================================================================
# Response Models
# =============================================================================


class SimilarEntityResponse(BaseModel):
    """A similar entity with comparison details."""

    entity_id: int
    entity_name: str
    similarity_score: float = Field(ge=0, le=1)
    similarity_label: str
    shared_traits: list[str]
    key_differences: list[str]


class SimilarEntitiesResponse(BaseModel):
    """Response containing similar entities for a source entity."""

    entity_id: int
    entity_name: str
    entity_type: str
    sport: str
    season: int | None = None
    similar_entities: list[SimilarEntityResponse]


# =============================================================================
# Helper Functions
# =============================================================================


def _get_similarity_label(score: float) -> str:
    """Convert similarity score to human-readable label."""
    if score >= 0.95:
        return "Nearly Identical"
    elif score >= 0.90:
        return "Very Similar"
    elif score >= 0.85:
        return "Similar"
    elif score >= 0.80:
        return "Somewhat Similar"
    elif score >= 0.70:
        return "Moderately Similar"
    else:
        return "Different"


def _get_entity_name(
    db: DBDependency, entity_type: str, entity_id: int, sport: str
) -> str:
    """Get entity name from profile table."""
    return get_entity_name(db, entity_type, entity_id, sport=sport)


# =============================================================================
# Endpoints
# =============================================================================


@router.get("/{entity_type}/{entity_id}", response_model=SimilarEntitiesResponse)
async def get_similar_entities(
    entity_type: EntityType,
    entity_id: int,
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
    db: DBDependency,
    season: Annotated[
        int | None, Query(description="Season year (defaults to current)")
    ] = None,
    limit: Annotated[
        int, Query(ge=1, le=10, description="Number of similar entities")
    ] = 5,
    if_none_match: Annotated[str | None, Header(alias="If-None-Match")] = None,
) -> SimilarEntitiesResponse | Response:
    """
    Get entities most similar to the specified entity.

    Returns the top N most similar players/teams based on percentile
    vector similarity (cosine similarity). Similarities are pre-computed
    nightly after percentile calculation.

    The response includes:
    - similarity_score: 0.0 (different) to 1.0 (identical)
    - similarity_label: Human-readable description
    - shared_traits: Stats where both entities excel similarly
    - key_differences: Stats with largest gaps between entities

    Cached for 1 hour. Supports conditional requests via If-None-Match.
    """
    # Validate and normalize season
    season = validate_season(season, sport.value)

    cache = get_cache()
    cache_key = ("similarity", entity_type.value, entity_id, sport.value, season, limit)

    cached = cache.get(*cache_key)
    if cached:
        etag = compute_etag(cached)
        if check_etag_match(if_none_match, etag):
            return Response(status_code=HTTP_304_NOT_MODIFIED, headers={"ETag": etag})
        set_cache_headers(response, TTL_SIMILARITY, cache_hit=True)
        set_etag_headers(response, etag, TTL_SIMILARITY)
        return SimilarEntitiesResponse(**cached)

    # Get entity name for response
    entity_name = _get_entity_name(db, entity_type.value, entity_id, sport.value)

    if entity_name == "Unknown":
        raise NotFoundError(
            resource=entity_type.value.title(),
            identifier=entity_id,
            context=sport.value,
        )

    # Fetch pre-computed similarities from database
    rows = db.fetchall(
        """
        SELECT 
            similar_entity_id,
            similar_entity_name,
            similarity_score,
            shared_traits,
            key_differences
        FROM entity_similarities
        WHERE entity_type = %s 
          AND entity_id = %s 
          AND sport = %s
          AND season = %s
        ORDER BY rank
        LIMIT %s
        """,
        (entity_type.value, entity_id, sport.value, str(season), limit),
    )

    # Build response
    similar_entities = [
        SimilarEntityResponse(
            entity_id=row["similar_entity_id"],
            entity_name=row["similar_entity_name"],
            similarity_score=row["similarity_score"],
            similarity_label=_get_similarity_label(row["similarity_score"]),
            shared_traits=row["shared_traits"] or [],
            key_differences=row["key_differences"] or [],
        )
        for row in rows
    ]

    result = SimilarEntitiesResponse(
        entity_id=entity_id,
        entity_name=entity_name,
        entity_type=entity_type.value,
        sport=sport.value,
        season=season,
        similar_entities=similar_entities,
    )

    # Cache the result
    cache.set(result.model_dump(), *cache_key, ttl=TTL_SIMILARITY)
    etag = compute_etag(result.model_dump())
    set_cache_headers(response, TTL_SIMILARITY, cache_hit=False)
    set_etag_headers(response, etag, TTL_SIMILARITY)

    return result
