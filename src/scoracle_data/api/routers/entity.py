"""
Unified entity router for players and teams.

Provides a single endpoint pattern for both entity types, making frontend
integration simpler - the Astro component doesn't need to know if it's
rendering a player or team widget.

Performance features:
- Aggressive caching with TTL based on data type
- HTTP/2 Link headers for resource preloading
- Response streaming for bulk endpoints
- Cache-aware responses with hit/miss indicators

Usage from frontend:
    const { type, id } = getRouteParams(); // from /players/123 or /teams/456
    const widget = await fetch(`/api/v1/entity/${id}?type=${type}&sport=NBA`);
    const stats = await fetch(`/api/v1/entity/${id}/stats?type=${type}&sport=NBA&season=2024`);
"""

import json
import logging
from enum import Enum
from typing import Annotated, Any, AsyncGenerator

from fastapi import APIRouter, HTTPException, Query, Response
from fastapi.responses import StreamingResponse

from ..cache import (
    get_cache,
    get_ttl_for_season,
    get_ttl_for_entity,
    TTL_CURRENT_SEASON,
    TTL_HISTORICAL,
)
from ..dependencies import DBDependency

logger = logging.getLogger(__name__)

router = APIRouter()

# Current season years by sport
CURRENT_SEASONS = {
    "NBA": 2025,
    "NFL": 2025,
    "FOOTBALL": 2024,
}


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


def _add_link_header(response: Response, entity_id: int, entity_type: str, sport: str, season: int | None = None) -> None:
    """
    Add HTTP/2 Link header for resource preloading.

    This hints to the browser/CDN to preload related resources.
    """
    links = []

    # Preload stats endpoint if we're on entity info
    if season:
        links.append(
            f'</api/v1/entity/{entity_id}/stats?type={entity_type}&sport={sport}&season={season}>; rel="preload"; as="fetch"'
        )

    if links:
        response.headers["Link"] = ", ".join(links)


@router.get("/{entity_id}")
async def get_entity(
    entity_id: int,
    type: Annotated[EntityType, Query(description="Entity type: player or team")],
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    response: Response,
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
    ttl = get_ttl_for_entity()

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"
        # Add Link header for stats preload
        current_season = CURRENT_SEASONS.get(sport.value, 2025)
        _add_link_header(response, entity_id, type.value, sport.value, current_season)
        return cached

    response.headers["X-Cache"] = "MISS"

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

    # Cache result with appropriate TTL
    cache.set(result, *cache_key, ttl=ttl)

    # Set cache headers
    response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"

    # Add Link header for stats preload
    current_season = CURRENT_SEASONS.get(sport.value, 2025)
    _add_link_header(response, entity_id, type.value, sport.value, current_season)

    return result


@router.get("/{entity_id}/stats")
async def get_entity_stats(
    entity_id: int,
    type: Annotated[EntityType, Query(description="Entity type: player or team")],
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int, Query(ge=2000, le=2030, description="Season year")],
    include_percentiles: Annotated[bool, Query(description="Include percentile rankings")] = True,
    response: Response = None,
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

    # Determine TTL based on season (historical vs current)
    current_season = CURRENT_SEASONS.get(sport.value, 2025)
    ttl = get_ttl_for_season(season, current_season)

    # Check cache
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"
        return cached

    response.headers["X-Cache"] = "MISS"

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
    cache.set(result, *cache_key, ttl=ttl)

    # Set cache headers
    response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"

    return result


async def _stream_entities(
    db: Any,
    sport: str,
    entity_type: str,
    season: int,
    limit: int,
) -> AsyncGenerator[bytes, None]:
    """
    Stream entities as newline-delimited JSON (NDJSON).

    This allows the client to start processing data before the full
    response is received, reducing time-to-first-byte for bulk requests.
    """
    # Start JSON array
    yield b'{"entities":['

    first = True

    if entity_type == "player":
        # Get players with stats
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",
            "FOOTBALL": "football_player_stats",
        }
        stats_table = table_map.get(sport)

        if stats_table:
            season_id = db.get_season_id(sport, season)
            if season_id:
                players = db.fetchall(
                    f"""
                    SELECT DISTINCT p.*
                    FROM players p
                    JOIN {stats_table} s ON s.player_id = p.id
                    WHERE p.sport_id = %s AND s.season_id = %s AND p.is_active = true
                    ORDER BY p.id
                    LIMIT %s
                    """,
                    (sport, season_id, limit),
                )

                for player in players:
                    if not first:
                        yield b','
                    first = False

                    team = None
                    if player.get("current_team_id"):
                        team = db.get_team(player["current_team_id"], sport)

                    entity = _normalize_player(player, team)
                    yield json.dumps(entity).encode()

    else:  # team
        teams = db.fetchall(
            "SELECT * FROM teams WHERE sport_id = %s AND is_active = true ORDER BY id LIMIT %s",
            (sport, limit),
        )

        for team in teams:
            if not first:
                yield b','
            first = False

            entity = _normalize_team(team)
            yield json.dumps(entity).encode()

    # End JSON array
    yield b'],"count":'
    count = 0 if first else (len(players) if entity_type == "player" else len(teams))
    yield str(count).encode()
    yield b'}'


