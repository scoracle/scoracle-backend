"""
Data provider clients for external sports APIs.

Each sport has a dedicated provider client:
- NBA: BallDontLie API (BallDontLieNBA)
- NFL: BallDontLie API (BallDontLieNFL)
- Football: SportMonks API (SportMonksClient)

All clients inherit from BaseApiClient which provides shared
rate limiting, retry logic, and async context management.

Usage:
    from scoracle_data.providers import BallDontLieNBA

    async with BallDontLieNBA(api_key="...") as client:
        teams = await client.get_teams()
        async for player in client.get_players():
            print(player["first_name"])
"""

from ..core.http import BaseApiClient, RateLimiter
from .balldontlie_nba import BallDontLieNBA
from .balldontlie_nfl import BallDontLieNFL
from .sportmonks import SportMonksClient

__all__ = [
    "BaseApiClient",
    "RateLimiter",
    "BallDontLieNBA",
    "BallDontLieNFL",
    "SportMonksClient",
]
