#!/usr/bin/env python3
"""Daemon entry point for metadata worker.

This script runs the metadata worker as a background daemon,
continuously processing metadata refresh requests from the queue.

Usage:
    python -m scripts.run_metadata_worker --daemon
    python -m scripts.run_metadata_worker --once
    python -m scripts.run_metadata_worker --sport FOOTBALL
"""

import argparse
import logging
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from seed.scoracle_seed.metadata_worker import run_worker


def main():
    parser = argparse.ArgumentParser(
        description="Metadata refresh worker daemon",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Run as continuous daemon
  python run_metadata_worker.py --daemon
  
  # Run once and exit
  python run_metadata_worker.py --once
  
  # Process only Football
  python run_metadata_worker.py --daemon --sport FOOTBALL
  
  # Process with verbose logging
  python run_metadata_worker.py --daemon -v
        """,
    )

    parser.add_argument(
        "--daemon",
        action="store_true",
        help="Run as continuous daemon (default: run once and exit)",
    )

    parser.add_argument(
        "--once",
        action="store_true",
        help="Process queue once and exit (default if --daemon not specified)",
    )

    parser.add_argument(
        "--sport", choices=["NBA", "NFL", "FOOTBALL"], help="Filter by sport"
    )

    parser.add_argument(
        "--poll-interval",
        type=float,
        default=5.0,
        help="Seconds to wait between queue checks when empty (default: 5.0)",
    )

    parser.add_argument(
        "--max",
        type=int,
        default=None,
        help="Maximum items to process (non-daemon mode only)",
    )

    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Enable verbose logging"
    )

    args = parser.parse_args()

    # Configure logging
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        handlers=[
            logging.StreamHandler(sys.stdout),
        ],
    )

    # Determine mode
    daemon_mode = args.daemon or not args.once

    try:
        run_worker(
            daemon=daemon_mode,
            sport=args.sport,
            poll_interval=args.poll_interval,
        )
    except KeyboardInterrupt:
        print("\nShutdown requested")
        sys.exit(0)
    except Exception as e:
        logging.error(f"Worker failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
