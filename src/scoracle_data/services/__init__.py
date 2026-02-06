"""
Services module for Scoracle Data.

This module provides business logic services:
- profiles: Sport-aware entity profile lookups (safe SQL via psycopg.sql)
- stats: Sport-aware entity statistics lookups (safe SQL via psycopg.sql)
- news: Unified news service (Google News RSS + NewsAPI fallback)
- twitter: Twitter/X journalist feed
- percentiles: Per-36/Per-90 stat calculations

Usage:
    from scoracle_data.services.profiles import get_player_profile, get_team_profile
    from scoracle_data.services.stats import get_entity_stats, get_available_seasons
    from scoracle_data.services.news import NewsService, get_news_service
    from scoracle_data.services.twitter import TwitterService, get_twitter_service
    from scoracle_data.services.percentiles import PercentileService, get_percentile_service
"""

from .profiles import get_player_profile, get_team_profile
from .stats import get_entity_stats, get_available_seasons
from .news import NewsService, get_news_service
from .twitter import TwitterService, get_twitter_service
from .percentiles import PercentileService, get_percentile_service

__all__ = [
    # Profiles
    "get_player_profile",
    "get_team_profile",
    # Stats
    "get_entity_stats",
    "get_available_seasons",
    # News
    "NewsService",
    "get_news_service",
    # Twitter
    "TwitterService",
    "get_twitter_service",
    # Percentiles
    "PercentileService",
    "get_percentile_service",
]
