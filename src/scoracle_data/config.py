"""
Configuration - re-exports from core.config for backwards compatibility.

New code should import directly from scoracle_data.core.config.
"""

# Re-export everything from core.config
from .core.config import Settings, get_settings, settings

__all__ = [
    "Settings",
    "get_settings",
    "settings",
]
