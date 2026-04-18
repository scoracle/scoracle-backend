"""Event seeding CLI commands.

Commands:
  load-fixtures    — Load fixture schedule into Postgres
  process          — Seed event-level box scores for pending fixtures
"""

from __future__ import annotations

import sys
from typing import Any

import click
import psycopg

from shared import config as config_mod
from shared.db import check_connectivity, create_pool, get_conn
from shared.upsert import (
    finalize_fixture,
    upsert_event_box_score,
    upsert_event_team_stats,
    upsert_player,
    upsert_provider_entity_map,
    upsert_provider_fixture_map,
    upsert_team,
)
from .fixtures import (
    FixtureRow,
    get_pending,
    get_provider_fixture_id,
    record_failure,
    upsert_fixture,
)

_PROVIDER_BY_SPORT = {"NBA": "bdl", "NFL": "bdl", "FOOTBALL": "sportmonks"}


@click.group(name="event")
def cli() -> None:
    """Event seeding — fixtures and box scores."""


def _resolve_external_fixture_id(
    conn: psycopg.Connection, fixture: FixtureRow, provider: str
) -> int:
    provider_fixture_id = get_provider_fixture_id(conn, fixture.id, provider, fixture.sport)
    raw_id: Any = provider_fixture_id if provider_fixture_id is not None else fixture.external_id
    if raw_id is None:
        raise RuntimeError(
            f"fixture {fixture.id} has no provider fixture mapping and no external_id"
        )
    try:
        return int(raw_id)
    except (TypeError, ValueError) as exc:
        raise RuntimeError(
            f"fixture {fixture.id} provider fixture id is not an integer: {raw_id!r}"
        ) from exc


def _seed_fixture_box_scores(
    conn: psycopg.Connection, fixture: FixtureRow, handler: Any
) -> tuple[int, int, int, int]:
    provider = _PROVIDER_BY_SPORT[fixture.sport]
    external_fixture_id = _resolve_external_fixture_id(conn, fixture, provider)
    player_rows, team_rows = handler.get_box_score(external_fixture_id, fixture.id)

    if not player_rows and not team_rows:
        raise RuntimeError(
            f"provider returned no event rows for fixture_id={fixture.id} external_id={external_fixture_id}"
        )

    season = fixture.season
    league_id = fixture.league_id or 0

    # Clear stale rows so re-seeds don't leave orphaned data
    conn.execute(
        "DELETE FROM event_box_scores WHERE fixture_id = %s", (fixture.id,)
    )
    conn.execute(
        "DELETE FROM event_team_stats WHERE fixture_id = %s", (fixture.id,)
    )

    for row in player_rows:
        if row.player:
            upsert_player(conn, fixture.sport, row.player)
            upsert_provider_entity_map(
                conn,
                provider,
                fixture.sport,
                "player",
                str(row.player_id),
                row.player_id,
            )
        upsert_event_box_score(conn, fixture.sport, season, league_id, row)

    for row in team_rows:
        if row.team:
            upsert_team(conn, fixture.sport, row.team)
            upsert_provider_entity_map(
                conn,
                provider,
                fixture.sport,
                "team",
                str(row.team_id),
                row.team_id,
            )
        upsert_event_team_stats(conn, fixture.sport, season, league_id, row)

    players_updated, teams_updated = finalize_fixture(conn, fixture.id)
    return len(player_rows), len(team_rows), players_updated, teams_updated


