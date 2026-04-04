"""Click CLI for the Scoracle seeder.

Commands:
  bootstrap-teams  — One-time team roster seed at season start
  load-fixtures    — Load fixture schedule from provider API
  process          — Process all ready fixtures (called by cron)
  seed-fixture     — Seed a single fixture by ID
  backfill         — Backfill fixture-level historical box scores
  percentiles      — Recalculate percentiles + box score coverage report
"""

from __future__ import annotations

import logging
import sys
import time
from datetime import date, datetime

import click

from . import config as config_mod
from .db import check_connectivity, create_pool, get_conn

logger = logging.getLogger("scoracle_seed")


def _setup_logging() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


@click.group()
def cli() -> None:
    """Scoracle Seed — thin data ingestion for sports statistics."""
    _setup_logging()


# -------------------------------------------------------------------------
# bootstrap-teams — one-time team roster seed
# -------------------------------------------------------------------------


@cli.command("bootstrap-teams")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, default=2025, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
def bootstrap_teams(sport: str, season: int, league: int) -> None:
    """Seed teams for a sport (one-time, at season start)."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    sport_upper = sport.upper()

    with get_conn(pool) as conn:
        if sport_upper == "NBA":
            from .bdl_nba import NBAHandler

            handler = NBAHandler(cfg.bdl_api_key)
            try:
                teams = handler.get_teams()
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "NBA", team)
                click.echo(f"Upserted {len(teams)} NBA teams")
            finally:
                handler.close()

        elif sport_upper == "NFL":
            from .bdl_nfl import NFLHandler

            handler = NFLHandler(cfg.bdl_api_key)
            try:
                teams = handler.get_teams()
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "NFL", team)
                click.echo(f"Upserted {len(teams)} NFL teams")
            finally:
                handler.close()

        elif sport_upper == "FOOTBALL":
            from .seed_football import resolve_provider_season_id, resolve_sm_league_id
            from .sportmonks_football import FootballHandler

            if not league:
                click.echo("--league is required for football", err=True)
                sys.exit(1)

            handler = FootballHandler(cfg.sportmonks_api_token)
            try:
                sm_season_id = resolve_provider_season_id(conn, league, season)
                if not sm_season_id:
                    click.echo(
                        f"No provider season found for league={league} season={season}",
                        err=True,
                    )
                    sys.exit(1)

                teams = handler.get_teams(sm_season_id)
                from .upsert import upsert_team

                for team in teams:
                    upsert_team(conn, "FOOTBALL", team)
                click.echo(f"Upserted {len(teams)} Football teams")
            finally:
                handler.close()

    pool.close()


# -------------------------------------------------------------------------
# load-fixtures — load fixture schedule from provider API
# -------------------------------------------------------------------------


@cli.command("load-fixtures")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, default=2025, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option("--from-date", type=str, default=None, help="Start date YYYY-MM-DD")
@click.option("--to-date", type=str, default=None, help="End date YYYY-MM-DD")
@click.option("--week", type=int, default=None, help="NFL week filter")
def load_fixtures(
    sport: str,
    season: int,
    league: int,
    from_date: str | None,
    to_date: str | None,
    week: int | None,
) -> None:
    """Load fixture schedule from provider API into the fixtures table."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    sport_upper = sport.upper()
    loaded = 0

    with get_conn(pool) as conn:
        from .fixtures import upsert_fixture
        from .upsert import (
            upsert_provider_entity_map,
            upsert_provider_fixture_map,
            upsert_team,
        )

        if sport_upper == "NBA":
            from .bdl_nba import NBAHandler

            handler = NBAHandler(cfg.bdl_api_key)
            try:
                games = handler.get_games(season, from_date=from_date, to_date=to_date)
                for game in games:
                    home_team = game.get("home_team")
                    away_team = game.get("away_team")
                    if home_team is not None:
                        upsert_team(conn, "NBA", home_team)
                        upsert_provider_entity_map(
                            conn,
                            "bdl",
                            "NBA",
                            "team",
                            str(home_team.id),
                            home_team.id,
                            {"source": "games"},
                        )
                    if away_team is not None:
                        upsert_team(conn, "NBA", away_team)
                        upsert_provider_entity_map(
                            conn,
                            "bdl",
                            "NBA",
                            "team",
                            str(away_team.id),
                            away_team.id,
                            {"source": "games"},
                        )

                    fixture_id = upsert_fixture(
                        conn,
                        external_id=game["external_id"],
                        sport="NBA",
                        league_id=0,
                        season=season,
                        home_team_id=game["home_team_id"],
                        away_team_id=game["away_team_id"],
                        start_time=game["start_time"],
                        round_name=game.get("round"),
                    )
                    if fixture_id:
                        upsert_provider_fixture_map(
                            conn,
                            "bdl",
                            "NBA",
                            str(game["external_id"]),
                            fixture_id,
                            {"season": season},
                        )
                        loaded += 1
            finally:
                handler.close()

        elif sport_upper == "NFL":
            from .bdl_nfl import NFLHandler

            handler = NFLHandler(cfg.bdl_api_key)
            try:
                games = handler.get_games(season, week=week)
                for game in games:
                    home_team = game.get("home_team")
                    away_team = game.get("away_team")
                    if home_team is not None:
                        upsert_team(conn, "NFL", home_team)
                        upsert_provider_entity_map(
                            conn,
                            "bdl",
                            "NFL",
                            "team",
                            str(home_team.id),
                            home_team.id,
                            {"source": "games"},
                        )
                    if away_team is not None:
                        upsert_team(conn, "NFL", away_team)
                        upsert_provider_entity_map(
                            conn,
                            "bdl",
                            "NFL",
                            "team",
                            str(away_team.id),
                            away_team.id,
                            {"source": "games"},
                        )

                    fixture_id = upsert_fixture(
                        conn,
                        external_id=game["external_id"],
                        sport="NFL",
                        league_id=0,
                        season=season,
                        home_team_id=game["home_team_id"],
                        away_team_id=game["away_team_id"],
                        start_time=game["start_time"],
                        round_name=game.get("round"),
                    )
                    if fixture_id:
                        upsert_provider_fixture_map(
                            conn,
                            "bdl",
                            "NFL",
                            str(game["external_id"]),
                            fixture_id,
                            {"season": season, "week": week},
                        )
                        loaded += 1
            finally:
                handler.close()

        elif sport_upper == "FOOTBALL":
            from .seed_football import resolve_provider_season_id
            from .sportmonks_football import FootballHandler

            if not league:
                click.echo("--league is required for football", err=True)
                sys.exit(1)

            sm_season_id = resolve_provider_season_id(conn, league, season)
            if not sm_season_id:
                click.echo(
                    f"No provider season found for league={league} season={season}",
                    err=True,
                )
                sys.exit(1)

            handler = FootballHandler(cfg.sportmonks_api_token)
            try:
                fixtures = handler.get_fixtures(sm_season_id)
                from .upsert import upsert_team

                for fx in fixtures:
                    if not _within_date_window(
                        fx["start_time"], from_date=from_date, to_date=to_date
                    ):
                        continue
                    home_team_id = fx["home_team_id"]
                    away_team_id = fx["away_team_id"]
                    home_team = fx.get("home_team")
                    away_team = fx.get("away_team")
                    if home_team is not None:
                        upsert_team(conn, "FOOTBALL", home_team)
                    if away_team is not None:
                        upsert_team(conn, "FOOTBALL", away_team)
                    fixture_id = upsert_fixture(
                        conn,
                        external_id=fx["external_id"],
                        sport="FOOTBALL",
                        league_id=league,
                        season=fx.get("season") or season,
                        home_team_id=home_team_id,
                        away_team_id=away_team_id,
                        start_time=fx["start_time"],
                        round_name=fx.get("round"),
                    )
                    if fixture_id:
                        upsert_provider_entity_map(
                            conn,
                            "sportmonks",
                            "FOOTBALL",
                            "team",
                            str(home_team_id),
                            home_team_id,
                            {"source": "fixtures"},
                        )
                        upsert_provider_entity_map(
                            conn,
                            "sportmonks",
                            "FOOTBALL",
                            "team",
                            str(away_team_id),
                            away_team_id,
                            {"source": "fixtures"},
                        )
                        upsert_provider_fixture_map(
                            conn,
                            "sportmonks",
                            "FOOTBALL",
                            str(fx["external_id"]),
                            fixture_id,
                            {"season_id": sm_season_id, "league_id": league},
                        )
                        loaded += 1
            finally:
                handler.close()

    click.echo(
        f"Loaded fixtures: sport={sport_upper} season={season} league={league} count={loaded}"
    )
    pool.close()


