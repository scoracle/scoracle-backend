"""API-Sports HTTP client. Used for metadata (logos, photos) only.

Free tier: 100 requests/day, resets 00:00 UTC. Image URLs returned in
responses are CDN assets and do NOT count toward the quota — the quota
only covers JSON API calls that go through the gateway.

Base URLs are per-sport (v2.nba.api-sports.io, etc.). Auth is a static
header, `x-apisports-key`.
"""

from __future__ import annotations

import logging
import time
from typing import Any

import httpx

logger = logging.getLogger(__name__)


class APISportsClient:
    def __init__(self, base_url: str, api_key: str, min_interval: float = 1.0):
        self._base_url = base_url.rstrip("/")
        self._min_interval = min_interval
        self._last_request = 0.0
        self._client = httpx.Client(
            timeout=30.0,
            headers={"x-apisports-key": api_key},
        )

    def close(self) -> None:
        self._client.close()

    def _throttle(self) -> None:
        elapsed = time.monotonic() - self._last_request
        if elapsed < self._min_interval:
            time.sleep(self._min_interval - elapsed)
        self._last_request = time.monotonic()

    def get(self, path: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self._throttle()
        url = self._base_url + path
        resp = self._client.get(url, params=params or {})
        resp.raise_for_status()
        body = resp.json()
        # api-sports returns {"errors": [...]} on failure with 200 status
        errors = body.get("errors")
        if errors:
            logger.warning("api-sports error payload on %s: %s", path, errors)
        remaining = resp.headers.get("x-ratelimit-requests-remaining")
        if remaining is not None:
            logger.info("api-sports quota remaining: %s", remaining)
        return body
