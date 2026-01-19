"""
Football (Soccer) sport module.

Provides Football-specific configurations, provider adapters, and constants.
"""

from pathlib import Path

# Config file location
CONFIG_PATH = Path(__file__).parent / "config.toml"

# Football-specific constants
CURRENT_SEASON = "2024"
SEASON_FORMAT = "YYYY"

# Top 5 European Leagues for percentile comparison
TOP_5_LEAGUES = {
    39: "Premier League",
    140: "La Liga",
    78: "Bundesliga",
    135: "Serie A",
    61: "Ligue 1",
}

# Position groups for percentile comparison
POSITION_GROUPS = {
    "G": "Goalkeeper",
    "GK": "Goalkeeper",
    "Goalkeeper": "Goalkeeper",
    "D": "Defender",
    "CB": "Defender",
    "LB": "Defender",
    "RB": "Defender",
    "Defender": "Defender",
    "M": "Midfielder",
    "CM": "Midfielder",
    "CDM": "Midfielder",
    "CAM": "Midfielder",
    "LM": "Midfielder",
    "RM": "Midfielder",
    "Midfielder": "Midfielder",
    "F": "Forward",
    "ST": "Forward",
    "CF": "Forward",
    "LW": "Forward",
    "RW": "Forward",
    "Attacker": "Forward",
}


def get_position_group(position: str | None) -> str:
    """Get the position group for percentile comparison."""
    if not position:
        return "Unknown"
    return POSITION_GROUPS.get(position, POSITION_GROUPS.get(position.upper(), "Unknown"))


def is_top_5_league(league_id: int) -> bool:
    """Check if a league is in the Top 5."""
    return league_id in TOP_5_LEAGUES


__all__ = [
    "CONFIG_PATH",
    "CURRENT_SEASON",
    "SEASON_FORMAT",
    "TOP_5_LEAGUES",
    "POSITION_GROUPS",
    "get_position_group",
    "is_top_5_league",
]
