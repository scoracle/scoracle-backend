"""
Sports module for Scoracle Data.

Provides sport-specific configurations and provider adapters.
Each sport can have its own data provider(s) configured via TOML.

Usage:
    from scoracle_data.sports import get_sport, list_sports, SportRegistry
    
    nba = get_sport("NBA")
    provider = nba.get_provider()
    
    # Or access sport-specific modules directly
    from scoracle_data.sports.nba import NBAProvider, NBA_CONFIG
"""

from .registry import SportRegistry, get_sport, list_sports

__all__ = [
    "SportRegistry",
    "get_sport",
    "list_sports",
]
