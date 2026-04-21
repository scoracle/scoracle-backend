"""Main CLI entry point — routes commands to appropriate services.

Usage:
  scoracle-seed event [command]    # Box scores, fixtures
  scoracle-seed meta [command]     # Profiles, metadata, images, purge
"""

from __future__ import annotations

import logging

import click

logger = logging.getLogger("scoracle_seed")


def _setup_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


@click.group()
def cli() -> None:
    """Scoracle Seed — sports data ingestion.

    Services:
      event  — Box scores and fixture data
      meta   — Player/team profiles, images, purge-inactive
    """
    _setup_logging()


from services.event.cli import cli as event_cli
from services.meta.cli import cli as meta_cli

cli.add_command(event_cli, name="event")
cli.add_command(meta_cli, name="meta")


if __name__ == "__main__":
    cli()
