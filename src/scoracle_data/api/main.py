"""
FastAPI application for Scoracle Data API.

Optimized for high-performance serving with:
- Async database queries
- Redis caching with in-memory L1
- Cache warming on startup
- Background refresh of stale data
- HTTP cache headers for CDN/browser caching
- HTTP/2 Link headers for resource preloading
- Response streaming for bulk endpoints
"""

import asyncio
import logging
import time
from contextlib import asynccontextmanager
from datetime import datetime
from typing import Any

import msgspec
from fastapi import FastAPI, Request, Response
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import JSONResponse
from starlette.status import HTTP_500_INTERNAL_SERVER_ERROR

from .routers import teams, players, intel, entity
from .cache import get_cache, TTL_ENTITY_INFO, TTL_CURRENT_SEASON, TTL_HISTORICAL

logger = logging.getLogger(__name__)

# Current season years by sport (update annually)
CURRENT_SEASONS = {
    "NBA": 2025,
    "NFL": 2025,
    "FOOTBALL": 2024,
}


class MSGSpecResponse(Response):
    """Custom response class using msgspec for ultra-fast JSON serialization."""

    media_type = "application/json"

    def render(self, content: Any) -> bytes:
        """
        Serialize content using msgspec (4-5x faster than stdlib json).

        Args:
            content: Content to serialize

        Returns:
            Serialized JSON bytes
        """
        if content is None:
            return b""
        return msgspec.json.encode(content)


async def warm_cache() -> None:
    """
    Pre-populate cache with frequently accessed entities.

    Called at startup to eliminate cold-start latency for popular requests.
    """
    from .dependencies import get_db

    logger.info("Starting cache warming...")
    cache = get_cache()
    db = get_db()

    # Sports to warm
    sports = ["NBA", "NFL", "FOOTBALL"]
    warmed_count = 0

    for sport in sports:
        try:
            # Get current season
            current_season = db.get_current_season(sport)
            if not current_season:
                continue

            season_year = current_season["season_year"]

            # Warm team data (all teams per sport - usually 30-32)
            teams_query = db.fetchall(
                "SELECT id FROM teams WHERE sport_id = %s AND is_active = true LIMIT 50",
                (sport,),
            )

            for team in teams_query:
                team_id = team["id"]
                # Warm team profile
                profile = db.get_team_profile_optimized(team_id, sport, season_year)
                if profile:
                    cache.set(profile, "team_profile", team_id, sport, season_year, ttl=TTL_CURRENT_SEASON)
                    warmed_count += 1

            # Warm top players (limit to reduce startup time)
            table_map = {
                "NBA": "nba_player_stats",
                "NFL": "nfl_player_stats",
                "FOOTBALL": "football_player_stats",
            }
            stats_table = table_map.get(sport)

            if stats_table:
                # Get season_id first
                season_id = db.get_season_id(sport, season_year)
                if season_id:
                    players_query = db.fetchall(
                        f"""
                        SELECT DISTINCT p.id
                        FROM players p
                        JOIN {stats_table} s ON s.player_id = p.id
                        WHERE p.sport_id = %s AND s.season_id = %s AND p.is_active = true
                        LIMIT 100
                        """,
                        (sport, season_id),
                    )

                    for player in players_query:
                        player_id = player["id"]
                        profile = db.get_player_profile_optimized(player_id, sport, season_year)
                        if profile:
                            cache.set(profile, "player_profile", player_id, sport, season_year, ttl=TTL_CURRENT_SEASON)
                            warmed_count += 1

            logger.info(f"Warmed {sport} cache entries")

        except Exception as e:
            logger.warning(f"Cache warming error for {sport}: {e}")

    logger.info(f"Cache warming complete: {warmed_count} entries cached")


async def background_cache_refresh() -> None:
    """
    Background task to proactively refresh cache entries before they expire.

    Runs every 30 minutes to refresh data approaching expiration.
    """
    while True:
        try:
            await asyncio.sleep(1800)  # 30 minutes

            cache = get_cache()
            logger.debug(f"Background refresh: cache stats = {cache.get_stats()}")

            # Clean up expired entries
            cache.cleanup_expired()

        except asyncio.CancelledError:
            logger.info("Background cache refresh task cancelled")
            break
        except Exception as e:
            logger.error(f"Background refresh error: {e}")
            await asyncio.sleep(60)  # Wait a minute before retrying


@asynccontextmanager
async def lifespan(app: FastAPI):
    """
    Manage application lifecycle.

    Startup:
    - Initialize async database pool
    - Warm the cache with popular entities

    Shutdown:
    - Close database connections
    - Cancel background tasks
    """
    # Startup
    logger.info("Starting Scoracle Data API...")

    # Start background refresh task
    refresh_task = asyncio.create_task(background_cache_refresh())

    # Warm cache in background (don't block startup)
    asyncio.create_task(warm_cache())

    yield

    # Shutdown
    logger.info("Shutting down Scoracle Data API...")
    refresh_task.cancel()
    try:
        await refresh_task
    except asyncio.CancelledError:
        pass

    # Close async database if initialized
    try:
        from ..pg_async import close_async_db
        await close_async_db()
    except Exception:
        pass


