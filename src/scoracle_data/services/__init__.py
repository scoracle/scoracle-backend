"""
Services module for Scoracle Data.

This module provides business logic services:
- news: Unified news service (Google News RSS + NewsAPI fallback)
- twitter: Twitter/X journalist feed
- percentiles: Per-36/Per-90 stat calculations

Usage:
    from scoracle_data.services.news import NewsService, get_news_service
    from scoracle_data.services.twitter import TwitterService, get_twitter_service
    from scoracle_data.services.percentiles import PercentileService, get_percentile_service
"""

from .news import NewsService, get_news_service
from .twitter import TwitterService, get_twitter_service
from .percentiles import PercentileService, get_percentile_service

__all__ = [
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
