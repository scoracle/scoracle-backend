"""Configuration loaded from environment variables."""

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    database_url: str
    bdl_api_key: str
    sportmonks_api_token: str
    api_sports_key: str
    db_pool_min: int = 2
    db_pool_max: int = 10


def load() -> Config:
    """Load configuration from environment variables.

    DB URL resolution chain: DATABASE_PRIVATE_URL > DATABASE_URL
    """
    db_url = (
        os.environ.get("DATABASE_PRIVATE_URL")
        or os.environ.get("DATABASE_URL")
        or ""
    )
    if not db_url:
        raise SystemExit("DATABASE_PRIVATE_URL or DATABASE_URL must be set")

    return Config(
        database_url=db_url,
        bdl_api_key=os.environ.get("BALLDONTLIE_API_KEY", ""),
        sportmonks_api_token=os.environ.get("SPORTMONKS_API_TOKEN", ""),
        api_sports_key=os.environ.get("API_SPORTS_KEY", ""),
        db_pool_min=int(os.environ.get("DB_POOL_MIN_CONNS", "2")),
        db_pool_max=int(os.environ.get("DB_POOL_MAX_CONNS", "10")),
    )
