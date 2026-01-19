"""
NBA sport module.

Provides NBA-specific configurations, provider adapters, and constants.
"""

from pathlib import Path

# Config file location
CONFIG_PATH = Path(__file__).parent / "config.toml"

# NBA-specific constants
CURRENT_SEASON = "2024-25"
SEASON_FORMAT = "YYYY-YY"

# Position groups for percentile comparison
POSITION_GROUPS = {
    "G": "Guard",
    "PG": "Guard",
    "SG": "Guard",
    "F": "Forward",
    "SF": "Forward",
    "PF": "Forward",
    "C": "Center",
    "G-F": "Guard",
    "F-G": "Guard",
    "F-C": "Forward",
    "C-F": "Forward",
}


def get_position_group(position: str | None) -> str:
    """Get the position group for percentile comparison."""
    if not position:
        return "Unknown"
    return POSITION_GROUPS.get(position.upper(), "Unknown")


__all__ = [
    "CONFIG_PATH",
    "CURRENT_SEASON",
    "SEASON_FORMAT",
    "POSITION_GROUPS",
    "get_position_group",
]
