"""
Twitter/X Service for Scoracle Data.

Provides access to curated journalist feeds for sports intel.

Usage:
    from scoracle_data.services.twitter import TwitterService, get_twitter_service
    
    service = get_twitter_service()
    result = await service.get_journalist_feed("LeBron James")
"""

from .service import TwitterService, get_twitter_service

__all__ = [
    "TwitterService",
    "get_twitter_service",
]
