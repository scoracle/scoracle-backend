"""BallDontLie HTTP client with rate limiting and cursor-based pagination.

Shared by NBA and NFL handlers. Rate limit: 600 req/min.
Auth: Authorization header with API key.
Pagination: cursor-based via meta.next_cursor.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Generator

import httpx

from .http_retry import with_network_retry

logger = logging.getLogger(__name__)


class BDLClient:
    """Rate-limited HTTP client for BallDontLie API."""

    def __init__(self, base_url: str, api_key: str):
        self._base_url = base_url
        self._api_key = api_key
        self._min_interval = 60.0 / 600  # 600 req/min
        self._last_request = 0.0
        self._client = httpx.Client(
            timeout=30.0,
            headers={"Authorization": api_key},
        )

    def close(self) -> None:
        self._client.close()

    def _wait_rate_limit(self) -> None:
        elapsed = time.monotonic() - self._last_request
        if elapsed < self._min_interval:
            time.sleep(self._min_interval - elapsed)
        self._last_request = time.monotonic()

    def get(self, path: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        """Perform a rate-limited GET request. Returns parsed JSON."""
        self._wait_rate_limit()
        url = self._base_url + path
        resp = with_network_retry(
            lambda: self._client.get(url, params=params or {}),
            logger=logger,
        )
        resp.raise_for_status()
        return resp.json()

    def get_paginated(
        self, path: str, params: dict[str, Any] | None = None
    ) -> Generator[list[dict[str, Any]], None, None]:
        """Iterate cursor-paginated responses, yielding each page's data list."""
        params = dict(params or {})
        params.setdefault("per_page", 100)

        while True:
            resp = self.get(path, params)
            data = resp.get("data", [])
            if data:
                yield data

            meta = resp.get("meta", {})
            next_cursor = meta.get("next_cursor")
            if next_cursor is None:
                break
            params["cursor"] = next_cursor

    def get_all_pages(
        self, path: str, params: dict[str, Any] | None = None
    ) -> list[dict[str, Any]]:
        """Fetch all pages and return a flat list of all items."""
        items: list[dict[str, Any]] = []
        for page in self.get_paginated(path, params):
            items.extend(page)
        return items
