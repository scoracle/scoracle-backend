"""
Data provider abstraction layer.

This module provides a clean interface for fetching sports data
from various providers (API-Sports, SportRadar, etc.) in a 
provider-agnostic way.

Usage:
    from scoracle_data.providers import get_provider, DataProviderProtocol
    
    # Get the default provider (API-Sports)
    provider = get_provider()
    
    # Fetch teams
    teams = await provider.fetch_teams("NBA", season=2024)
    
    # Each team has canonical_data (mapped fields) and raw_response (full API response)
    for team in teams:
        print(team.canonical_data["name"])
        print(team.raw_response)  # Full API response for JSONB storage
"""

from .base import (
    DataProviderProtocol,
    RawEntityData,
    ProviderError,
    RateLimitError,
)

__all__ = [
    "DataProviderProtocol",
    "RawEntityData", 
    "ProviderError",
    "RateLimitError",
]


def get_provider(
    provider_name: str = "api_sports",
    api_key: str | None = None,
) -> DataProviderProtocol:
    """
    Get a data provider instance.
    
    Args:
        provider_name: Provider to use ("api_sports", "sportradar", etc.)
        api_key: Optional API key (uses env var if not provided)
        
    Returns:
        DataProviderProtocol implementation
        
    Raises:
        ValueError: If provider not found
    """
    if provider_name == "api_sports":
        from .api_sports import ApiSportsProvider
        return ApiSportsProvider(api_key=api_key)
    else:
        raise ValueError(f"Unknown provider: {provider_name}")
