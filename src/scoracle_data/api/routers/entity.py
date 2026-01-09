"""
Unified entity router for players and teams.

Provides a single endpoint pattern for both entity types, making frontend
integration simpler - the Astro component doesn't need to know if it's
rendering a player or team widget.

Usage from frontend:
    const { type, id } = getRouteParams(); // from /players/123 or /teams/456
    const widget = await fetch(`/api/v1/entity/${id}?type=${type}&sport=NBA`);
    const stats = await fetch(`/api/v1/entity/${id}/stats?type=${type}&sport=NBA&season=2024`);
"""

import logging
from enum import Enum
from typing import Annotated, Any

from fastapi import APIRouter, HTTPException, Query

from ..cache import get_cache
from ..dependencies import DBDependency

logger = logging.getLogger(__name__)

router = APIRouter()


class EntityType(str, Enum):
    """Entity types."""
    player = "player"
    team = "team"


class Sport(str, Enum):
    """Supported sports."""
    NBA = "NBA"
    NFL = "NFL"
    FOOTBALL = "FOOTBALL"


def _normalize_player(player: dict, team: dict | None = None) -> dict[str, Any]:
    """Normalize player data to unified entity format."""
    return {
        "id": player["id"],
        "type": "player",
        "sport": player["sport_id"],
        "name": player["full_name"],
        "short_name": f"{player['first_name'][0]}. {player['last_name']}" if player.get("first_name") else player["full_name"],
        "image_url": player.get("photo_url"),
        "subtitle": player.get("position") or player.get("position_group"),
        "meta": {
            "first_name": player.get("first_name"),
            "last_name": player.get("last_name"),
            "position": player.get("position"),
            "position_group": player.get("position_group"),
            "jersey_number": player.get("jersey_number"),
            "nationality": player.get("nationality"),
            "height_inches": player.get("height_inches"),
            "weight_lbs": player.get("weight_lbs"),
            "college": player.get("college"),
            "experience_years": player.get("experience_years"),
            "birth_date": str(player["birth_date"]) if player.get("birth_date") else None,
            "is_active": player.get("is_active", True),
        },
        "team": {
            "id": team["id"],
            "name": team["name"],
            "abbreviation": team.get("abbreviation"),
            "logo_url": team.get("logo_url"),
        } if team else None,
    }


def _normalize_team(team: dict) -> dict[str, Any]:
    """Normalize team data to unified entity format."""
    return {
        "id": team["id"],
        "type": "team",
        "sport": team["sport_id"],
        "name": team["name"],
        "short_name": team.get("abbreviation") or team["name"],
        "image_url": team.get("logo_url"),
        "subtitle": f"{team.get('conference', '')} {team.get('division', '')}".strip() or None,
        "meta": {
            "abbreviation": team.get("abbreviation"),
            "conference": team.get("conference"),
            "division": team.get("division"),
            "city": team.get("city"),
            "country": team.get("country"),
            "founded": team.get("founded"),
            "venue_name": team.get("venue_name"),
            "venue_city": team.get("venue_city"),
            "venue_capacity": team.get("venue_capacity"),
            "is_active": team.get("is_active", True),
        },
        "team": None,  # Teams don't have a parent team
    }


@router.get("/{entity_id}")
async def get_entity(
    entity_id: int,
    type: Annotated[EntityType, Query(description="Entity type: player or team")],
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get basic entity info for widget rendering.

    Returns a normalized response shape regardless of whether it's a player
    or team, making frontend components simpler.

    Response shape:
    ```json
    {
        "id": 123,
        "type": "player",
        "sport": "NBA",
        "name": "LeBron James",
        "short_name": "L. James",
        "image_url": "https://...",
        "subtitle": "Forward",
        "meta": { ... type-specific fields ... },
        "team": { "id": 1, "name": "Lakers", ... } | null
    }
    ```
    """
    cache = get_cache()
    cache_key = ("entity", entity_id, type.value, sport.value)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        return cached

    if type == EntityType.player:
        player = db.get_player(entity_id, sport.value)
        if not player:
            raise HTTPException(
                status_code=404,
                detail=f"Player {entity_id} not found for {sport.value}",
            )

        # Get team info if available
        team = None
        if player.get("current_team_id"):
            team = db.get_team(player["current_team_id"], sport.value)

        result = _normalize_player(player, team)

    else:  # team
        team = db.get_team(entity_id, sport.value)
        if not team:
            raise HTTPException(
                status_code=404,
                detail=f"Team {entity_id} not found for {sport.value}",
            )

        result = _normalize_team(team)

    # Cache result
    cache.set(result, *cache_key)

    return result


@router.get("/{entity_id}/stats")
async def get_entity_stats(
    entity_id: int,
    type: Annotated[EntityType, Query(description="Entity type: player or team")],
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int, Query(ge=2000, le=2030, description="Season year")],
    include_percentiles: Annotated[bool, Query(description="Include percentile rankings")] = True,
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get entity statistics for a season.

    Returns stats and optionally percentile rankings.

    Response shape:
    ```json
    {
        "entity": { ... basic entity info ... },
        "stats": { ... sport-specific stats ... },
        "percentiles": [ ... percentile rankings ... ] | null
    }
    ```
    """
    cache = get_cache()
    cache_key = ("entity_stats", entity_id, type.value, sport.value, season, include_percentiles)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        return cached

    if type == EntityType.player:
        # Get basic player info
        player = db.get_player(entity_id, sport.value)
        if not player:
            raise HTTPException(
                status_code=404,
                detail=f"Player {entity_id} not found for {sport.value}",
            )

        # Get team info
        team = None
        if player.get("current_team_id"):
            team = db.get_team(player["current_team_id"], sport.value)

        entity_info = _normalize_player(player, team)

        # Get stats
        stats = db.get_player_stats(entity_id, sport.value, season)

        # Get percentiles if requested
        percentiles = None
        if include_percentiles:
            percentiles = db.get_percentiles("player", entity_id, sport.value, season)

    else:  # team
        team = db.get_team(entity_id, sport.value)
        if not team:
            raise HTTPException(
                status_code=404,
                detail=f"Team {entity_id} not found for {sport.value}",
            )

        entity_info = _normalize_team(team)

        # Get stats
        stats = db.get_team_stats(entity_id, sport.value, season)

        # Get percentiles if requested
        percentiles = None
        if include_percentiles:
            percentiles = db.get_percentiles("team", entity_id, sport.value, season)

    result = {
        "entity": entity_info,
        "stats": stats,
        "percentiles": percentiles if percentiles else None,
        "season": season,
    }

    # Cache result
    cache.set(result, *cache_key)

    return result
