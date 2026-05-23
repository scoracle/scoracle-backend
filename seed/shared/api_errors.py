"""Shared exception types for provider HTTP clients."""

from __future__ import annotations


class RateLimitExhausted(Exception):
    """Raised when provider 429 retries are exhausted (or a hard quota is hit).

    Distinct from a generic per-request failure: the caller should stop the
    whole run and let pending work be picked up on the next invocation, not
    record a failure on the current item.
    """

    def __init__(self, provider: str, detail: str = ""):
        self.provider = provider
        self.detail = detail
        suffix = f": {detail}" if detail else ""
        super().__init__(f"{provider} rate limit exhausted{suffix}")
