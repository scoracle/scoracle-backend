"""
Services module for Scoracle Data.

This module provides business logic services:
- profiles: Sport-aware entity profile lookups (safe SQL via psycopg.sql)
- stats: Sport-aware entity statistics lookups (safe SQL via psycopg.sql)
- bootstrap: Autofill entity database builder for frontend autocomplete
- news: Unified news service (Google News RSS + NewsAPI fallback)
- twitter: Twitter/X journalist feed

For percentile calculation, use PythonPercentileCalculator directly:
    from scoracle_data.percentiles import PythonPercentileCalculator

Usage:
    from scoracle_data.services.profiles import get_player_profile, get_team_profile
    from scoracle_data.services.stats import get_entity_stats, get_available_seasons
    from scoracle_data.services.bootstrap import get_autofill_database
"""

from .profiles import get_player_profile, get_team_profile
from .stats import get_entity_stats, get_available_seasons
from .bootstrap import get_autofill_database
from .news import NewsService, get_news_service
from .twitter import TwitterService, get_twitter_service

__all__ = [
    # Profiles
    "get_player_profile",
    "get_team_profile",
    # Stats
    "get_entity_stats",
    "get_available_seasons",
    # Bootstrap / Autofill
    "get_autofill_database",
    # News
    "NewsService",
    "get_news_service",
    # Twitter
    "TwitterService",
    "get_twitter_service",
]
