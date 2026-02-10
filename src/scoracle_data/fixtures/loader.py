"""
Fixture loader for importing match schedules.

Supports importing from:
- CSV files
- JSON files

CSV Format:
    sport,league_id,home_team_id,away_team_id,start_time,round
    NBA,,1,2,2025-01-15T19:30:00-05:00,Week 12
    FOOTBALL,39,49,50,2025-01-18T15:00:00+00:00,Matchday 22

JSON Format:
    [
        {
            "sport": "NBA",
            "home_team_id": 1,
            "away_team_id": 2,
            "start_time": "2025-01-15T19:30:00-05:00",
            "round": "Week 12"
        },
        ...
    ]
"""

from __future__ import annotations

import csv
import json
import logging
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from ..pg_connection import PostgresDB

logger = logging.getLogger(__name__)


class FixtureLoader:
    """Load match schedules into the fixtures table."""

    def __init__(self, db: "PostgresDB"):
        """
        Initialize the loader.

        Args:
            db: PostgreSQL database connection
        """
        self.db = db

    def load_from_csv(
        self,
        file_path: str | Path,
        sport: str | None = None,
        season_year: int | None = None,
        seed_delay_hours: int = 4,
        clear_existing: bool = False,
    ) -> dict[str, int]:
        """
        Load fixtures from a CSV file.

        Args:
            file_path: Path to CSV file
            sport: Default sport if not in CSV (NBA, NFL, FOOTBALL)
            season_year: Season year (required if not derivable from start_time)
            seed_delay_hours: Hours after match start to seed stats
            clear_existing: If True, delete existing scheduled fixtures for this sport/season

        Returns:
            Summary with counts: {"loaded": N, "skipped": N, "errors": N}

        CSV columns (required):
            - home_team_id: Integer team ID
            - away_team_id: Integer team ID
            - start_time: ISO 8601 datetime

        CSV columns (optional):
            - sport: Sport identifier (NBA, NFL, FOOTBALL)
            - league_id: League ID (required for FOOTBALL)
            - round: Match round/week label
            - external_id: Provider fixture ID
            - venue_name: Venue name
        """
        file_path = Path(file_path)
        if not file_path.exists():
            raise FileNotFoundError(f"CSV file not found: {file_path}")

        summary = {"loaded": 0, "skipped": 0, "errors": 0}
        fixtures_to_insert = []

        with open(file_path, newline="", encoding="utf-8") as f:
            reader = csv.DictReader(f)

            for row_num, row in enumerate(reader, start=2):
                try:
                    fixture = self._parse_csv_row(
                        row, sport, season_year, seed_delay_hours
                    )
                    if fixture:
                        fixtures_to_insert.append(fixture)
                    else:
                        summary["skipped"] += 1
                except Exception as e:
                    logger.warning(f"Error parsing row {row_num}: {e}")
                    summary["errors"] += 1

        if not fixtures_to_insert:
            logger.warning("No valid fixtures found in CSV")
            return summary

        # Determine sport/season for clearing
        first_fixture = fixtures_to_insert[0]
        effective_sport = first_fixture["sport"]
        effective_season = first_fixture["season"]

        # Clear existing if requested
        if clear_existing and effective_sport:
            deleted = self._clear_existing_fixtures(effective_sport, effective_season)
            logger.info(f"Cleared {deleted} existing fixtures")

        # Batch insert fixtures
        summary["loaded"] = self._batch_insert_fixtures(fixtures_to_insert)

        logger.info(
            f"Loaded {summary['loaded']} fixtures from {file_path.name} "
            f"(skipped: {summary['skipped']}, errors: {summary['errors']})"
        )

        return summary

    def load_from_json(
        self,
        file_path: str | Path,
        sport: str | None = None,
        season_year: int | None = None,
        seed_delay_hours: int = 4,
        clear_existing: bool = False,
    ) -> dict[str, int]:
        """
        Load fixtures from a JSON file.

        Args:
            file_path: Path to JSON file
            sport: Default sport if not in JSON
            season_year: Season year
            seed_delay_hours: Hours after match start to seed stats
            clear_existing: If True, delete existing fixtures first

        Returns:
            Summary with counts
        """
        file_path = Path(file_path)
        if not file_path.exists():
            raise FileNotFoundError(f"JSON file not found: {file_path}")

        with open(file_path, encoding="utf-8") as f:
            data = json.load(f)

        # Handle both array and object with "fixtures" key
        if isinstance(data, dict):
            fixtures_data = data.get("fixtures", data.get("data", []))
        else:
            fixtures_data = data

        summary = {"loaded": 0, "skipped": 0, "errors": 0}
        fixtures_to_insert = []

        for idx, item in enumerate(fixtures_data):
            try:
                fixture = self._parse_json_item(
                    item, sport, season_year, seed_delay_hours
                )
                if fixture:
                    fixtures_to_insert.append(fixture)
                else:
                    summary["skipped"] += 1
            except Exception as e:
                logger.warning(f"Error parsing item {idx}: {e}")
                summary["errors"] += 1

        if not fixtures_to_insert:
            logger.warning("No valid fixtures found in JSON")
            return summary

        # Clear existing if requested
        if clear_existing and fixtures_to_insert:
            first = fixtures_to_insert[0]
            deleted = self._clear_existing_fixtures(first["sport"], first["season"])
            logger.info(f"Cleared {deleted} existing fixtures")

        # Batch insert
        summary["loaded"] = self._batch_insert_fixtures(fixtures_to_insert)

        logger.info(
            f"Loaded {summary['loaded']} fixtures from {file_path.name} "
            f"(skipped: {summary['skipped']}, errors: {summary['errors']})"
        )

        return summary

    def _parse_csv_row(
        self,
        row: dict[str, str],
        default_sport: str | None,
        season_year: int | None,
        seed_delay_hours: int,
    ) -> dict[str, Any] | None:
        """Parse a CSV row into fixture data."""
        # Required fields
        home_team_id = row.get("home_team_id")
        away_team_id = row.get("away_team_id")
        start_time = row.get("start_time")

        if not all([home_team_id, away_team_id, start_time]):
            return None

        # Accept both "sport" and legacy "sport_id" CSV headers
        sport = row.get("sport") or row.get("sport_id") or default_sport
        if not sport:
            return None

        # Parse start_time
        try:
            start_dt = datetime.fromisoformat(start_time.replace("Z", "+00:00"))
        except ValueError:
            logger.warning(f"Invalid start_time format: {start_time}")
            return None

        # Season is just the year — stored directly on the fixtures table
        season = season_year or start_dt.year

        # Optional fields
        league_id = row.get("league_id")
        if league_id:
            league_id = int(league_id)

        external_id = row.get("external_id")
        if external_id:
            external_id = int(external_id)

        return {
            "sport": sport,
            "league_id": league_id,
            "season": season,
            "home_team_id": int(home_team_id),
            "away_team_id": int(away_team_id),
            "start_time": start_dt,
            "round": row.get("round"),
            "venue_name": row.get("venue_name"),
            "external_id": external_id,
            "seed_delay_hours": seed_delay_hours,
        }

    def _parse_json_item(
        self,
        item: dict[str, Any],
        default_sport: str | None,
        season_year: int | None,
        seed_delay_hours: int,
    ) -> dict[str, Any] | None:
        """Parse a JSON item into fixture data."""
        # Support nested API-Sports format
        if "fixture" in item:
            # API-Sports format: {"fixture": {...}, "teams": {"home": {...}, "away": {...}}}
            fixture_info = item["fixture"]
            teams = item.get("teams", {})
            home_team_id = teams.get("home", {}).get("id")
            away_team_id = teams.get("away", {}).get("id")
            start_time = fixture_info.get("date")
            external_id = fixture_info.get("id")
            venue_name = fixture_info.get("venue", {}).get("name")
            round_name = item.get("league", {}).get("round")
        else:
            # Simple format
            home_team_id = item.get("home_team_id")
            away_team_id = item.get("away_team_id")
            start_time = item.get("start_time")
            external_id = item.get("external_id")
            venue_name = item.get("venue_name")
            round_name = item.get("round")

        if not all([home_team_id, away_team_id, start_time]):
            return None

        # Accept both "sport" and legacy "sport_id" keys
        sport = item.get("sport") or item.get("sport_id") or default_sport
        if not sport:
            return None

        # Parse start_time
        try:
            if isinstance(start_time, str):
                start_dt = datetime.fromisoformat(start_time.replace("Z", "+00:00"))
            else:
                start_dt = start_time
        except ValueError:
            return None

        # Season is the year — stored directly on fixtures table
        season = season_year or start_dt.year

        return {
            "sport": sport,
            "league_id": item.get("league_id"),
            "season": season,
            "home_team_id": int(home_team_id),
            "away_team_id": int(away_team_id),
            "start_time": start_dt,
            "round": round_name,
            "venue_name": venue_name,
            "external_id": int(external_id) if external_id else None,
            "seed_delay_hours": seed_delay_hours,
        }

    def _clear_existing_fixtures(self, sport: str, season: int) -> int:
        """Delete existing scheduled fixtures for a sport/season."""
        result = self.db.fetchone(
            """
            WITH deleted AS (
                DELETE FROM fixtures
                WHERE sport = %s AND season = %s AND status = 'scheduled'
                RETURNING 1
            )
            SELECT COUNT(*) as count FROM deleted
            """,
            (sport, season),
        )
        return result["count"] if result else 0

    def _batch_insert_fixtures(self, fixtures: list[dict[str, Any]]) -> int:
        """Batch insert fixtures using multi-row INSERT."""
        if not fixtures:
            return 0

        # Build VALUES clause
        placeholders = []
        params = []

        for f in fixtures:
            placeholders.append("(%s, %s, %s, %s, %s, %s, %s, %s, %s, %s)")
            params.extend(
                [
                    f.get("external_id"),
                    f["sport"],
                    f.get("league_id"),
                    f["season"],
                    f["home_team_id"],
                    f["away_team_id"],
                    f["start_time"],
                    f.get("venue_name"),
                    f.get("round"),
                    f["seed_delay_hours"],
                ]
            )

        query = f"""
            INSERT INTO fixtures (
                external_id, sport, league_id, season,
                home_team_id, away_team_id, start_time,
                venue_name, round, seed_delay_hours
            )
            VALUES {", ".join(placeholders)}
            ON CONFLICT (external_id) DO UPDATE SET
                start_time = EXCLUDED.start_time,
                venue_name = EXCLUDED.venue_name,
                round = EXCLUDED.round,
                updated_at = NOW()
            WHERE fixtures.status = 'scheduled'
        """

        self.db.execute(query, params)
        return len(fixtures)

    def get_fixture_count(self, sport: str, season_year: int) -> dict[str, int]:
        """Get fixture counts by status for a sport/season."""
        rows = self.db.fetchall(
            """
            SELECT status, COUNT(*) as count
            FROM fixtures
            WHERE sport = %s AND season = %s
            GROUP BY status
            """,
            (sport, season_year),
        )
        return {row["status"]: row["count"] for row in rows}
