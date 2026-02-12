"""
API contract tests for Scoracle Data API.

These tests validate the response shape of every API endpoint.
They serve as the contract between the Python API and the future
Go API — both must produce responses matching these schemas.

Tests validate STRUCTURE, not specific data values. They pass
against any database state (empty or populated).

Requirements:
    - DATABASE_URL must be set (tests are skipped otherwise)
    - pip install httpx pytest-asyncio
"""

from __future__ import annotations

import os

import pytest

# Skip entire module if no database URL is available
pytestmark = pytest.mark.skipif(
    not (
        os.environ.get("DATABASE_URL")
        or os.environ.get("NEON_DATABASE_URL")
        or os.environ.get("NEON_DATABASE_URL_V2")
    ),
    reason="DATABASE_URL not set — skipping API contract tests",
)


@pytest.fixture(scope="module")
def client():
    """Create a sync test client for the FastAPI app.

    Uses Starlette's TestClient which handles the async lifespan
    and provides a synchronous interface for test simplicity.
    """
    from starlette.testclient import TestClient
    from scoracle_data.api.main import app

    with TestClient(app) as c:
        yield c


# =========================================================================
# Health endpoints
# =========================================================================


class TestHealthEndpoints:
    """Health endpoints must always return a consistent shape."""

    def test_health_basic(self, client):
        r = client.get("/health")
        assert r.status_code == 200
        data = r.json()
        assert data["status"] == "healthy"
        assert "timestamp" in data

    def test_health_db(self, client):
        r = client.get("/health/db")
        # Can be 200 or 503 depending on DB state
        data = r.json()
        assert "status" in data
        assert "database" in data
        assert "timestamp" in data

    def test_health_cache(self, client):
        r = client.get("/health/cache")
        assert r.status_code == 200
        data = r.json()
        assert data["status"] == "healthy"
        assert "cache" in data

    def test_health_rate_limit(self, client):
        r = client.get("/health/rate-limit")
        assert r.status_code == 200
        data = r.json()
        assert "enabled" in data
        assert "rate_limit" in data


# =========================================================================
# Root endpoint
# =========================================================================


class TestRootEndpoint:
    def test_root(self, client):
        r = client.get("/")
        assert r.status_code == 200
        data = r.json()
        assert data["name"] == "Scoracle Data API"
        assert "version" in data
        assert "status" in data


# =========================================================================
# Profile endpoints
# =========================================================================


class TestProfileContract:
    """Profile endpoint response shape validation."""

    def test_player_profile_not_found(self, client):
        """Non-existent player returns consistent error shape."""
        r = client.get("/api/v1/profile/player/999999?sport=NBA")
        assert r.status_code == 404
        data = r.json()
        assert "error" in data
        assert "code" in data["error"]
        assert "message" in data["error"]

    def test_team_profile_not_found(self, client):
        """Non-existent team returns consistent error shape."""
        r = client.get("/api/v1/profile/team/999999?sport=NBA")
        assert r.status_code == 404
        data = r.json()
        assert "error" in data
        assert "code" in data["error"]

    def test_profile_missing_sport(self, client):
        """Missing required sport parameter returns 422."""
        r = client.get("/api/v1/profile/player/1")
        assert r.status_code == 422

    def test_profile_invalid_entity_type(self, client):
        """Invalid entity type returns 422."""
        r = client.get("/api/v1/profile/invalid/1?sport=NBA")
        assert r.status_code == 422


# =========================================================================
# Stats endpoints
# =========================================================================


class TestStatsContract:
    """Stats endpoint response shape validation."""

    def test_stats_not_found(self, client):
        """Non-existent entity stats returns 404 with error shape."""
        r = client.get("/api/v1/stats/player/999999?sport=NBA&season=2025")
        assert r.status_code == 404
        data = r.json()
        assert "error" in data
        assert "code" in data["error"]

    def test_stats_missing_sport(self, client):
        """Missing required sport parameter returns 422."""
        r = client.get("/api/v1/stats/player/1")
        assert r.status_code == 422

    def test_stats_invalid_season(self, client):
        """Season before 2000 returns validation error."""
        r = client.get("/api/v1/stats/player/1?sport=NBA&season=1990")
        assert r.status_code == 400
        data = r.json()
        assert "error" in data

    def test_stats_future_season(self, client):
        """Season too far in the future returns validation error."""
        r = client.get("/api/v1/stats/player/1?sport=NBA&season=2099")
        assert r.status_code == 400

    def test_seasons_not_found(self, client):
        """Available seasons for non-existent entity returns valid shape."""
        r = client.get("/api/v1/stats/player/999999/seasons?sport=NBA")
        assert r.status_code == 200
        data = r.json()
        assert "entity_id" in data
        assert "entity_type" in data
        assert "sport" in data
        assert "seasons" in data
        assert isinstance(data["seasons"], list)