@cli.command("load-fixtures")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option("--from-date", type=str, default=None, help="Start date YYYY-MM-DD")
@click.option("--to-date", type=str, default=None, help="End date YYYY-MM-DD")
def load_fixtures(
    sport: str,
    season: int,
    league: int,
    from_date: str | None,
    to_date: str | None,
) -> None:
    """Load fixture schedule from provider APIs into fixtures/provider maps."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        sport_upper = sport.upper()
        loaded = 0
        skipped = 0

        with get_conn(pool) as conn:
            from .handlers.bdl_nba import NBAHandler
            from .handlers.bdl_nfl import NFLHandler
            from .handlers.sportmonks_football import FootballHandler

            if sport_upper == "NBA":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NBA seeding", err=True)
                    sys.exit(1)

                handler = NBAHandler(cfg.bdl_api_key)
                try:
                    games = handler.get_games(
                        season, from_date=from_date, to_date=to_date
                    )
                    for game in games:
                        external_id = game.get("external_id")
                        home_team_id = game.get("home_team_id")
                        away_team_id = game.get("away_team_id")
                        start_time = game.get("start_time")
                        if not isinstance(external_id, int):
                            skipped += 1
                            continue
                        if not isinstance(home_team_id, int) or not isinstance(
                            away_team_id, int
                        ):
                            skipped += 1
                            continue
                        if not isinstance(start_time, str):
                            skipped += 1
                            continue

                        home_team = game.get("home_team")
                        away_team = game.get("away_team")
                        if home_team:
                            upsert_team(conn, "NBA", home_team)
                            upsert_provider_entity_map(
                                conn, "bdl", "NBA", "team", str(home_team.id), home_team.id
                            )
                        if away_team:
                            upsert_team(conn, "NBA", away_team)
                            upsert_provider_entity_map(
                                conn, "bdl", "NBA", "team", str(away_team.id), away_team.id
                            )

                        fixture_season = (
                            game["season"] if isinstance(game.get("season"), int) else season
                        )
                        fixture_id = upsert_fixture(
                            conn,
                            external_id=external_id,
                            sport="NBA",
                            league_id=0,
                            season=fixture_season,
                            home_team_id=home_team_id,
                            away_team_id=away_team_id,
                            start_time=start_time,
                            round_name=(
                                str(game["round"]) if game.get("round") is not None else None
                            ),
                            seed_delay_hours=0,
                        )
                        upsert_provider_fixture_map(
                            conn, "bdl", "NBA", str(external_id), fixture_id
                        )
                        loaded += 1
                finally:
                    handler.close()

                click.echo(
                    f"Loaded {loaded} NBA fixtures for season {season} (skipped={skipped})"
                )

            elif sport_upper == "NFL":
                if not cfg.bdl_api_key:
                    click.echo("BALLDONTLIE_API_KEY is required for NFL seeding", err=True)
                    sys.exit(1)

                handler = NFLHandler(cfg.bdl_api_key)
                try:
                    games = handler.get_games(
                        season, from_date=from_date, to_date=to_date
                    )
                    for game in games:
                        external_id = game.get("external_id")
                        home_team_id = game.get("home_team_id")
                        away_team_id = game.get("away_team_id")
                        start_time = game.get("start_time")
                        if not isinstance(external_id, int):
                            skipped += 1
                            continue
                        if not isinstance(home_team_id, int) or not isinstance(
                            away_team_id, int
                        ):
                            skipped += 1
                            continue
                        if not isinstance(start_time, str):
                            skipped += 1
                            continue

                        home_team = game.get("home_team")
                        away_team = game.get("away_team")
                        if home_team:
                            upsert_team(conn, "NFL", home_team)
                            upsert_provider_entity_map(
                                conn, "bdl", "NFL", "team", str(home_team.id), home_team.id
                            )
                        if away_team:
                            upsert_team(conn, "NFL", away_team)
                            upsert_provider_entity_map(
                                conn, "bdl", "NFL", "team", str(away_team.id), away_team.id
                            )

                        fixture_season = (
                            game["season"] if isinstance(game.get("season"), int) else season
                        )
                        fixture_id = upsert_fixture(
                            conn,
                            external_id=external_id,
                            sport="NFL",
                            league_id=0,
                            season=fixture_season,
                            home_team_id=home_team_id,
                            away_team_id=away_team_id,
                            start_time=start_time,
                            round_name=(
                                str(game["round"]) if game.get("round") is not None else None
                            ),
                            seed_delay_hours=0,
                        )
                        upsert_provider_fixture_map(
                            conn, "bdl", "NFL", str(external_id), fixture_id
                        )
                        loaded += 1
                finally:
                    handler.close()

                click.echo(
                    f"Loaded {loaded} NFL fixtures for season {season} (skipped={skipped})"
                )

            elif sport_upper == "FOOTBALL":
                if not cfg.sportmonks_api_token:
                    click.echo(
                        "SPORTMONKS_API_TOKEN is required for football seeding", err=True
                    )
                    sys.exit(1)

                from shared.db import (
                    get_football_league_ids,
                    resolve_provider_season_id,
                )

                if league:
                    league_ids = [league]
                else:
                    league_ids = get_football_league_ids(conn, season)
                    if not league_ids:
                        click.echo(
                            f"No provider_seasons rows found for football season={season}. "
                            "Add them or pass --league explicitly.",
                            err=True,
                        )
                        sys.exit(1)
                    click.echo(
                        f"Iterating {len(league_ids)} football leagues: {league_ids}"
                    )

                handler = FootballHandler(cfg.sportmonks_api_token)
                try:
                    per_league_loaded: dict[int, int] = {}
                    per_league_skipped: dict[int, int] = {}

                    for current_league in league_ids:
                        click.echo(f"--- league={current_league} ---")
                        loaded_this = 0
                        skipped_this = 0

                        sm_season_id = resolve_provider_season_id(
                            conn, current_league, season
                        )
                        if not sm_season_id:
                            click.echo(
                                f"No SportMonks season mapping for league={current_league} "
                                f"season={season}; skipping",
                                err=True,
                            )
                            continue

                        fixtures = handler.get_fixtures(sm_season_id)
                        for fixture in fixtures:
                            external_id = fixture.get("external_id")
                            home_team_id = fixture.get("home_team_id")
                            away_team_id = fixture.get("away_team_id")
                            start_time = fixture.get("start_time")
                            if not isinstance(external_id, int):
                                skipped_this += 1
                                continue
                            if not isinstance(home_team_id, int) or not isinstance(
                                away_team_id, int
                            ):
                                skipped_this += 1
                                continue
                            if not isinstance(start_time, str):
                                skipped_this += 1
                                continue

                            home_team = fixture.get("home_team")
                            away_team = fixture.get("away_team")
                            if home_team:
                                upsert_team(conn, "FOOTBALL", home_team)
                                upsert_provider_entity_map(
                                    conn,
                                    "sportmonks",
                                    "FOOTBALL",
                                    "team",
                                    str(home_team.id),
                                    home_team.id,
                                )
                            if away_team:
                                upsert_team(conn, "FOOTBALL", away_team)
                                upsert_provider_entity_map(
                                    conn,
                                    "sportmonks",
                                    "FOOTBALL",
                                    "team",
                                    str(away_team.id),
                                    away_team.id,
                                )

                            fixture_season = (
                                fixture["season"]
                                if isinstance(fixture.get("season"), int)
                                else season
                            )
                            fixture_id = upsert_fixture(
                                conn,
                                external_id=external_id,
                                sport="FOOTBALL",
                                league_id=current_league,
                                season=fixture_season,
                                home_team_id=home_team_id,
                                away_team_id=away_team_id,
                                start_time=start_time,
                                round_name=(
                                    str(fixture["round"])
                                    if fixture.get("round") is not None
                                    else None
                                ),
                                seed_delay_hours=0,
                            )
                            upsert_provider_fixture_map(
                                conn, "sportmonks", "FOOTBALL",
                                str(external_id), fixture_id,
                            )
                            loaded_this += 1

                        per_league_loaded[current_league] = loaded_this
                        per_league_skipped[current_league] = skipped_this
                        click.echo(
                            f"Loaded {loaded_this} Football fixtures for "
                            f"league {current_league} (skipped={skipped_this})"
                        )

                    total_loaded = sum(per_league_loaded.values())
                    total_skipped = sum(per_league_skipped.values())
                    click.echo(
                        f"Total: {total_loaded} fixtures loaded across "
                        f"{len(per_league_loaded)} leagues (skipped={total_skipped})"
                    )
                finally:
                    handler.close()
    finally:
        pool.close()


@cli.command("process")
@click.option(
    "--sport",
    type=click.Choice(["nba", "nfl", "football"], case_sensitive=False),
    default=None,
    help="Filter by sport",
)
@click.option("--season", type=int, default=None, help="Filter by season")
@click.option(
    "--max", "max_fixtures", type=int, default=None, help="Max fixtures to process"
)
def process(sport: str | None, season: int | None, max_fixtures: int | None) -> None:
    """Process pending fixtures and seed event-level box scores/team stats."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    try:
        if not check_connectivity(pool):
            click.echo("Database connectivity check failed", err=True)
            sys.exit(1)

        sport_filter = sport.upper() if sport else None

        with get_conn(pool) as conn:
            from .handlers.bdl_nba import NBAHandler
            from .handlers.bdl_nfl import NFLHandler
            from .handlers.sportmonks_football import FootballHandler

            pending = get_pending(conn, sport=sport_filter, limit=max_fixtures)
            if season is not None:
                pending = [fixture for fixture in pending if fixture.season == season]

            if not pending:
                click.echo("No pending fixtures")
                return

            conn.commit()
            click.echo(f"Processing {len(pending)} fixtures")

            pending_sports = {fixture.sport for fixture in pending}
            handlers: dict[str, Any] = {}
            if ("NBA" in pending_sports or "NFL" in pending_sports) and not cfg.bdl_api_key:
                click.echo(
                    "BALLDONTLIE_API_KEY is required to process NBA/NFL fixtures",
                    err=True,
                )
                sys.exit(1)
            if "FOOTBALL" in pending_sports and not cfg.sportmonks_api_token:
                click.echo(
                    "SPORTMONKS_API_TOKEN is required to process football fixtures",
                    err=True,
                )
                sys.exit(1)

            try:
                if "NBA" in pending_sports:
                    handlers["NBA"] = NBAHandler(cfg.bdl_api_key)
                if "NFL" in pending_sports:
                    handlers["NFL"] = NFLHandler(cfg.bdl_api_key)
                if "FOOTBALL" in pending_sports:
                    handlers["FOOTBALL"] = FootballHandler(cfg.sportmonks_api_token)

                processed = 0
                failed = 0
                total_box_rows = 0
                total_team_rows = 0
                total_players_updated = 0
                total_teams_updated = 0

                for fixture in pending:
                    handler = handlers.get(fixture.sport)
                    if handler is None:
                        click.echo(
                            f"Skipping fixture {fixture.id}: unsupported sport={fixture.sport}",
                            err=True,
                        )
                        failed += 1
                        continue

                    try:
                        with conn.transaction():
                            (
                                box_rows,
                                team_rows,
                                players_updated,
                                teams_updated,
                            ) = _seed_fixture_box_scores(conn, fixture, handler)
                        processed += 1
                        total_box_rows += box_rows
                        total_team_rows += team_rows
                        total_players_updated += players_updated
                        total_teams_updated += teams_updated
                        click.echo(
                            f"Seeded fixture {fixture.id} ({fixture.sport}) "
                            f"box_rows={box_rows} team_rows={team_rows}"
                        )
                    except Exception as exc:
                        error_msg = str(exc).strip() or exc.__class__.__name__
                        with conn.transaction():
                            record_failure(conn, fixture.id, error_msg[:1000])
                        failed += 1
                        click.echo(
                            f"Failed fixture {fixture.id} ({fixture.sport}): {error_msg}",
                            err=True,
                        )

                click.echo(
                    "Done: "
                    f"fixtures_seeded={processed} "
                    f"failed={failed} "
                    f"event_box_rows={total_box_rows} "
                    f"event_team_rows={total_team_rows} "
                    f"players_updated={total_players_updated} "
                    f"teams_updated={total_teams_updated}"
                )
            finally:
                for handler in handlers.values():
                    handler.close()
    finally:
        pool.close()


if __name__ == "__main__":
    cli()
