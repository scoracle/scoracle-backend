"""News backfill CLI.

Iterates starter players + all teams for a sport and hits the Go API's
GetEntityNews endpoint for each. The API's handler does the heavy lifting
(RSS fetch, entity matching, cross-entity linking, write-through to
news_articles + news_article_entities). Python just drives the loop.

Why the Go API instead of direct RSS in Python?
The fetcher, entity matcher, and write-through logic already live in Go.
Calling the API reuses that code verbatim. A Python reimplementation
would duplicate three non-trivial modules and risk divergence.
"""

from __future__ import annotations

import logging
import sys
import time
from typing import Any

import click
import httpx
import psycopg

from shared import config as config_mod
from shared.db import check_connectivity, create_pool, get_conn

logger = logging.getLogger("news_backfill")


# Starter heuristics — what counts as "worth backfilling news for."
# Post-purge the players table already excludes never-played entries,
# but we narrow further to avoid spamming Google RSS with deep-bench
# players whose coverage would be thin anyway.
_STARTER_SQL = {
    "NBA": """
        SELECT DISTINCT ebs.player_id AS id
        FROM event_box_scores ebs
        JOIN fixtures f ON f.id = ebs.fixture_id
        WHERE ebs.sport = 'NBA' AND f.season = %s
        GROUP BY ebs.player_id
        HAVING count(*) >= 20
           AND AVG(ebs.minutes_played) >= 15
    """,
    "NFL": """
        SELECT DISTINCT ebs.player_id AS id
        FROM event_box_scores ebs
        JOIN fixtures f ON f.id = ebs.fixture_id
        JOIN players p ON p.id = ebs.player_id AND p.sport = 'NFL'
        WHERE ebs.sport = 'NFL' AND f.season = %s
          AND COALESCE(p.position, '') NOT IN ('P', 'LS', 'Punter', 'Long Snapper')
        GROUP BY ebs.player_id
        HAVING count(*) >= 5
    """,
    # Football: "starter" = appeared in 8+ fixtures with 60+ minutes each.
    "FOOTBALL": """
        SELECT DISTINCT ebs.player_id AS id
        FROM event_box_scores ebs
        JOIN fixtures f ON f.id = ebs.fixture_id
        WHERE ebs.sport = 'FOOTBALL' AND f.season = %s
          AND COALESCE(ebs.minutes_played, 0) >= 60
        GROUP BY ebs.player_id
        HAVING count(*) >= 8
    """,
}


@click.group(name="news")
def cli() -> None:
    """News corpus seeding — backfill Google RSS into news_articles."""


def _starter_player_ids(
    conn: psycopg.Connection, sport: str, season: int
) -> list[int]:
    sql = _STARTER_SQL[sport]
    rows = conn.execute(sql, (season,)).fetchall()
    return [r["id"] for r in rows]


def _team_ids(conn: psycopg.Connection, sport: str) -> list[int]:
    rows = conn.execute(
        "SELECT id FROM teams WHERE sport = %s ORDER BY id", (sport,)
    ).fetchall()
    return [r["id"] for r in rows]


def _recently_backfilled(
    conn: psycopg.Connection, entity_type: str, entity_id: int, sport: str, hours: int
) -> bool:
    row = conn.execute(
        """
        SELECT 1 FROM news_article_entities nae
        JOIN news_articles a ON a.id = nae.article_id
        WHERE nae.entity_type = %s
          AND nae.entity_id = %s
          AND nae.sport = %s
          AND a.fetched_at > NOW() - (%s || ' hours')::interval
        LIMIT 1
        """,
        (entity_type, entity_id, sport, hours),
    ).fetchone()
    return row is not None


