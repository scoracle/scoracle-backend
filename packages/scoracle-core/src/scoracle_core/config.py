"""Configuration loading from environment variables."""

import os
import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class Config:
    """Core configuration from environment."""
    
    database_url: str
    log_level: str = "INFO"
    
    @classmethod
    def from_env(cls) -> "Config":
        """Load configuration from environment variables."""
        database_url = os.environ.get("DATABASE_URL")
        if not database_url:
            raise ValueError("DATABASE_URL environment variable is required")
        
        return cls(
            database_url=database_url,
            log_level=os.environ.get("LOG_LEVEL", "INFO"),
        )
    
    def setup_logging(self) -> None:
        """Configure logging based on config."""
        logging.basicConfig(
            level=getattr(logging, self.log_level.upper()),
            format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
            datefmt="%Y-%m-%d %H:%M:%S",
        )


@dataclass
class BallDontLieConfig:
    """BallDontLie API configuration."""
    
    api_key: str
    requests_per_minute: int = 600  # GOAT tier
    
    @classmethod
    def from_env(cls) -> "BallDontLieConfig":
        """Load from environment."""
        api_key = os.environ.get("BALLDONTLIE_API_KEY")
        if not api_key:
            raise ValueError("BALLDONTLIE_API_KEY environment variable is required")
        
        return cls(api_key=api_key)


@dataclass
class SportMonksConfig:
    """SportMonks API configuration."""
    
    api_token: str
    requests_per_minute: int = 2999  # Advanced plan hourly limit / 60
    
    @classmethod
    def from_env(cls) -> "SportMonksConfig":
        """Load from environment."""
        api_token = os.environ.get("SPORTMONKS_API_TOKEN")
        if not api_token:
            raise ValueError("SPORTMONKS_API_TOKEN environment variable is required")
        
        return cls(api_token=api_token)