# -------------------------------------------------------------------------
# process — process all ready fixtures
# -------------------------------------------------------------------------


@cli.command()
@click.option(
    "--sport", type=str, default=None, help="Filter by sport (NBA, NFL, FOOTBALL)"
)
@click.option(
    "--season", type=int, default=None, help="Filter by season year (e.g., 2025)"
)
@click.option(
    "--max",
    "max_fixtures",
    type=int,
    default=None,
    help="Max fixtures to process (default: unlimited)",
)
def process(sport: str | None, season: int | None, max_fixtures: int | None) -> None:
    """Process all ready fixtures (called by cron every 30 min)."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    start = time.monotonic()

    with get_conn(pool) as conn:
        from .fixtures import get_pending

        pending = get_pending(conn, sport=sport, limit=max_fixtures)

        # Filter by season if specified
        if season is not None:
            pending = [f for f in pending if f.season == season]
            if not pending:
                click.echo(f"No pending fixtures for season {season}")
                pool.close()
                return

        if not pending:
            click.echo("No pending fixtures to seed")
            pool.close()
            return

        season_msg = f" (season={season})" if season else ""
        click.echo(f"Found {len(pending)} pending fixtures{season_msg}")

        succeeded = 0
        failed = 0

        for f in pending:
            click.echo(
                f"Seeding fixture: id={f.id} sport={f.sport} "
                f"season={f.season} league={f.league_id or 0}"
            )

            try:
                result = _seed_fixture(conn, cfg, f)

                if result.errors:
                    from .fixtures import record_failure

                    record_failure(conn, f.id, result.errors[0])
                    failed += 1
                    click.echo(f"  FAILED: {result.errors[0]}")
                else:
                    from .upsert import finalize_fixture

                    try:
                        finalize_fixture(conn, f.id)
                    except Exception as exc:
                        logger.warning("finalize_fixture %d failed: %s", f.id, exc)
                    succeeded += 1
                    click.echo(f"  OK: {result.summary()}")

            except Exception as exc:
                from .fixtures import record_failure

                record_failure(conn, f.id, str(exc))
                failed += 1
                click.echo(f"  ERROR: {exc}")

    elapsed = time.monotonic() - start
    click.echo(
        f"Process complete: succeeded={succeeded} failed={failed} "
        f"duration={elapsed:.1f}s"
    )
    pool.close()


# -------------------------------------------------------------------------
# seed-fixture — seed a single fixture by ID
# -------------------------------------------------------------------------


@cli.command("seed-fixture")
@click.option("--id", "fixture_id", type=int, required=True, help="Fixture ID")
def seed_fixture_cmd(fixture_id: int) -> None:
    """Seed a single fixture by ID."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    with get_conn(pool) as conn:
        from .fixtures import get_by_id

        f = get_by_id(conn, fixture_id)
        if not f:
            click.echo(f"Fixture {fixture_id} not found", err=True)
            sys.exit(1)

        click.echo(
            f"Seeding fixture {f.id}: sport={f.sport} season={f.season} "
            f"home={f.home_team_id} away={f.away_team_id}"
        )

        result = _seed_fixture(conn, cfg, f)

        if result.errors:
            from .fixtures import record_failure

            record_failure(conn, f.id, result.errors[0])
            click.echo(f"FAILED: {result.errors[0]}", err=True)
            sys.exit(1)

        from .upsert import finalize_fixture

        players, teams = finalize_fixture(conn, f.id)
        click.echo(
            f"OK: {result.summary()} | percentiles: players={players} teams={teams}"
        )

    pool.close()


