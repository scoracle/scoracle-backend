"""Team API endpoints."""

from typing import Any, Optional

from fastapi import APIRouter, HTTPException, Query
from fastapi.responses import JSONResponse

from ...queries.teams import TeamQueries
from ..cache import get_cache
from ..dependencies import DBDependency


router = APIRouter()


@router.get("/{team_id}")
async def get_team(
    team_id: int,
    sport: str = Query(..., description="Sport ID (NBA, NFL, FOOTBALL)"),
    season: int = Query(..., description="Season year", ge=2000, le=2030),
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get complete team profile with all stats and percentiles.

    Returns all datapoints for the team - the requesting website will filter
    what it wants to present to the consumer.

    Args:
        team_id: Team ID
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        season: Season year (2000-2030)
        db: Database dependency (injected)

    Returns:
        Complete team profile with stats and percentiles

    Raises:
        HTTPException: 404 if team not found
    """
    cache = get_cache()

    # Check cache first
    cached = cache.get("team", team_id, sport, season)
    if cached:
        return cached

    # Fetch from database using optimized single-query method
    profile = db.get_team_profile_optimized(team_id, sport, season)

    if not profile:
        raise HTTPException(
            status_code=404,
            detail=f"Team {team_id} not found for {sport} season {season}",
        )

    # Cache result (5 minutes)
    cache.set(profile, "team", team_id, sport, season)

    return profile


@router.get("/{team_id}/stats")
async def get_team_stats_only(
    team_id: int,
    sport: str = Query(..., description="Sport ID (NBA, NFL, FOOTBALL)"),
    season: int = Query(..., description="Season year", ge=2000, le=2030),
    db: DBDependency = None,
) -> dict[str, Any]:
    """
    Get only team statistics (no percentiles).

    Args:
        team_id: Team ID
        sport: Sport identifier (NBA, NFL, FOOTBALL)
        season: Season year (2000-2030)
        db: Database dependency (injected)

    Returns:
        Team info and stats only

    Raises:
        HTTPException: 404 if team not found
    """
    cache = get_cache()

    # Check cache
    cached = cache.get("team_stats", team_id, sport, season)
    if cached:
        return cached

    # Get team info
    team = db.get_team(team_id, sport)
    if not team:
        raise HTTPException(
            status_code=404,
            detail=f"Team {team_id} not found for sport {sport}",
        )

    # Get stats
    stats = db.get_team_stats(team_id, sport, season)

    result = {
        "team": team,
        "stats": stats,
    }

    # Cache result
    cache.set(result, "team_stats", team_id, sport, season)

    return result
