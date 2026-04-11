"""SportMonks HTTP client with rate limiting, page-based pagination, and 429 backoff.

Auth: api_token query parameter.
Rate limit: 300 req/min.
Pagination: page-based via pagination.has_more.
429 retry: exponential backoff (2s, 4s, 8s, 16s, 32s), max 5 retries.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Generator

import httpx

logger = logging.getLogger(__name__)

BASE_URL = "https://api.sportmonks.com/v3/football"


class SportMonksClient:
    """Rate-limited HTTP client for SportMonks Football API."""

    def __init__(self, api_token: str):
        self._api_token = api_token
        self._min_interval = 60.0 / 300  # 300 req/min
        self._last_request = 0.0
        self._client = httpx.Client(timeout=30.0)

    def close(self) -> None:
        self._client.close()

    def _wait_rate_limit(self) -> None:
        elapsed = time.monotonic() - self._last_request
        if elapsed < self._min_interval:
            time.sleep(self._min_interval - elapsed)
        self._last_request = time.monotonic()

    def get(self, path: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        """Perform a rate-limited GET with 429 retry and exponential backoff."""
        params = dict(params or {})
        params["api_token"] = self._api_token

        url = BASE_URL + path
        max_retries = 5
        backoff = 2.0

        for attempt in range(max_retries + 1):
            self._wait_rate_limit()
            resp = self._client.get(url, params=params)

            if resp.status_code == 429:
                if attempt == max_retries:
                    resp.raise_for_status()
                logger.warning(
                    "Rate limited (429), backing off %.1fs (attempt %d/%d)",
                    backoff,
                    attempt + 1,
                    max_retries,
                )
                time.sleep(backoff)
                backoff *= 2
                continue

            resp.raise_for_status()
            return resp.json()

        # Should not reach here
        raise RuntimeError(f"SportMonks {path}: exhausted retries")

    def get_paginated(
        self, path: str, params: dict[str, Any] | None = None, per_page: int = 50
    ) -> Generator[list[dict[str, Any]], None, None]:
        """Iterate page-based responses, yielding each page's data list."""
        params = dict(params or {})
        params["per_page"] = per_page
        page = 1

        while True:
            params["page"] = page
            resp = self.get(path, params)
            data = resp.get("data", [])

            # Data can be a single object or a list
            if isinstance(data, dict):
                yield [data]
                break

            if data:
                yield data

            pagination = resp.get("pagination")
            if pagination is None or not pagination.get("has_more", False):
                break
            page += 1

    def get_all_pages(
        self, path: str, params: dict[str, Any] | None = None, per_page: int = 50
    ) -> list[dict[str, Any]]:
        """Fetch all pages and return a flat list of all items."""
        items: list[dict[str, Any]] = []
        for page_data in self.get_paginated(path, params, per_page):
            items.extend(page_data)
        return items
