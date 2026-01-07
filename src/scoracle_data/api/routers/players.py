"""Player API endpoints."""

from typing import Any, Optional

from fastapi import APIRouter, HTTPException, Query
from fastapi.responses import JSONResponse

from ...queries.players import PlayerQueries
from ..cache import get_cache
from ..dependencies import DBDependency


router = APIRouter()


@router.get("/{player_id}")
async def get_player(
    player_id: int,
    sport: str = Query(..., description="Sport ID (NBA, NFL, FOOTBALL)"),
    season: int = Query(..., description="Season year", ge=2000, le=2030),
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get complete player profile with all stats and percentiles.

    Returns all datapoints for the player - the requesting website will filter
    what it wants to present to the consumer.

    Args:
        player_id: Player ID
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        season: Season year (2000-2030)
        db: Database dependency (injected)

    Returns:
        Complete player profile with stats, team info, and percentiles

    Raises:
        HTTPException: 404 if player not found
    """
    cache = get_cache()

    # Check cache first
    cached = cache.get("player", player_id, sport, season)
    if cached:
        return cached

    # Fetch from database using optimized single-query method
    profile = db.get_player_profile_optimized(player_id, sport, season)

    if not profile:
        raise HTTPException(
            status_code=404,
            detail=f"Player {player_id} not found for {sport} season {season}",
        )

    # Cache result (5 minutes)
    cache.set(profile, "player", player_id, sport, season)

    return profile


@router.get("/{player_id}/stats")
async def get_player_stats_only(
    player_id: int,
    sport: str = Query(..., description="Sport ID (NBA, NFL, FOOTBALL)"),
    season: int = Query(..., description="Season year", ge=2000, le=2030),
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get only player statistics (no percentiles).

    Args:
        player_id: Player ID
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        season: Season year (2000-2030)
        db: Database dependency (injected)

    Returns:
        Player info, team info, and stats only

    Raises:
        HTTPException: 404 if player not found
    """
    cache = get_cache()

    # Check cache
    cached = cache.get("player_stats", player_id, sport, season)
    if cached:
        return cached

    # Get player info
    player = db.get_player(player_id, sport)
    if not player:
        raise HTTPException(
            status_code=404,
            detail=f"Player {player_id} not found for sport {sport}",
        )

    # Get team info if available
    team = None
    if player.get("current_team_id"):
        team = db.get_team(player["current_team_id"], sport)

    # Get stats
    stats = db.get_player_stats(player_id, sport, season)

    result = {
        "player": player,
        "team": team,
        "stats": stats,
    }

    # Cache result
    cache.set(result, "player_stats", player_id, sport, season)

    return result
