"""
Data provider handlers — fetch from external APIs and normalize to canonical format.

Each handler extends BaseApiClient (HTTP infrastructure) and adds
provider-specific data normalization. Handlers return canonical dicts
that seeders write directly to Postgres without further transformation.

Switching providers means writing a new handler; seeders stay untouched.
"""


# ---------------------------------------------------------------------------
# Shared utilities used by multiple handlers (defined BEFORE handler imports
# so that `from . import extract_value` works without circular import)
# ---------------------------------------------------------------------------


def extract_value(val):
    """Normalize a stat value from various API formats.

    SportMonks returns dicts like {"total": 15, "goals": 12, "penalties": 3}.
    BDL returns flat numbers. This handles both, extracting the aggregate.

    Returns:
        int | float | None — the scalar value, or None if not extractable.
    """
    if val is None:
        return None
    if isinstance(val, (int, float)):
        return val
    if isinstance(val, dict):
        for key in ("total", "all", "count", "average"):
            if key in val and val[key] is not None:
                return val[key]
        return None
    if isinstance(val, str):
        try:
            return float(val) if "." in val else int(val)
        except (ValueError, TypeError):
            return None
    return None


# ---------------------------------------------------------------------------
# Handler classes (imported after utilities to avoid circular import)
# ---------------------------------------------------------------------------

from .balldontlie import BDLNBAHandler, BDLNFLHandler
from .sportmonks import SportMonksHandler

__all__ = [
    "BDLNBAHandler",
    "BDLNFLHandler",
    "SportMonksHandler",
    "extract_value",
]
