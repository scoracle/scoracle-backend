"""
Configuration management for Scoracle Data API.

Uses Pydantic settings for type-safe configuration with environment variable support.
All settings can be overridden via environment variables.
"""

import os
from functools import lru_cache
from typing import Optional

from pydantic import Field, computed_field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """
    Application settings with environment variable support.

    All settings can be overridden via environment variables.
    For nested settings, use double underscore: CORS__ALLOW_ORIGINS="http://localhost:3000"
    """

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        env_nested_delimiter="__",
    )

    # ==========================================================================
    # Application Metadata
    # ==========================================================================
    app_name: str = "Scoracle Data API"
    app_version: str = "2.0.0"
    debug: bool = False
    environment: str = Field(default="development", description="development, staging, production")

    # ==========================================================================
    # Database Configuration
    # ==========================================================================
    database_url: Optional[str] = Field(
        default=None,
        description="PostgreSQL connection string",
    )
    neon_database_url: Optional[str] = Field(
        default=None,
        description="Alternative Neon-specific database URL",
    )
    database_pool_size: int = Field(default=10, ge=1, le=50)
    database_pool_timeout: int = Field(default=30, ge=5, le=120)

    @computed_field
    @property
    def db_url(self) -> str:
        """Get the effective database URL."""
        return self.database_url or self.neon_database_url or ""

    # ==========================================================================
    # API Configuration
    # ==========================================================================
    api_host: str = "0.0.0.0"
    api_port: int = 8000
    api_prefix: str = "/api/v1"
    api_docs_url: str = "/docs"
    api_redoc_url: str = "/redoc"

    # ==========================================================================
    # CORS Configuration
    # ==========================================================================
    cors_allow_origins: list[str] = Field(
        default=[
            "http://localhost:3000",
            "http://localhost:4321",
            "http://localhost:5173",
            "http://127.0.0.1:3000",
            "http://127.0.0.1:4321",
            "http://127.0.0.1:5173",
        ],
        description="Allowed CORS origins. Set to ['*'] for development.",
    )
    cors_allow_methods: list[str] = ["GET", "HEAD", "OPTIONS"]
    cors_allow_headers: list[str] = ["Accept", "Accept-Encoding", "Content-Type", "If-None-Match", "Cache-Control"]
    cors_allow_credentials: bool = False
    cors_expose_headers: list[str] = ["X-Process-Time", "X-Cache", "Link"]

    @computed_field
    @property
    def cors_origins(self) -> list[str]:
        """Get CORS origins, adding production URLs if in production."""
        origins = list(self.cors_allow_origins)

        # Add production URLs from environment
        if self.environment == "production":
            prod_origins = os.getenv("CORS_PRODUCTION_ORIGINS", "")
            if prod_origins:
                origins.extend(prod_origins.split(","))

        return origins

    # ==========================================================================
    # External API Keys
    # ==========================================================================
    twitter_bearer_token: Optional[str] = Field(
        default=None,
        description="Twitter API v2 bearer token",
    )
    twitter_journalist_list_id: Optional[str] = Field(
        default=None,
        description="X List ID containing trusted sports journalists",
    )
    twitter_feed_cache_ttl: int = Field(
        default=3600,
        description="TTL for cached journalist feed in seconds (default: 1 hour)",
    )
    news_api_key: Optional[str] = Field(
        default=None,
        description="NewsAPI.org API key",
    )
    reddit_client_id: Optional[str] = Field(
        default=None,
        description="Reddit API client ID",
    )
    reddit_client_secret: Optional[str] = Field(
        default=None,
        description="Reddit API client secret",
    )

    # ==========================================================================
    # Caching Configuration
    # TTL values are centralized in api/cache.py (single source of truth)
    # ==========================================================================
    cache_enabled: bool = True
    cache_backend: str = Field(
        default="memory",
        description="Cache backend: memory, redis",
    )
    redis_url: Optional[str] = Field(
        default=None,
        description="Redis connection URL (redis://host:port/db)",
    )
    cache_warmup_enabled: bool = Field(default=True, description="Enable cache warming on startup")

    # ==========================================================================
    # Rate Limiting
    # ==========================================================================
    rate_limit_enabled: bool = Field(default=True, description="Enable rate limiting")
    rate_limit_requests: int = Field(default=100, description="Requests per window")
    rate_limit_window: int = Field(default=60, description="Window size in seconds")

    # NOTE: Current season values are defined in core.types.SPORT_REGISTRY.
    # Use get_sport_config(sport).current_season for the current season year.


@lru_cache
def get_settings() -> Settings:
    """
    Get cached settings instance.

    Uses lru_cache to ensure settings are only loaded once.
    """
    return Settings()


# Export commonly used settings
settings = get_settings()
