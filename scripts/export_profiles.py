#!/usr/bin/env python3
"""
Export player and team profiles for frontend autocomplete.

Output format optimized for:
1. Client-side fuzzy search (Fuse.js)
2. Minimal payload size
3. Instant autocomplete without API calls

Usage:
    python scripts/export_profiles.py
    python scripts/export_profiles.py --output ./custom-path
"""

import argparse
import json
import logging
import unicodedata
from datetime import datetime
from pathlib import Path

from scoracle_data.pg_connection import PostgresDB, get_postgres_db

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
logger = logging.getLogger(__name__)

DEFAULT_OUTPUT_DIR = Path(__file__).parent.parent / "exports"


def normalize_name(name: str) -> str:
    """Normalize name for search matching (lowercase, no accents)."""
    if not name:
        return ""
    normalized = unicodedata.normalize("NFKD", name)
    normalized = "".join(c for c in normalized if not unicodedata.combining(c))
    return normalized.lower().strip()


def tokenize_name(name: str) -> list[str]:
    """Tokenize name for search indexing."""
    if not name:
        return []
    normalized = normalize_name(name)
    # Split on spaces, hyphens, apostrophes
    tokens = normalized.replace("-", " ").replace("'", "").split()
    return list(set(tokens))


def export_nba_players(db) -> list[dict]:
    """Export NBA player profiles."""
    rows = db.fetchall("""
        SELECT
            p.id, p.full_name as name, p.position, p.position_group,
            t.name as team_name, t.abbreviation as team_abbr
        FROM nba_player_profiles p
        LEFT JOIN nba_team_profiles t ON t.id = p.current_team_id
        WHERE p.is_active = true
    """)

    entities = []
    for p in rows:
        entities.append({
            "id": f"nba_player_{p['id']}",
            "entity_id": p["id"],
            "type": "player",
            "sport": "NBA",
            "name": p["name"],
            "normalized": normalize_name(p["name"]),
            "tokens": tokenize_name(p["name"]),
            "meta": {
                "position": p.get("position"),
                "position_group": p.get("position_group"),
                "team": p.get("team_abbr") or p.get("team_name"),
            }
        })

    logger.info(f"Exported {len(entities)} NBA players")
    return entities


def export_nba_teams(db) -> list[dict]:
    """Export NBA team profiles."""
    rows = db.fetchall("""
        SELECT id, name, abbreviation, conference, division
        FROM nba_team_profiles
        WHERE is_active = true
    """)

    entities = []
    for t in rows:
        entities.append({
            "id": f"nba_team_{t['id']}",
            "entity_id": t["id"],
            "type": "team",
            "sport": "NBA",
            "name": t["name"],
            "normalized": normalize_name(t["name"]),
            "tokens": tokenize_name(t["name"]),
            "meta": {
                "abbreviation": t.get("abbreviation"),
                "conference": t.get("conference"),
                "division": t.get("division"),
            }
        })

    logger.info(f"Exported {len(entities)} NBA teams")
    return entities


def export_nfl_players(db) -> list[dict]:
    """Export NFL player profiles."""
    rows = db.fetchall("""
        SELECT
            p.id, p.full_name as name, p.position, p.position_group,
            t.name as team_name, t.abbreviation as team_abbr
        FROM nfl_player_profiles p
        LEFT JOIN nfl_team_profiles t ON t.id = p.current_team_id
        WHERE p.is_active = true
    """)

    entities = []
    for p in rows:
        entities.append({
            "id": f"nfl_player_{p['id']}",
            "entity_id": p["id"],
            "type": "player",
            "sport": "NFL",
            "name": p["name"],
            "normalized": normalize_name(p["name"]),
            "tokens": tokenize_name(p["name"]),
            "meta": {
                "position": p.get("position"),
                "position_group": p.get("position_group"),
                "team": p.get("team_abbr") or p.get("team_name"),
            }
        })

    logger.info(f"Exported {len(entities)} NFL players")
    return entities


def export_nfl_teams(db) -> list[dict]:
    """Export NFL team profiles."""
    rows = db.fetchall("""
        SELECT id, name, abbreviation, conference, division
        FROM nfl_team_profiles
        WHERE is_active = true
    """)

    entities = []
    for t in rows:
        entities.append({
            "id": f"nfl_team_{t['id']}",
            "entity_id": t["id"],
            "type": "team",
            "sport": "NFL",
            "name": t["name"],
            "normalized": normalize_name(t["name"]),
            "tokens": tokenize_name(t["name"]),
            "meta": {
                "abbreviation": t.get("abbreviation"),
                "conference": t.get("conference"),
                "division": t.get("division"),
            }
        })

    logger.info(f"Exported {len(entities)} NFL teams")
    return entities