# =========================================================================
# Stat definitions endpoint
# =========================================================================


class TestStatDefinitionsContract:
    """Stat definitions endpoint response shape validation."""

    def test_definitions_shape(self, client):
        """Stat definitions returns list with expected fields."""
        r = client.get("/api/v1/stats/definitions?sport=NBA")
        assert r.status_code == 200
        data = r.json()
        assert "sport" in data
        assert data["sport"] == "NBA"
        assert "definitions" in data
        assert "count" in data
        assert isinstance(data["definitions"], list)

        # If definitions exist, validate individual shape
        if data["definitions"]:
            defn = data["definitions"][0]
            assert "sport" in defn
            assert "stat_key" in defn
            assert "display_name" in defn

    def test_definitions_all_sports(self, client):
        """Stat definitions work for all supported sports."""
        for sport in ["NBA", "NFL", "FOOTBALL"]:
            r = client.get(f"/api/v1/stats/definitions?sport={sport}")
            assert r.status_code == 200
            data = r.json()
            assert data["sport"] == sport

    def test_definitions_missing_sport(self, client):
        """Missing sport parameter returns 422."""
        r = client.get("/api/v1/stats/definitions")
        assert r.status_code == 422


# =========================================================================
# News endpoints
# =========================================================================


class TestNewsContract:
    """News endpoint response shape validation."""

    def test_news_not_found_entity(self, client):
        """News for non-existent entity returns 404."""
        r = client.get("/api/v1/news/player/999999?sport=NBA")
        assert r.status_code == 404
        data = r.json()
        assert "error" in data

    def test_news_status(self, client):
        """News status returns configuration info."""
        r = client.get("/api/v1/news/status")
        assert r.status_code == 200
        data = r.json()
        assert "rss_available" in data
        assert "newsapi_configured" in data


# =========================================================================
# Twitter endpoints
# =========================================================================


class TestTwitterContract:
    """Twitter endpoint response shape validation."""

    def test_twitter_status(self, client):
        """Twitter status returns configuration info."""
        r = client.get("/api/v1/twitter/status")
        assert r.status_code == 200
        data = r.json()
        assert "configured" in data
        assert "journalist_list_configured" in data

    def test_journalist_feed_missing_query(self, client):
        """Missing query parameter returns 422."""
        r = client.get("/api/v1/twitter/journalist-feed")
        assert r.status_code == 422


# =========================================================================
# Cache and ETag behavior
# =========================================================================


class TestCacheBehavior:
    """Validate caching headers and ETag support."""

    def test_cache_control_on_api_response(self, client):
        """API responses include Cache-Control header."""
        r = client.get("/api/v1/stats/player/999999/seasons?sport=NBA")
        assert "Cache-Control" in r.headers

    def test_vary_header_present(self, client):
        """Vary header set for CDN caching."""
        r = client.get("/health")
        assert "Vary" in r.headers

    def test_process_time_header(self, client):
        """X-Process-Time header present on all responses."""
        r = client.get("/health")
        assert "X-Process-Time" in r.headers


# =========================================================================
# Error response format
# =========================================================================


class TestErrorFormat:
    """All errors must follow the consistent error response format."""

    def test_404_error_shape(self, client):
        """404 errors have standard error body."""
        r = client.get("/api/v1/profile/player/999999?sport=NBA")
        assert r.status_code == 404
        data = r.json()
        assert "error" in data
        error = data["error"]
        assert "code" in error
        assert "message" in error
        assert error["code"] == "NOT_FOUND"

    def test_422_error_shape(self, client):
        """FastAPI validation errors return 422."""
        r = client.get("/api/v1/profile/player/not_an_int?sport=NBA")
        assert r.status_code == 422
