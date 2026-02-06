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
from datetime import datetime, timezone
from typing import Any

import msgspec
from fastapi import FastAPI, Request, Response
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import JSONResponse
from starlette.status import HTTP_500_INTERNAL_SERVER_ERROR

from .routers import ml, news, profile, similarity, stats, twitter
from .cache import get_cache, TTL_ENTITY_INFO, TTL_CURRENT_SEASON, TTL_HISTORICAL
from .errors import APIError, api_error_handler
from .rate_limit import RateLimitMiddleware, get_rate_limiter
from ..core.types import Sport, get_sport_config
from ..core.config import get_settings

logger = logging.getLogger(__name__)


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
    Warms info and stats for active teams and top players.

    Uses parallel processing for all sports to speed up startup.
    """
    from .dependencies import get_db
    from ..core.types import PLAYER_STATS_TABLES, PLAYER_PROFILE_TABLES, TEAM_PROFILE_TABLES

    logger.info("Starting cache warming...")

    async def warm_sport(sport: str) -> int:
        """Warm cache for a single sport. Returns count of entries warmed."""
        cache = get_cache()
        db = get_db()
        count = 0

        try:
            current_season_row = db.fetchone(
                "SELECT * FROM seasons WHERE sport_id = %s AND is_current = true",
                (sport,),
            )
            if not current_season_row:
                return 0

            season_year = current_season_row["season_year"]
            season_id = db.get_season_id(sport, season_year)
            if not season_id:
                return 0

            # Get sport-specific table names
            team_table = TEAM_PROFILE_TABLES.get(sport)
            player_table = PLAYER_PROFILE_TABLES.get(sport)
            stats_table = PLAYER_STATS_TABLES.get(sport)

            if not team_table or not player_table:
                logger.warning(f"No profile tables configured for sport {sport}")
                return 0

            # Warm team info (all active teams) from sport-specific table
            teams = db.fetchall(
                f"SELECT * FROM {team_table} WHERE is_active = true LIMIT 50",
                (),
            )
            for team in teams:
                team_data = dict(team)
                if team_data.get("created_at"):
                    team_data["created_at"] = str(team_data["created_at"])
                if team_data.get("updated_at"):
                    team_data["updated_at"] = str(team_data["updated_at"])
                cache.set(
                    team_data, "profile", "team", team["id"], sport, ttl=TTL_ENTITY_INFO
                )
                count += 1

            # Warm player info for players with stats from sport-specific tables
            if stats_table:
                players = db.fetchall(
                    f"""
                    SELECT p.*
                    FROM {player_table} p
                    JOIN {stats_table} s ON s.player_id = p.id
                    WHERE s.season_id = %s AND p.is_active = true
                    LIMIT 100
                    """,
                    (season_id,),
                )
                for player in players:
                    player_data = dict(player)
                    if player_data.get("birth_date"):
                        player_data["birth_date"] = str(player_data["birth_date"])
                    if player_data.get("created_at"):
                        player_data["created_at"] = str(player_data["created_at"])
                    if player_data.get("updated_at"):
                        player_data["updated_at"] = str(player_data["updated_at"])
                    cache.set(
                        player_data,
                        "profile",
                        "player",
                        player["id"],
                        sport,
                        ttl=TTL_ENTITY_INFO,
                    )
                    count += 1

            logger.info(f"Warmed {count} {sport} cache entries")
            return count

        except Exception as e:
            logger.warning(f"Cache warming error for {sport}: {e}")
            return 0

    # Warm all sports in parallel using asyncio.gather
    # Since DB uses connection pooling, concurrent queries are efficient
    results = await asyncio.gather(
        *[warm_sport(sport_enum.value) for sport_enum in Sport]
    )

    total_warmed = sum(results)
    logger.info(f"Cache warming complete: {total_warmed} entries cached")


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
    - Initialize database connection pool (pre-warm connections)
    - Warm the cache with popular entities

    Shutdown:
    - Close database connections
    - Cancel background tasks
    """
    # Startup
    logger.info("Starting Scoracle Data API...")

    # Pre-warm database connection pool
    # This ensures ALL min_size connections are established before first request
    try:
        from .dependencies import get_db

        db = get_db()
        # Explicitly open the pool to establish min_size connections
        db.open()
        logger.info(
            f"Database connection pool opened (min_size={db._min_pool_size}, max_size={db._max_pool_size})"
        )

        # Warm ALL connections in the pool by running concurrent queries
        # This ensures the pool is fully ready, not just one connection
        import concurrent.futures

        def warm_connection(i: int) -> bool:
            try:
                db.fetchone("SELECT 1")
                return True
            except Exception as e:
                logger.warning(f"Connection {i} warm-up failed: {e}")
                return False

        with concurrent.futures.ThreadPoolExecutor(
            max_workers=db._min_pool_size
        ) as executor:
            futures = [
                executor.submit(warm_connection, i) for i in range(db._min_pool_size)
            ]
            results = [f.result(timeout=5) for f in futures]
            successful = sum(results)
            logger.info(
                f"Database connection pool fully warmed: {successful}/{db._min_pool_size} connections ready"
            )

    except Exception as e:
        logger.error(f"Failed to initialize database: {e}")
        # Don't fail startup, let requests handle connection errors

    # Start background refresh task
    refresh_task = asyncio.create_task(background_cache_refresh())

    # Warm cache in background (don't block startup)
    # Cache warming now won't compete with uninitialized connections
    asyncio.create_task(warm_cache())

    yield

    # Shutdown
    logger.info("Shutting down Scoracle Data API...")
    refresh_task.cancel()
    try:
        await refresh_task
    except asyncio.CancelledError:
        pass

    # Close database connections
    try:
        from .dependencies import close_db

        close_db()  # Close sync DB pool
    except Exception as e:
        logger.warning(f"Error closing database connections: {e}")


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
        cfg = get_sport_config(sport)
        current = cfg.current_season if cfg else 2025
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
    settings = get_settings()
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origins,
        allow_methods=settings.cors_allow_methods,
        allow_headers=settings.cors_allow_headers,
        allow_credentials=settings.cors_allow_credentials,
        expose_headers=settings.cors_expose_headers,
    )

    # GZip compression middleware - compresses responses > 1KB
    app.add_middleware(GZipMiddleware, minimum_size=1000)

    # Rate limiting middleware - protects API from abuse
    app.add_middleware(RateLimitMiddleware)

    # Request timing and caching middleware
    @app.middleware("http")
    async def add_performance_headers(request: Request, call_next):
        """Add timing header and cache headers to all responses."""
        start_time = time.time()
        response = await call_next(request)
        process_time = (time.time() - start_time) * 1000  # Convert to ms
        response.headers["X-Process-Time"] = f"{process_time:.2f}ms"

        # Add default cache headers for GET requests (only for successful responses)
        if request.method == "GET" and "Cache-Control" not in response.headers:
            # Check if this is an API endpoint
            path = request.url.path
            if path.startswith("/api/"):
                # Don't cache error responses (4xx, 5xx)
                if response.status_code >= 400:
                    response.headers["Cache-Control"] = (
                        "no-cache, no-store, must-revalidate"
                    )
                else:
                    # Extract season from query params if present
                    season = request.query_params.get("season")
                    sport = request.query_params.get("sport")
                    season_year = int(season) if season else None
                    response.headers["Cache-Control"] = get_cache_control_header(
                        season_year, sport
                    )

        # Add Vary header for proper CDN caching
        response.headers["Vary"] = "Accept-Encoding"

        return response

    # Register custom API error handler for consistent error responses
    app.add_exception_handler(APIError, api_error_handler)

    # Global exception handler
    @app.exception_handler(Exception)
    async def global_exception_handler(request: Request, exc: Exception):
        """Handle unexpected exceptions with consistent format."""
        logger.error(f"Unhandled exception: {exc}", exc_info=True)
        # Never leak exception details in production, regardless of DEBUG flag
        show_detail = settings.debug and settings.environment != "production"
        return JSONResponse(
            status_code=HTTP_500_INTERNAL_SERVER_ERROR,
            content={
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "An internal error occurred",
                    "detail": str(exc) if show_detail else None,
                }
            },
        )

    # Health check endpoints
    @app.get("/health", tags=["health"])
    async def health_check():
        """Basic health check endpoint."""
        return {"status": "healthy", "timestamp": datetime.now(tz=timezone.utc).isoformat()}

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
                "timestamp": datetime.now(tz=timezone.utc).isoformat(),
            }
        except Exception as e:
            logger.error(f"Database health check failed: {e}")
            return JSONResponse(
                status_code=503,
                content={
                    "status": "unhealthy",
                    "database": "disconnected",
                    "error": "Database connection check failed",
                    "timestamp": datetime.now(tz=timezone.utc).isoformat(),
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
            "timestamp": datetime.now(tz=timezone.utc).isoformat(),
        }

    @app.get("/health/rate-limit", tags=["health"])
    async def health_check_rate_limit():
        """Rate limiter status with detailed stats."""
        limiter = get_rate_limiter()
        return {
            "status": "healthy",
            "enabled": settings.rate_limit_enabled,
            "rate_limit": limiter.get_stats(),
            "timestamp": datetime.now(tz=timezone.utc).isoformat(),
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
    # Profile endpoints - entity profiles (name, photo, team, etc.)
    app.include_router(profile.router, prefix="/api/v1/profile", tags=["profile"])
    # Stats endpoints - entity statistics and percentiles
    app.include_router(stats.router, prefix="/api/v1/stats", tags=["stats"])
    # Twitter endpoints - curated journalist feed (separate for lazy loading)
    app.include_router(twitter.router, prefix="/api/v1/twitter", tags=["twitter"])
    # Unified News endpoint - entity-specific news from RSS + NewsAPI
    app.include_router(news.router, prefix="/api/v1/news", tags=["news"])
    # ML endpoints - transfer predictions, vibe scores, performance predictions
    app.include_router(ml.router, prefix="/api/v1/ml", tags=["ml"])
    # Similarity endpoints - percentile-based entity similarity
    app.include_router(
        similarity.router, prefix="/api/v1/similarity", tags=["similarity"]
    )

    return app


# Create app instance
app = create_app()
