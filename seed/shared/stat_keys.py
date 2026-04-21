"""Provider-key -> canonical-key normalization for stat dicts.

Provider quirks stop at the seeder boundary: each handler imports this
helper and applies its own inline mapping before constructing
EventBoxScore / EventTeamStats / PlayerStats / TeamStats. Postgres only
ever sees canonical keys.
"""

from __future__ import annotations

from typing import Any


def canonicalize(stats: dict[str, Any], mapping: dict[str, str]) -> dict[str, Any]:
    """Translate raw provider keys into canonical keys.

    Resolution order per key:
      1. Explicit `mapping` entry wins.
      2. Otherwise hyphen-to-underscore replacement (e.g. `accurate-passes`
         -> `accurate_passes`).

    Values are passed through untouched. The result is a new dict — the
    input is not mutated.
    """
    out: dict[str, Any] = {}
    for raw_key, value in stats.items():
        out[mapping.get(raw_key) or raw_key.replace("-", "_")] = value
    return out
