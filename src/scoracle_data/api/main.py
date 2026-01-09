"""FastAPI application for Scoracle Data API."""

import time
from datetime import datetime
from typing import Any

import msgspec
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.gzip import GZipMiddleware
from fastapi.responses import Response, JSONResponse
from starlette.status import HTTP_500_INTERNAL_SERVER_ERROR

from .routers import teams, players, intel, entity


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


def create_app() -> FastAPI:
    """
    Create and configure the FastAPI application.

    Returns:
        Configured FastAPI application instance
    """
    app = FastAPI(
        title="Scoracle Data API",
        description="High-performance JSON API for serving team and player statistics",
        version="1.0.0",
        default_response_class=MSGSpecResponse,
        docs_url="/docs",
        redoc_url="/redoc",
    )

    # CORS middleware - allows web clients to access the API
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],  # TODO: Configure for production with specific origins
        allow_methods=["GET", "HEAD", "OPTIONS"],
        allow_headers=["*"],
        allow_credentials=False,
    )

    # GZip compression middleware - compresses responses > 1KB
    app.add_middleware(GZipMiddleware, minimum_size=1000)

    # Request timing middleware
    @app.middleware("http")
    async def add_process_time_header(request: Request, call_next):
        """Add timing header to all responses."""
        start_time = time.time()
        response = await call_next(request)
        process_time = (time.time() - start_time) * 1000  # Convert to ms
        response.headers["X-Process-Time"] = f"{process_time:.2f}ms"
        return response

    # Global exception handler
    @app.exception_handler(Exception)
    async def global_exception_handler(request: Request, exc: Exception):
        """Handle unexpected exceptions."""
        return JSONResponse(
            status_code=HTTP_500_INTERNAL_SERVER_ERROR,
            content={
                "error": "Internal server error",
                "detail": str(exc),
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
        """Cache status check."""
        from .cache import get_cache

        cache = get_cache()
        return {
            "status": "healthy",
            "cache": {
                "entries": cache.size(),
                "ttl_seconds": cache.default_ttl,
            },
            "timestamp": datetime.utcnow().isoformat(),
        }

    @app.get("/", tags=["root"])
    async def root():
        """Root endpoint with API information."""
        return {
            "name": "Scoracle Data API",
            "version": "1.0.0",
            "status": "running",
            "docs": "/docs",
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