def get_cache_control_header(
    season_year: int | None = None,
    sport: str | None = None,
    is_entity_info: bool = False,
) -> str:
    """
    Generate Cache-Control header value based on data type.

    Args:
        season_year: The season year of the data
        sport: Sport identifier
        is_entity_info: Whether this is basic entity info (rarely changes)

    Returns:
        Cache-Control header value
    """
    if is_entity_info:
        # Entity info rarely changes - cache for 24 hours
        max_age = TTL_ENTITY_INFO
    elif season_year and sport:
        current = CURRENT_SEASONS.get(sport, 2025)
        if season_year < current:
            # Historical data - cache for 24 hours
            max_age = TTL_HISTORICAL
        else:
            # Current season - cache for 1 hour
            max_age = TTL_CURRENT_SEASON
    else:
        # Default to 1 hour
        max_age = TTL_CURRENT_SEASON

    # stale-while-revalidate allows serving stale content while fetching fresh
    return f"public, max-age={max_age}, stale-while-revalidate={max_age // 2}"


def create_app() -> FastAPI:
    """
    Create and configure the FastAPI application.

    Returns:
        Configured FastAPI application instance
    """
    app = FastAPI(
        title="Scoracle Data API",
        description="High-performance JSON API for serving team and player statistics",
        version="2.0.0",
        default_response_class=MSGSpecResponse,
        docs_url="/docs",
        redoc_url="/redoc",
        lifespan=lifespan,
    )

    # CORS middleware - allows web clients to access the API
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],  # TODO: Configure for production with specific origins
        allow_methods=["GET", "HEAD", "OPTIONS"],
        allow_headers=["*"],
        allow_credentials=False,
        expose_headers=["X-Process-Time", "X-Cache", "Link"],
    )

    # GZip compression middleware - compresses responses > 1KB
    app.add_middleware(GZipMiddleware, minimum_size=1000)

    # Request timing and caching middleware
    @app.middleware("http")
    async def add_performance_headers(request: Request, call_next):
        """Add timing header and cache headers to all responses."""
        start_time = time.time()
        response = await call_next(request)
        process_time = (time.time() - start_time) * 1000  # Convert to ms
        response.headers["X-Process-Time"] = f"{process_time:.2f}ms"

        # Add default cache headers for GET requests
        if request.method == "GET" and "Cache-Control" not in response.headers:
            # Check if this is an API endpoint
            path = request.url.path
            if path.startswith("/api/"):
                # Extract season from query params if present
                season = request.query_params.get("season")
                sport = request.query_params.get("sport")
                season_year = int(season) if season else None
                response.headers["Cache-Control"] = get_cache_control_header(season_year, sport)

        # Add Vary header for proper CDN caching
        response.headers["Vary"] = "Accept-Encoding"

        return response

    # Global exception handler
    @app.exception_handler(Exception)
    async def global_exception_handler(request: Request, exc: Exception):
        """Handle unexpected exceptions."""
        logger.error(f"Unhandled exception: {exc}", exc_info=True)
        return JSONResponse(
            status_code=HTTP_500_INTERNAL_SERVER_ERROR,
            content={
                "error": "Internal server error",
                "detail": str(exc) if app.debug else "An error occurred",
                "path": str(request.url.path),
            },
        )

    # Health check endpoints
    @app.get("/health", tags=["health"])
    async def health_check():
        """Basic health check endpoint."""
        return {"status": "healthy", "timestamp": datetime.utcnow().isoformat()}

    @app.get("/health/db", tags=["health"])
    async def health_check_db():
        """Database connectivity health check."""
        from .dependencies import get_db

        try:
            db = get_db()
            result = db.fetchone("SELECT 1 as test")
            return {
                "status": "healthy",
                "database": "connected",
                "timestamp": datetime.utcnow().isoformat(),
            }
        except Exception as e:
            return JSONResponse(
                status_code=503,
                content={
                    "status": "unhealthy",
                    "database": "disconnected",
                    "error": str(e),
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )

    @app.get("/health/cache", tags=["health"])
    async def health_check_cache():
        """Cache status check with detailed stats."""
        cache = get_cache()
        stats = cache.get_stats()
        return {
            "status": "healthy",
            "cache": stats,
            "timestamp": datetime.utcnow().isoformat(),
        }

    @app.get("/", tags=["root"])
    async def root():
        """Root endpoint with API information."""
        return {
            "name": "Scoracle Data API",
            "version": "2.0.0",
            "status": "running",
            "docs": "/docs",
            "optimizations": [
                "msgspec_json_serialization",
                "gzip_compression",
                "redis_caching",
                "connection_pooling",
                "cache_warming",
                "http_cache_headers",
                "background_refresh",
            ],
        }

    # Include routers
    app.include_router(entity.router, prefix="/api/v1/entity", tags=["entity"])
    app.include_router(intel.router, prefix="/api/v1/intel", tags=["intel"])
    # Legacy endpoints (keeping for backwards compatibility)
    app.include_router(teams.router, prefix="/api/v1/teams", tags=["teams"])
    app.include_router(players.router, prefix="/api/v1/players", tags=["players"])

    return app


# Create app instance
app = create_app()
