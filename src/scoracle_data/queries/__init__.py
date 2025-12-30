"""
Query utilities for stats database.
"""

from .players import PlayerQueries
from .teams import TeamQueries

__all__ = [
    "PlayerQueries",
    "TeamQueries",
]
