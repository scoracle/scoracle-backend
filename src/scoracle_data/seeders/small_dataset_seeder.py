"""
Seeder for small test dataset fixture.

Reads `tests/fixtures/small_dataset.json` and upserts minimal team/player
records into the database using existing sport-specific seeders' helpers
(`upsert_team` / `upsert_player`).

This module intentionally does NOT call external APIs so it can be run in
tests without network access. It stores the fixture's endpoints mapping in
`meta` (key: `small_dataset_endpoints`) for later use by integration tests.
"""
from __future__ import annotations

import json
import logging
import os
import sys
from pathlib import Path
from typing import Any, Dict, Optional


# Allow running as a script: `python src/scoracle_data/seeders/small_dataset_seeder.py`
# by ensuring `src/` is on sys.path so absolute imports work.
if __name__ == "__main__" and __package__ is None:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

try:
    from .base import BaseSeeder
    from .nba_seeder import NBASeeder
    from .nfl_seeder import NFLSeeder
    from .football_seeder import FootballSeeder
    from ..connection import get_stats_db
    from ..pg_connection import PostgresDB
except ImportError:  # pragma: no cover
    # Fallback when executed outside package context
    from scoracle_data.seeders.base import BaseSeeder
    from scoracle_data.seeders.nba_seeder import NBASeeder
    from scoracle_data.seeders.nfl_seeder import NFLSeeder
    from scoracle_data.seeders.football_seeder import FootballSeeder
    from scoracle_data.connection import get_stats_db
    from scoracle_data.pg_connection import PostgresDB

logger = logging.getLogger(__name__)

# Map fixture sport keys to seeder classes and canonical sport IDs
SPORT_MAP = {
    "nba": (NBASeeder, "NBA"),
    "nfl": (NFLSeeder, "NFL"),
    "football": (FootballSeeder, "FOOTBALL"),
}


def _ensure_fixture_path(path: Optional[str]) -> Path:
    if path:
        p = Path(path)
    else:
        # Prefer repository root (three levels up), fall back to src-based path or CWD
        repo_root = Path(__file__).resolve().parents[3]
        p = repo_root / "tests" / "fixtures" / "small_dataset.json"
        if not p.exists():
            alt = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "small_dataset.json"
            if alt.exists():
                p = alt
            else:
                cwd_alt = Path.cwd() / "tests" / "fixtures" / "small_dataset.json"
                if cwd_alt.exists():
                    p = cwd_alt
    if not p.exists():
        raise FileNotFoundError(f"Fixture not found: {p}")
    return p


