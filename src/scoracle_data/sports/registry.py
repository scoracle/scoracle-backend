"""
Sport Registry - manages sport configurations and provider adapters.

Loads sport-specific settings from TOML files and provides
a unified interface for accessing providers across sports.
"""

from __future__ import annotations

import logging
import tomllib
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path
from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from ..providers import ApiClientProtocol

logger = logging.getLogger(__name__)

# Directory containing this module (sports/)
SPORTS_DIR = Path(__file__).parent


@dataclass
class ProviderConfig:
    """Configuration for a data provider."""
    name: str
    module: str  # e.g., "scoracle_data.providers.api_sports"
    class_name: str  # e.g., "ApiSportsNBA"
    priority: int = 0  # Higher = preferred
    enabled: bool = True
    settings: dict[str, Any] = field(default_factory=dict)


@dataclass
class SportConfig:
    """Configuration for a sport."""
    id: str
    display_name: str
    current_season: str
    season_format: str
    providers: list[ProviderConfig] = field(default_factory=list)
    position_groups: dict[str, str] = field(default_factory=dict)
    percentile_settings: dict[str, Any] = field(default_factory=dict)
    
    @property
    def primary_provider(self) -> ProviderConfig | None:
        """Get the highest priority enabled provider."""
        enabled = [p for p in self.providers if p.enabled]
        if not enabled:
            return None
        return max(enabled, key=lambda p: p.priority)


class Sport:
    """
    Runtime representation of a sport with provider access.
    
    Provides a clean interface for accessing sport-specific
    data providers and configuration.
    """
    
    def __init__(self, config: SportConfig):
        self._config = config
        self._provider_cache: dict[str, Any] = {}
    
    @property
    def id(self) -> str:
        return self._config.id
    
    @property
    def display_name(self) -> str:
        return self._config.display_name
    
    @property
    def current_season(self) -> str:
        return self._config.current_season
    
    @property
    def season_format(self) -> str:
        return self._config.season_format
    
    @property
    def position_groups(self) -> dict[str, str]:
        return self._config.position_groups
    
    def get_provider(self, name: str | None = None) -> "ApiClientProtocol":
        """
        Get a data provider instance.
        
        Args:
            name: Provider name, or None for primary provider
        
        Returns:
            Provider instance implementing ApiClientProtocol
        
        Raises:
            ValueError: If provider not found or not configured
        """
        if name is None:
            provider_config = self._config.primary_provider
            if provider_config is None:
                raise ValueError(f"No providers configured for {self.id}")
        else:
            provider_config = next(
                (p for p in self._config.providers if p.name == name),
                None
            )
            if provider_config is None:
                raise ValueError(f"Provider '{name}' not found for {self.id}")
        
        # Return cached instance if available
        cache_key = provider_config.name
        if cache_key in self._provider_cache:
            return self._provider_cache[cache_key]
        
        # Dynamically import and instantiate provider
        provider = self._instantiate_provider(provider_config)
        self._provider_cache[cache_key] = provider
        return provider
    
    def _instantiate_provider(self, config: ProviderConfig) -> Any:
        """Dynamically instantiate a provider from config."""
        import importlib
        
        try:
            module = importlib.import_module(config.module)
            cls = getattr(module, config.class_name)
            return cls(**config.settings)
        except (ImportError, AttributeError) as e:
            logger.error(f"Failed to instantiate provider {config.name}: {e}")
            raise ValueError(f"Could not load provider {config.name}: {e}")
    
    def list_providers(self) -> list[str]:
        """List available provider names."""
        return [p.name for p in self._config.providers if p.enabled]


class SportRegistry:
    """
    Registry of all sports and their configurations.
    
    Loads configurations from TOML files in sport subdirectories.
    """
    
    def __init__(self, sports_dir: Path | None = None):
        self._sports_dir = sports_dir or SPORTS_DIR
        self._sports: dict[str, Sport] = {}
        self._loaded = False
    
    def _load_all(self) -> None:
        """Load all sport configurations."""
        if self._loaded:
            return
        
        # Look for config.toml in each sport subdirectory
        for sport_dir in self._sports_dir.iterdir():
            if not sport_dir.is_dir():
                continue
            
            config_file = sport_dir / "config.toml"
            if not config_file.exists():
                continue
            
            try:
                config = self._load_config(config_file)
                sport = Sport(config)
                self._sports[sport.id.upper()] = sport
                logger.debug(f"Loaded sport config: {sport.id}")
            except Exception as e:
                logger.warning(f"Failed to load {config_file}: {e}")
        
        self._loaded = True
    
    def _load_config(self, config_file: Path) -> SportConfig:
        """Load a single sport configuration from TOML."""
        with open(config_file, "rb") as f:
            data = tomllib.load(f)
        
        # Parse providers
        providers = []
        for prov_data in data.get("providers", []):
            providers.append(ProviderConfig(
                name=prov_data["name"],
                module=prov_data["module"],
                class_name=prov_data["class"],
                priority=prov_data.get("priority", 0),
                enabled=prov_data.get("enabled", True),
                settings=prov_data.get("settings", {}),
            ))
        
        return SportConfig(
            id=data["id"],
            display_name=data.get("display_name", data["id"]),
            current_season=data.get("current_season", ""),
            season_format=data.get("season_format", "YYYY"),
            providers=providers,
            position_groups=data.get("position_groups", {}),
            percentile_settings=data.get("percentile_settings", {}),
        )
    
    def get(self, sport_id: str) -> Sport:
        """
        Get a sport by ID.
        
        Args:
            sport_id: Sport identifier (NBA, NFL, FOOTBALL)
        
        Returns:
            Sport instance
        
        Raises:
            KeyError: If sport not found
        """
        self._load_all()
        sport_upper = sport_id.upper()
        
        if sport_upper not in self._sports:
            raise KeyError(f"Sport not found: {sport_id}")
        
        return self._sports[sport_upper]
    
    def list(self) -> list[str]:
        """List all registered sport IDs."""
        self._load_all()
        return list(self._sports.keys())
    
    def __contains__(self, sport_id: str) -> bool:
        """Check if a sport is registered."""
        self._load_all()
        return sport_id.upper() in self._sports


# Singleton registry
_registry: SportRegistry | None = None


def _get_registry() -> SportRegistry:
    """Get the singleton registry."""
    global _registry
    if _registry is None:
        _registry = SportRegistry()
    return _registry


def get_sport(sport_id: str) -> Sport:
    """Get a sport by ID."""
    return _get_registry().get(sport_id)


def list_sports() -> list[str]:
    """List all registered sport IDs."""
    return _get_registry().list()