def export_football_players(db) -> list[dict]:
    """Export Football player profiles (with league info).

    Note: Uses first_name + last_name instead of full_name because
    API-Sports returns abbreviated names like "C. Palmer" in full_name.
    """
    rows = db.fetchall("""
        SELECT
            p.id,
            COALESCE(NULLIF(TRIM(CONCAT(p.first_name, ' ', p.last_name)), ''), p.full_name) as name,
            p.position,
            p.current_league_id as league_id,
            t.name as team_name, t.abbreviation as team_abbr,
            l.name as league_name
        FROM football_player_profiles p
        LEFT JOIN football_team_profiles t ON t.id = p.current_team_id
        LEFT JOIN leagues l ON l.id = p.current_league_id
        WHERE p.is_active = true
    """)

    entities = []
    for p in rows:
        entities.append({
            "id": f"football_player_{p['id']}",
            "entity_id": p["id"],
            "type": "player",
            "sport": "FOOTBALL",
            "name": p["name"],
            "normalized": normalize_name(p["name"]),
            "tokens": tokenize_name(p["name"]),
            "league_id": p.get("league_id"),
            "meta": {
                "position": p.get("position"),
                "team": p.get("team_abbr") or p.get("team_name"),
                "league": p.get("league_name"),
            }
        })

    logger.info(f"Exported {len(entities)} Football players")
    return entities


def export_football_teams(db) -> list[dict]:
    """Export Football team profiles (with league info)."""
    rows = db.fetchall("""
        SELECT t.id, t.name, t.abbreviation, t.country,
               t.league_id, l.name as league_name
        FROM football_team_profiles t
        LEFT JOIN leagues l ON l.id = t.league_id
        WHERE t.is_active = true
    """)

    entities = []
    for t in rows:
        entities.append({
            "id": f"football_team_{t['id']}",
            "entity_id": t["id"],
            "type": "team",
            "sport": "FOOTBALL",
            "name": t["name"],
            "normalized": normalize_name(t["name"]),
            "tokens": tokenize_name(t["name"]),
            "league_id": t.get("league_id"),
            "meta": {
                "abbreviation": t.get("abbreviation"),
                "country": t.get("country"),
                "league": t.get("league_name"),
            }
        })

    logger.info(f"Exported {len(entities)} Football teams")
    return entities


def export_sport_specific(output_dir: Path = DEFAULT_OUTPUT_DIR, db_url: str = None) -> dict:
    """
    Export profiles to sport-specific JSON files for frontend autocomplete.

    Generates separate files per sport to prevent cross-sport ID collisions.
    No combined file is generated to avoid downstream issues.

    Args:
        output_dir: Output directory for JSON files
        db_url: Optional database URL. If not provided, uses default from environment.
    """
    if db_url:
        db = PostgresDB(connection_string=db_url)
    else:
        db = get_postgres_db()

    # Ensure output directory exists
    output_dir.mkdir(parents=True, exist_ok=True)

    generated_at = datetime.now().isoformat()
    counts = {}

    # Export NBA
    nba_entities = export_nba_players(db) + export_nba_teams(db)
    nba_output = {
        "version": "2.0",
        "generated_at": generated_at,
        "sport": "NBA",
        "count": len(nba_entities),
        "entities": nba_entities,
    }
    nba_path = output_dir / "nba_entities.json"
    with open(nba_path, "w", encoding="utf-8") as f:
        json.dump(nba_output, f, indent=2, ensure_ascii=False)
    logger.info(f"Exported {len(nba_entities)} NBA entities to {nba_path}")
    counts["nba"] = len(nba_entities)

    # Export NFL
    nfl_entities = export_nfl_players(db) + export_nfl_teams(db)
    nfl_output = {
        "version": "2.0",
        "generated_at": generated_at,
        "sport": "NFL",
        "count": len(nfl_entities),
        "entities": nfl_entities,
    }
    nfl_path = output_dir / "nfl_entities.json"
    with open(nfl_path, "w", encoding="utf-8") as f:
        json.dump(nfl_output, f, indent=2, ensure_ascii=False)
    logger.info(f"Exported {len(nfl_entities)} NFL entities to {nfl_path}")
    counts["nfl"] = len(nfl_entities)

    # Export FOOTBALL
    football_entities = export_football_players(db) + export_football_teams(db)
    football_output = {
        "version": "2.0",
        "generated_at": generated_at,
        "sport": "FOOTBALL",
        "count": len(football_entities),
        "entities": football_entities,
    }
    football_path = output_dir / "football_entities.json"
    with open(football_path, "w", encoding="utf-8") as f:
        json.dump(football_output, f, indent=2, ensure_ascii=False)
    logger.info(f"Exported {len(football_entities)} FOOTBALL entities to {football_path}")
    counts["football"] = len(football_entities)

    return {
        "generated_at": generated_at,
        "counts": counts,
        "total": sum(counts.values()),
    }


def main():
    parser = argparse.ArgumentParser(description="Export profiles for frontend autocomplete")
    parser.add_argument(
        "--output", "-o",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help=f"Output directory (default: {DEFAULT_OUTPUT_DIR})"
    )
    parser.add_argument(
        "--db-url",
        type=str,
        default=None,
        help="Database URL to use (overrides environment variable)"
    )
    args = parser.parse_args()

    result = export_sport_specific(args.output, db_url=args.db_url)
    print(f"\nExport complete! Total entities: {result['total']}")
    print(f"Files: nba_entities.json, nfl_entities.json, football_entities.json")
    print(f"Output directory: {args.output}")


if __name__ == "__main__":
    main()