def seed_small_dataset(fixture_path: Optional[str] = None, db=None) -> Dict[str, Any]:
    """Seed the small dataset fixture into the database.

    Args:
        fixture_path: Optional path to the JSON fixture. Defaults to repo tests/fixtures.
        db: Optional StatsDB instance. If not provided, will call get_stats_db(read_only=False).

    Returns:
        A dict summary with counts and the endpoints mapping saved to meta.
    """
    fixture_file = _ensure_fixture_path(fixture_path)

    if db is None:
        # Prefer Postgres when configured, otherwise fall back to sqlite StatsDB.
        conn_str = os.environ.get("DATABASE_URL") or os.environ.get("NEON_DATABASE_URL")
        db = PostgresDB(connection_string=conn_str) if conn_str else get_stats_db(read_only=False)

    with open(fixture_file, "r", encoding="utf-8") as fh:
        payload = json.load(fh)

    meta = payload.get("meta", {})

    endpoints_map: Dict[str, Any] = {}
    summary: Dict[str, int] = {"teams": 0, "players": 0}

    # Iterate sports from fixture
    for sport_key in ("nba", "nfl", "football"):
        sport_section = payload.get(sport_key)
        if not sport_section:
            continue

        seeder_cls, sport_id = SPORT_MAP[sport_key]
        # Instantiate a lightweight seeder to reuse upsert helpers; Api client is not needed
        seeder = seeder_cls(db, api_service=None)  # type: ignore[arg-type]

        # Ensure season exists if present
        season = sport_section.get("season")
        if season:
            seeder.ensure_season(int(season))

        endpoints_map[sport_id] = {
            "teams": [],
            "players": [],
            "meta": meta,
        }

        teams = sport_section.get("teams", [])
        for t in teams:
            team_id = t.get("id")
            team_name = t.get("name")
            league_id_raw = t.get("league") or t.get("league_id") or sport_section.get("league")

            # DB schema: teams.league_id and players.current_league_id are INTEGER FKs to leagues(id).
            # - NBA: fixture uses "standard" but DB expects NULL (NBA isn't modeled via leagues table)
            # - NFL: fixture uses league=1 for API, but DB expects NULL (NFL isn't modeled via leagues table)
            # - FOOTBALL: league is an integer FK and must exist in leagues table
            league_id: Optional[int]
            if sport_id == "FOOTBALL":
                league_id = int(league_id_raw) if str(league_id_raw).isdigit() else None
            else:
                league_id = None

            # Ensure Football league exists (FK requirement)
            if sport_id == "FOOTBALL" and league_id is not None:
                try:
                    db.execute(
                        """
                        INSERT INTO leagues (id, sport_id, name, is_active)
                        VALUES (%s, %s, %s, true)
                        ON CONFLICT (id) DO UPDATE SET
                            sport_id = excluded.sport_id,
                            name = excluded.name,
                            is_active = true,
                            updated_at = NOW()
                        """,
                        (league_id, "FOOTBALL", f"League {league_id}"),
                    )
                except Exception as e:
                    logger.warning("Failed to ensure league %s exists: %s", league_id, e)

            # Minimal team payload to upsert_team
            team_payload = {
                "id": team_id,
                "name": team_name,
                "abbreviation": t.get("code") or t.get("abbreviation"),
                "logo_url": t.get("logo"),
                "conference": t.get("conference"),
                "division": t.get("division"),
                "country": t.get("country"),
                "city": t.get("city"),
                "founded": t.get("founded") or None,
                "league_id": league_id,
            }

            # Upsert team: prefer BaseSeeder helper for Postgres, but handle sqlite separately
            try:
                import sqlite3
                if hasattr(db, "connection") and isinstance(db.connection, sqlite3.Connection):
                    # Minimal SQLite-friendly upsert using INSERT OR REPLACE
                    db.execute(
                        "INSERT OR REPLACE INTO teams (id, sport_id, league_id, name, updated_at) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)",
                        (team_id, seeder.sport_id, league_id, team_name),
                    )
                    summary["teams"] += 1
                else:
                    seeder.upsert_team(team_payload, mark_profile_fetched=False)
                    summary["teams"] += 1
            except Exception as e:
                logger.warning("Failed to upsert team %s (%s): %s", team_id, team_name, e)

            endpoints_map[sport_id]["teams"].append({
                "id": team_id,
                "name": team_name,
                "profile_url": t.get("profile_url"),
                "stats_url": t.get("stats_url"),
                "standings_url": t.get("standings_url"),
            })

            # Players
            players = t.get("players", [])
            for p in players:
                player_id = p.get("id")
                player_name = p.get("name")

                # Prefer explicit fixture fields when provided (e.g., Football)
                first = p.get("first_name") or p.get("firstname")
                last = p.get("last_name") or p.get("lastname")

                # Fallback: split the display name
                if not first and not last:
                    parts = (player_name or "").split(" ", 1)
                    first = parts[0] if parts else None
                    last = parts[1] if len(parts) > 1 else None

                full_name = (f"{first} {last}".strip() if (first or last) else None) or player_name or f"player_{player_id}"

                player_payload = {
                    "id": player_id,
                    "first_name": first,
                    "last_name": last,
                    "full_name": full_name,
                    "position": None,
                    "position_group": None,
                    "nationality": None,
                    "birth_date": None,
                    "height_inches": None,
                    "weight_lbs": None,
                    "photo_url": None,
                    "current_team_id": team_id,
                    "current_league_id": league_id,
                    "jersey_number": None,
                    "college": None,
                    "experience_years": None,
                }

                try:
                    import sqlite3
                    if hasattr(db, "connection") and isinstance(db.connection, sqlite3.Connection):
                        # Minimal SQLite-friendly upsert for players
                        db.execute(
                            "INSERT OR REPLACE INTO players (id, sport_id, full_name, current_team_id, current_league_id, updated_at) VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)",
                            (player_id, seeder.sport_id, player_name, team_id, league_id),
                        )
                        summary["players"] += 1
                    else:
                        seeder.upsert_player(player_payload, mark_profile_fetched=False)
                        summary["players"] += 1
                except Exception as e:
                    logger.warning("Failed to upsert player %s (%s): %s", player_id, player_name, e)

                endpoints_map[sport_id]["players"].append({
                    "id": player_id,
                    "name": player_name,
                    "profile_url": p.get("profile_url"),
                    "stats_url": p.get("stats_url"),
                })

    # Persist endpoints mapping to meta for later inspection
    try:
        db.execute(
            """
            INSERT INTO meta (key, value, updated_at)
            VALUES (%s, %s, NOW())
            ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
            """,
            ("small_dataset_endpoints", json.dumps(endpoints_map)),
        )
    except Exception:
        # Fallback for sqlite-style set_meta
        try:
            db.set_meta("small_dataset_endpoints", json.dumps(endpoints_map))
        except Exception:
            logger.warning("Failed to persist endpoints mapping to meta")

    return {"summary": summary, "endpoints": endpoints_map}


if __name__ == "__main__":
    # Run as a script
    logging.basicConfig(level=logging.INFO)

    # Load .env if present (matches test behavior)
    try:
        from dotenv import load_dotenv  # type: ignore

        repo_root = Path(__file__).resolve().parents[3]
        load_dotenv(repo_root / ".env")
    except Exception:
        pass

    # Let seed_small_dataset decide Postgres vs sqlite based on env vars.
    result = seed_small_dataset()
    logger.info("Seed summary: %s", result.get("summary"))
