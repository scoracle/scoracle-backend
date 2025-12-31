"""
Pytest configuration for scoracle-data tests.
"""

import os
import pytest


def pytest_configure(config):
    """Configure pytest with database URL if available."""
    # Try to load from .env file if environment variables not already set
    env_file = os.path.join(os.path.dirname(__file__), "..", ".env")
    if os.path.exists(env_file):
        with open(env_file) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    key, value = line.split("=", 1)
                    key = key.strip()
                    value = value.strip().strip('"').strip("'")
                    if key not in os.environ:
                        os.environ[key] = value


@pytest.fixture(scope="session")
def neon_url():
    """Get the Neon database URL."""
    url = os.environ.get("DATABASE_URL") or os.environ.get("NEON_DATABASE_URL")
    if not url:
        pytest.skip("DATABASE_URL not set")
    return url
