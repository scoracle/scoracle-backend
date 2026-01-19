"""
Sport configuration management.

Loads sport-specific configurations from YAML files, providing
a centralized place for field mappings, provider configurations,
and sport-specific settings.

Usage:
    from scoracle_data.sport_configs import get_config, SportConfig
    
    config = get_config()
    nba_config = config.get_sport("NBA")
    mapping = nba_config.get_provider_mapping("api_sports", "player_discovery")
"""

from __future__ import annotations

import logging
from functools import lru_cache
from pathlib import Path
from typing import Any

import yaml

logger = logging.getLogger(__name__)

# Directory containing YAML config files
CONFIG_DIR = Path(__file__).parent


class SportConfig:
    """Configuration for a single sport."""
    
    def __init__(self, sport_id: str, data: dict[str, Any]):
        self.sport_id = sport_id
        self._data = data
    
    @property
    def display_name(self) -> str:
        return self._data.get("display_name", self.sport_id)
    
    @property
    def season_format(self) -> str:
        return self._data.get("season_format", "YYYY")
    
    def get_provider_mapping(
        self, 
        provider: str, 
        mapping_type: str
    ) -> dict[str, Any]:
        """
        Get field mapping for a specific provider and mapping type.
        
        Args:
            provider: Provider name (e.g., "api_sports")
            mapping_type: Type of mapping (e.g., "player_discovery", "player_profile")
            
        Returns:
            Dict mapping canonical field names to provider field paths
        """
        providers = self._data.get("providers", {})
        provider_config = providers.get(provider, {})
        return provider_config.get(mapping_type, {})
    
    def get_provider_config(self, provider: str) -> dict[str, Any]:
        """Get full provider configuration."""
        return self._data.get("providers", {}).get(provider, {})
    
    def get_position_groups(self) -> dict[str, str]:
        """Get position to position group mapping."""
        return self._data.get("position_groups", {})
    
    def get_columns(self, table_type: str) -> list[str]:
        """
        Get column list for a table type.
        
        Args:
            table_type: One of "player_profile", "team_profile", 
                       "player_stats", "team_stats"
        """
        return self._data.get("columns", {}).get(table_type, [])
    
    def get_stat_aggregations(self) -> dict[str, str]:
        """Get stat aggregation rules (sum, avg, max, etc.)."""
        return self._data.get("stat_aggregations", {})
    
    def get_filters(self) -> dict[str, Any]:
        """Get entity filters (e.g., nbaFranchise=True)."""
        return self._data.get("filters", {})
    
    def __getitem__(self, key: str) -> Any:
        """Allow dict-like access to raw config."""
        return self._data.get(key)
    
    def get(self, key: str, default: Any = None) -> Any:
        """Get config value with default."""
        return self._data.get(key, default)


class ConfigLoader:
    """Loads and caches sport configurations from YAML files."""
    
    def __init__(self, config_dir: Path | None = None):
        self.config_dir = config_dir or CONFIG_DIR
        self._configs: dict[str, SportConfig] = {}
        self._loaded = False
    
    def _load_all(self) -> None:
        """Load all YAML config files."""
        if self._loaded:
            return
        
        for yaml_file in self.config_dir.glob("*.yaml"):
            sport_id = yaml_file.stem.upper()
            try:
                with open(yaml_file, "r") as f:
                    data = yaml.safe_load(f)
                    if data:
                        # Use sport_id from file if specified
                        actual_id = data.get("sport_id", sport_id)
                        self._configs[actual_id.upper()] = SportConfig(actual_id, data)
                        logger.debug(f"Loaded config for {actual_id}")
            except Exception as e:
                logger.warning(f"Failed to load {yaml_file}: {e}")
        
        self._loaded = True
    
    def get_sport(self, sport_id: str) -> SportConfig:
        """
        Get configuration for a sport.
        
        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
            
        Returns:
            SportConfig instance
            
        Raises:
            KeyError: If sport config not found
        """
        self._load_all()
        sport_upper = sport_id.upper()
        
        if sport_upper not in self._configs:
            raise KeyError(f"No configuration found for sport: {sport_id}")
        
        return self._configs[sport_upper]
    
    def list_sports(self) -> list[str]:
        """List all configured sports."""
        self._load_all()
        return list(self._configs.keys())
    
    def get_player_columns(self, sport_id: str) -> list[str]:
        """Get player profile columns for a sport."""
        return self.get_sport(sport_id).get_columns("player_profile")
    
    def get_team_columns(self, sport_id: str) -> list[str]:
        """Get team profile columns for a sport."""
        return self.get_sport(sport_id).get_columns("team_profile")
    
    def get_player_stats_columns(self, sport_id: str) -> list[str]:
        """Get player stats columns for a sport."""
        return self.get_sport(sport_id).get_columns("player_stats")
    
    def get_team_stats_columns(self, sport_id: str) -> list[str]:
        """Get team stats columns for a sport."""
        return self.get_sport(sport_id).get_columns("team_stats")


@lru_cache
def get_config() -> ConfigLoader:
    """Get cached config loader instance."""
    return ConfigLoader()


# Convenience function
def get_sport_config(sport_id: str) -> SportConfig:
    """Get configuration for a specific sport."""
    return get_config().get_sport(sport_id)
