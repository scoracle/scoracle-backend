"""
Unified News Service for Scoracle Data.

Combines Google News RSS (free) and NewsAPI (paid) into a single interface.
Primary source is RSS; NewsAPI is used as fallback or enhancement.

Usage:
    from scoracle_data.services.news import NewsService, get_news_service
    
    service = get_news_service()
    result = await service.get_entity_news("LeBron James", sport="NBA")
"""

from .service import NewsService, get_news_service

__all__ = [
    "NewsService",
    "get_news_service",
]
