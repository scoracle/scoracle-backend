#!/usr/bin/env python3
"""
Master database seeding script.

This script:
1. Seeds teams and players from the small_dataset.json fixture
2. Seeds sample statistics for those entities
3. Provides a complete test database for development

Usage:
    python scripts/seed_database.py

Environment Variables:
    DATABASE_URL or NEON_DATABASE_URL: PostgreSQL connection string
"""

import logging
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from scoracle_data.seeders.small_dataset_seeder import seed_small_dataset
from scoracle_data.seeders.stats_seeder import seed_all_stats

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)


def main():
    """Run the complete database seeding process."""
    logger.info("=" * 70)
    logger.info("Starting database seeding...")
    logger.info("=" * 70)

    # Load .env if present
    try:
        from dotenv import load_dotenv
        repo_root = Path(__file__).resolve().parents[1]
        env_path = repo_root / ".env"
        if env_path.exists():
            load_dotenv(env_path)
            logger.info("Loaded environment variables from .env")
    except Exception as e:
        logger.warning("Could not load .env: %s", e)

    # Step 1: Seed entities (teams and players)
    logger.info("")
    logger.info("Step 1: Seeding teams and players...")
    logger.info("-" * 70)
    try:
        entity_result = seed_small_dataset()
        summary = entity_result.get("summary", {})
        logger.info("✓ Teams seeded: %d", summary.get("teams", 0))
        logger.info("✓ Players seeded: %d", summary.get("players", 0))
    except Exception as e:
        logger.error("✗ Failed to seed entities: %s", e)
        sys.exit(1)

    # Step 2: Seed statistics
    logger.info("")
    logger.info("Step 2: Seeding player statistics...")
    logger.info("-" * 70)
    try:
        stats_result = seed_all_stats()
        for sport, count in stats_result.items():
            if count > 0:
                logger.info("✓ %s: %d records", sport.replace("_", " ").title(), count)
    except Exception as e:
        logger.error("✗ Failed to seed statistics: %s", e)
        sys.exit(1)

    # Summary
    logger.info("")
    logger.info("=" * 70)
    logger.info("Database seeding complete!")
    logger.info("=" * 70)
    logger.info("Summary:")
    logger.info("  - Teams: %d", summary.get("teams", 0))
    logger.info("  - Players: %d", summary.get("players", 0))
    total_stats = sum(stats_result.values())
    logger.info("  - Statistics records: %d", total_stats)
    logger.info("")
    logger.info("Your database is now ready for testing and development.")
    logger.info("=" * 70)


if __name__ == "__main__":
    main()
