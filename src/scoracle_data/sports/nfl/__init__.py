"""
NFL sport module.

Provides NFL-specific configurations, provider adapters, and constants.
"""

from pathlib import Path

# Config file location
CONFIG_PATH = Path(__file__).parent / "config.toml"

# NFL-specific constants
CURRENT_SEASON = "2024"
SEASON_FORMAT = "YYYY"

# Position groups for percentile comparison
POSITION_GROUPS = {
    # Offense
    "QB": "Quarterback",
    "RB": "Running Back",
    "FB": "Running Back",
    "WR": "Wide Receiver",
    "TE": "Tight End",
    "OL": "Offensive Line",
    "OT": "Offensive Line",
    "OG": "Offensive Line",
    "C": "Offensive Line",
    # Defense
    "DL": "Defensive Line",
    "DE": "Defensive Line",
    "DT": "Defensive Line",
    "NT": "Defensive Line",
    "LB": "Linebacker",
    "ILB": "Linebacker",
    "OLB": "Linebacker",
    "MLB": "Linebacker",
    "DB": "Defensive Back",
    "CB": "Defensive Back",
    "S": "Defensive Back",
    "SS": "Defensive Back",
    "FS": "Defensive Back",
    # Special Teams
    "K": "Kicker",
    "P": "Punter",
    "LS": "Long Snapper",
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