@router.get("/bulk/{entity_type}")
async def get_bulk_entities(
    entity_type: Annotated[EntityType, Query(description="Entity type: player or team")],
    sport: Annotated[Sport, Query(description="Sport: NBA, NFL, or FOOTBALL")],
    season: Annotated[int, Query(ge=2000, le=2030, description="Season year")],
    limit: Annotated[int, Query(ge=1, le=500, description="Maximum entities to return")] = 100,
    stream: Annotated[bool, Query(description="Use streaming response")] = False,
    response: Response = None,
    db: DBDependency = None,
) -> Any:
    """
    Get multiple entities in bulk.

    Supports optional streaming for large responses using newline-delimited JSON.

    Args:
        entity_type: player or team
        sport: NBA, NFL, or FOOTBALL
        season: Season year
        limit: Maximum number of entities (1-500)
        stream: If true, returns streaming response

    Response shape:
    ```json
    {
        "entities": [ ... array of entity objects ... ],
        "count": 100
    }
    ```
    """
    cache = get_cache()
    cache_key = ("bulk_entities", entity_type.value, sport.value, season, limit)

    # Determine TTL
    current_season = CURRENT_SEASONS.get(sport.value, 2025)
    ttl = get_ttl_for_season(season, current_season)

    # For streaming, bypass cache and stream directly
    if stream:
        return StreamingResponse(
            _stream_entities(db, sport.value, entity_type.value, season, limit),
            media_type="application/json",
            headers={
                "Cache-Control": f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}",
                "X-Stream": "true",
            },
        )

    # Check cache for non-streaming
    cached = cache.get(*cache_key)
    if cached:
        response.headers["X-Cache"] = "HIT"
        response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"
        return cached

    response.headers["X-Cache"] = "MISS"

    # Build response
    entities = []

    if entity_type == EntityType.player:
        table_map = {
            "NBA": "nba_player_stats",
            "NFL": "nfl_player_stats",
            "FOOTBALL": "football_player_stats",
        }
        stats_table = table_map.get(sport.value)

        if stats_table:
            season_id = db.get_season_id(sport.value, season)
            if season_id:
                players = db.fetchall(
                    f"""
                    SELECT DISTINCT p.*
                    FROM players p
                    JOIN {stats_table} s ON s.player_id = p.id
                    WHERE p.sport_id = %s AND s.season_id = %s AND p.is_active = true
                    ORDER BY p.id
                    LIMIT %s
                    """,
                    (sport.value, season_id, limit),
                )

                for player in players:
                    team = None
                    if player.get("current_team_id"):
                        team = db.get_team(player["current_team_id"], sport.value)
                    entities.append(_normalize_player(player, team))

    else:  # team
        teams = db.fetchall(
            "SELECT * FROM teams WHERE sport_id = %s AND is_active = true ORDER BY id LIMIT %s",
            (sport.value, limit),
        )

        for team in teams:
            entities.append(_normalize_team(team))

    result = {
        "entities": entities,
        "count": len(entities),
    }

    # Cache result
    cache.set(result, *cache_key, ttl=ttl)

    # Set cache headers
    response.headers["Cache-Control"] = f"public, max-age={ttl}, stale-while-revalidate={ttl // 2}"

    return result