# -------------------------------------------------------------------------
# percentiles — ad-hoc recalculation
# -------------------------------------------------------------------------


@cli.command()
@click.option("--sport", type=str, required=True, help="Sport (NBA, NFL, FOOTBALL)")
@click.option("--season", type=int, required=True, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option(
    "--required-stat",
    "required_stats",
    multiple=True,
    help="Required box score stat key (repeatable)",
)
def percentiles(
    sport: str,
    season: int,
    league: int,
    required_stats: tuple[str, ...],
) -> None:
    """Recalculate percentiles for a sport/season and report box score coverage."""
    cfg = config_mod.load()
    pool = create_pool(cfg)

    if not check_connectivity(pool):
        click.echo("Database connectivity check failed", err=True)
        sys.exit(1)

    with get_conn(pool) as conn:
        sport_upper = sport.upper()
        league_id = league if sport_upper == "FOOTBALL" else 0

        coverage = conn.execute(
            "SELECT * FROM box_score_coverage_report(%s, %s, %s, %s)",
            (sport_upper, season, league_id, list(required_stats)),
        ).fetchone()
        if coverage:
            click.echo(
                "Coverage: "
                f"fixtures={coverage['fixture_count']} "
                f"event_player_rows={coverage['player_row_count']} "
                f"event_team_rows={coverage['team_row_count']} "
                f"missing_required_keys={coverage['missing_required_keys']}"
            )

        row = conn.execute(
            "SELECT * FROM recalculate_percentiles(%s, %s)",
            (sport_upper, season),
        ).fetchone()

        if row:
            click.echo(
                f"Percentiles recalculated: players={row['players_updated']} "
                f"teams={row['teams_updated']}"
            )
        else:
            click.echo("No results returned")

    pool.close()


# -------------------------------------------------------------------------
# backfill — historical fixture + box score ingestion
# -------------------------------------------------------------------------


@cli.command("backfill")
@click.argument(
    "sport", type=click.Choice(["nba", "nfl", "football"], case_sensitive=False)
)
@click.option("--season", type=int, required=True, help="Season year")
@click.option("--league", type=int, default=0, help="League ID (football only)")
@click.option("--from-date", type=str, default=None, help="Start date YYYY-MM-DD")
@click.option("--to-date", type=str, default=None, help="End date YYYY-MM-DD")
@click.option(
    "--max",
    "max_fixtures",
    type=int,
    default=None,
    help="Max fixtures to seed (default: unlimited)",
)
def backfill(
    sport: str,
    season: int,
    league: int,
    from_date: str | None,
    to_date: str | None,
    max_fixtures: int | None,
) -> None:
    """Backfill historical fixtures and box scores."""
    ctx = click.get_current_context()
    ctx.invoke(
        load_fixtures,
        sport=sport,
        season=season,
        league=league,
        from_date=from_date,
        to_date=to_date,
        week=None,
    )
    click.echo("Backfill load-fixtures complete, processing ready fixtures...")
    ctx.invoke(process, sport=sport.upper(), max_fixtures=max_fixtures)


# -------------------------------------------------------------------------
# Internal helpers
# -------------------------------------------------------------------------


def _seed_fixture(conn, cfg, fixture):
    """Seed one fixture's event box scores. Returns SeedResult."""
    from .models import SeedResult
    from .upsert import (
        upsert_event_box_score,
        upsert_event_team_stats,
        upsert_player,
        upsert_provider_entity_map,
    )

    from .fixtures import get_provider_fixture_id

    sport = fixture.sport
    season = fixture.season
    league_id = fixture.league_id or 0
    provider = "sportmonks" if sport == "FOOTBALL" else "bdl"
    mapped_fixture_id = get_provider_fixture_id(conn, fixture.id, provider, sport)
    ext_id_raw = mapped_fixture_id or (
        str(fixture.external_id) if fixture.external_id is not None else None
    )
    if ext_id_raw is None:
        result = SeedResult()
        result.add_error(
            f"fixture {fixture.id} missing provider fixture mapping and external_id"
        )
        return result
    try:
        ext_id = int(ext_id_raw)
    except ValueError:
        result = SeedResult()
        result.add_error(
            f"fixture {fixture.id} invalid provider fixture id: {ext_id_raw}"
        )
        return result
    if ext_id <= 0:
        result = SeedResult()
        result.add_error(
            f"fixture {fixture.id} invalid provider fixture id: {ext_id_raw}"
        )
        return result

    if sport == "NBA":
        from .bdl_nba import NBAHandler

        handler = NBAHandler(cfg.bdl_api_key)
        try:
            player_lines, team_lines = handler.get_box_score(ext_id, fixture.id)
            result = SeedResult()
            for line in player_lines:
                if line.player:
                    upsert_player(conn, "NBA", line.player)
                    upsert_provider_entity_map(
                        conn,
                        "bdl",
                        "NBA",
                        "player",
                        str(line.player_id),
                        line.player_id,
                        {"source": "box_score"},
                    )
                upsert_provider_entity_map(
                    conn,
                    "bdl",
                    "NBA",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_box_score(conn, "NBA", season, league_id, line)
                result.event_box_scores_upserted += 1
            for line in team_lines:
                upsert_provider_entity_map(
                    conn,
                    "bdl",
                    "NBA",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_team_stats(conn, "NBA", season, league_id, line)
                result.event_team_stats_upserted += 1
            if not player_lines and not team_lines:
                result.add_error("no box score rows returned")
            return result
        finally:
            handler.close()

    elif sport == "NFL":
        from .bdl_nfl import NFLHandler

        handler = NFLHandler(cfg.bdl_api_key)
        try:
            player_lines, team_lines = handler.get_box_score(ext_id, fixture.id)
            result = SeedResult()
            for line in player_lines:
                if line.player:
                    upsert_player(conn, "NFL", line.player)
                    upsert_provider_entity_map(
                        conn,
                        "bdl",
                        "NFL",
                        "player",
                        str(line.player_id),
                        line.player_id,
                        {"source": "box_score"},
                    )
                upsert_provider_entity_map(
                    conn,
                    "bdl",
                    "NFL",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_box_score(conn, "NFL", season, league_id, line)
                result.event_box_scores_upserted += 1
            for line in team_lines:
                upsert_provider_entity_map(
                    conn,
                    "bdl",
                    "NFL",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_team_stats(conn, "NFL", season, league_id, line)
                result.event_team_stats_upserted += 1
            if not player_lines and not team_lines:
                result.add_error("no box score rows returned")
            return result
        finally:
            handler.close()

    elif sport == "FOOTBALL":
        from .sportmonks_football import FootballHandler

        handler = FootballHandler(cfg.sportmonks_api_token)
        try:
            player_lines, team_lines = handler.get_box_score(ext_id, fixture.id)
            result = SeedResult()
            for line in player_lines:
                if line.player:
                    upsert_player(conn, "FOOTBALL", line.player)
                    upsert_provider_entity_map(
                        conn,
                        "sportmonks",
                        "FOOTBALL",
                        "player",
                        str(line.player_id),
                        line.player_id,
                        {"source": "box_score"},
                    )
                upsert_provider_entity_map(
                    conn,
                    "sportmonks",
                    "FOOTBALL",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_box_score(conn, "FOOTBALL", season, league_id, line)
                result.event_box_scores_upserted += 1
            for line in team_lines:
                upsert_provider_entity_map(
                    conn,
                    "sportmonks",
                    "FOOTBALL",
                    "team",
                    str(line.team_id),
                    line.team_id,
                    {"source": "box_score"},
                )
                upsert_event_team_stats(conn, "FOOTBALL", season, league_id, line)
                result.event_team_stats_upserted += 1
            if not player_lines and not team_lines:
                result.add_error("no box score rows returned")
            return result
        finally:
            handler.close()

    else:
        result = SeedResult()
        result.add_error(f"unknown sport: {sport}")
        return result


def _parse_iso_date(value: str | None) -> date | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).date()
    except ValueError:
        if len(value) >= 10:
            try:
                return date.fromisoformat(value[:10])
            except ValueError:
                return None
        return None


def _within_date_window(
    start_time: str,
    from_date: str | None,
    to_date: str | None,
) -> bool:
    fx_date = _parse_iso_date(start_time)
    if fx_date is None:
        return True
    from_d = _parse_iso_date(from_date)
    to_d = _parse_iso_date(to_date)
    if from_d and fx_date < from_d:
        return False
    if to_d and fx_date > to_d:
        return False
    return True