@cli.command("backfill")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Season year")
@click.option(
    "--api-base",
    default="http://localhost:8000",
    help="Go API base URL",
)
@click.option(
    "--request-delay",
    type=float,
    default=1.0,
    help="Seconds between requests. Defaults to 1s to stay polite with Google News.",
)
@click.option(
    "--skip-if-fresh",
    type=int,
    default=24,
    help="Skip entities that already have news articles linked from the last N hours. 0 to force refetch.",
)
@click.option(
    "--max",
    "max_entities",
    type=int,
    default=None,
    help="Cap total entities (teams + players). Useful for test runs.",
)
@click.option(
    "--teams-only",
    is_flag=True,
    default=False,
    help="Skip the starter-player loop. Team news still gets cross-entity-linked to any players mentioned in the headlines, which is usually enough for NFL and football.",
)
@click.option(
    "--dry-run",
    is_flag=True,
    default=False,
    help="Print what would be fetched without making HTTP calls.",
)
def backfill(
    sport: str,
    season: int,
    api_base: str,
    request_delay: float,
    skip_if_fresh: int,
    max_entities: int | None,
    teams_only: bool,
    dry_run: bool,
) -> None:
    """One-time news backfill for starters + all teams.

    Populates news_articles + news_article_entities with Google RSS
    results for each entity. Safe to re-run: entities already covered
    within --skip-if-fresh hours are skipped by default.

    Rough duration: ~1.5 seconds per entity (handler hits Google RSS up
    to 3 times with throttling), so a full NBA starter + team backfill
    (~200 entities) takes ~5 minutes.
    """
    sport_upper = sport.upper()
    cfg = config_mod.load()
    pool = create_pool(cfg)

    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        with get_conn(pool) as conn:
            teams = _team_ids(conn, sport_upper)
            players: list[int] = []
            if not teams_only:
                players = _starter_player_ids(conn, sport_upper, season)

        total = len(teams) + len(players)
        if max_entities is not None:
            total = min(total, max_entities)

        click.echo(
            f"Backfill plan sport={sport_upper} season={season} "
            f"teams={len(teams)} starter_players={len(players)} "
            f"teams_only={teams_only} total_requests<={total} dry_run={dry_run}"
        )

        if dry_run:
            return

        client = httpx.Client(base_url=api_base, timeout=60.0)
        try:
            stats = _run_backfill(
                pool=pool,
                client=client,
                sport=sport_upper,
                entities=_build_entity_list(teams, players),
                request_delay=request_delay,
                skip_if_fresh=skip_if_fresh,
                max_entities=max_entities,
            )
        finally:
            client.close()

        click.echo(
            f"Backfill complete sport={sport_upper} "
            f"fetched={stats['fetched']} skipped={stats['skipped']} "
            f"failed={stats['failed']}"
        )
    finally:
        pool.close()


def _build_entity_list(
    team_ids: list[int], player_ids: list[int]
) -> list[tuple[str, int]]:
    out: list[tuple[str, int]] = [("team", tid) for tid in team_ids]
    out.extend(("player", pid) for pid in player_ids)
    return out


def _run_backfill(
    *,
    pool: Any,
    client: httpx.Client,
    sport: str,
    entities: list[tuple[str, int]],
    request_delay: float,
    skip_if_fresh: int,
    max_entities: int | None,
) -> dict[str, int]:
    fetched = 0
    skipped = 0
    failed = 0

    for idx, (entity_type, entity_id) in enumerate(entities, start=1):
        if max_entities is not None and (fetched + skipped + failed) >= max_entities:
            break

        if skip_if_fresh > 0:
            with get_conn(pool) as conn:
                if _recently_backfilled(
                    conn, entity_type, entity_id, sport, skip_if_fresh
                ):
                    skipped += 1
                    continue

        url = f"/api/v1/news/{entity_type}/{entity_id}"
        try:
            resp = client.get(url, params={"sport": sport, "limit": 30})
            resp.raise_for_status()
            fetched += 1
        except httpx.HTTPError as exc:
            failed += 1
            logger.warning(
                "news fetch failed entity=%s/%d: %s",
                entity_type, entity_id, exc,
            )

        if idx % 25 == 0:
            click.echo(
                f"  progress {idx}/{len(entities)} "
                f"fetched={fetched} skipped={skipped} failed={failed}"
            )

        time.sleep(request_delay)

    return {"fetched": fetched, "skipped": skipped, "failed": failed}


if __name__ == "__main__":
    cli()
