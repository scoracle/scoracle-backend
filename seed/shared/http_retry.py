"""Retry-with-backoff for transient HTTP/network errors.

Used by the SportMonks and BDL clients to ride out short-lived network
hiccups (DNS resolution failures, connection refused, read timeouts) without
making the seeder operator-dependent. HTTP-level errors (4xx/5xx) are NOT
caught here — those are protocol responses, not transport failures, and
each client handles them with its own logic (e.g. 429 backoff).
"""

from __future__ import annotations

import logging
import time
from typing import Callable, TypeVar

import httpx

T = TypeVar("T")

# httpx exception hierarchy:
#   TransportError
#     ├── TimeoutException  (ConnectTimeout, ReadTimeout, WriteTimeout, PoolTimeout)
#     └── NetworkError      (ConnectError, ReadError, WriteError, CloseError)
#         + RemoteProtocolError, ProxyError, etc.
# Catching both branches covers DNS gaierror, connection refused, network
# unreachable, mid-request resets, and timeouts. We deliberately do NOT catch
# HTTPStatusError or DecodingError — those signal server/protocol problems
# that retrying won't fix.
_TRANSIENT_ERRORS = (httpx.NetworkError, httpx.TimeoutException)


def with_network_retry(
    fn: Callable[[], T],
    *,
    max_attempts: int = 4,
    base_delay: float = 1.0,
    logger: logging.Logger | None = None,
) -> T:
    """Call ``fn()`` with exponential backoff on transient network errors.

    Backoff schedule with defaults: 1s, 2s, 4s before the 4th attempt; total
    wait ≤ 7s before re-raising. Tune ``max_attempts`` / ``base_delay`` for
    callers that want to ride out longer outages.
    """
    last_exc: BaseException | None = None
    for attempt in range(max_attempts):
        try:
            return fn()
        except _TRANSIENT_ERRORS as exc:
            last_exc = exc
            if attempt == max_attempts - 1:
                break
            delay = base_delay * (2 ** attempt)
            if logger is not None:
                logger.warning(
                    "transient network error: %s: %s "
                    "(attempt %d/%d, backing off %.1fs)",
                    exc.__class__.__name__,
                    exc,
                    attempt + 1,
                    max_attempts,
                    delay,
                )
            time.sleep(delay)
    assert last_exc is not None  # loop exited via break, exc is set
    raise last_exc
