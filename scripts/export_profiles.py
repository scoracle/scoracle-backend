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

from scoracle_data.pg_connection import get_postgres_db

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
    """Export Football player profiles (with league info)."""
    rows = db.fetchall("""
        SELECT
            p.id, p.full_name as name, p.position,
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


def export_entities_minimal(output_dir: Path = DEFAULT_OUTPUT_DIR) -> dict:
    """Export all profiles to JSON for frontend autocomplete."""
    db = get_postgres_db()

    entities = []

    # Export all sports
    entities.extend(export_nba_players(db))
    entities.extend(export_nba_teams(db))
    entities.extend(export_nfl_players(db))
    entities.extend(export_nfl_teams(db))
    entities.extend(export_football_players(db))
    entities.extend(export_football_teams(db))

    # Build output structure
    output = {
        "version": "2.0",
        "generated_at": datetime.now().isoformat(),
        "counts": {
            "total": len(entities),
            "nba_players": sum(1 for e in entities if e["sport"] == "NBA" and e["type"] == "player"),
            "nba_teams": sum(1 for e in entities if e["sport"] == "NBA" and e["type"] == "team"),
            "nfl_players": sum(1 for e in entities if e["sport"] == "NFL" and e["type"] == "player"),
            "nfl_teams": sum(1 for e in entities if e["sport"] == "NFL" and e["type"] == "team"),
            "football_players": sum(1 for e in entities if e["sport"] == "FOOTBALL" and e["type"] == "player"),
            "football_teams": sum(1 for e in entities if e["sport"] == "FOOTBALL" and e["type"] == "team"),
        },
        "entities": entities,
    }

    # Ensure output directory exists
    output_dir.mkdir(parents=True, exist_ok=True)

    # Write combined output
    output_path = output_dir / "entities_minimal.json"
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(output, f, indent=2, ensure_ascii=False)

    logger.info(f"Exported {len(entities)} total entities to {output_path}")

    # Also write sport-specific files for optional separate loading
    for sport in ["NBA", "NFL", "FOOTBALL"]:
        sport_entities = [e for e in entities if e["sport"] == sport]
        sport_output = {
            "version": "2.0",
            "generated_at": output["generated_at"],
            "sport": sport,
            "count": len(sport_entities),
            "entities": sport_entities,
        }
        sport_path = output_dir / f"{sport.lower()}_entities.json"
        with open(sport_path, "w", encoding="utf-8") as f:
            json.dump(sport_output, f, indent=2, ensure_ascii=False)
        logger.info(f"Exported {len(sport_entities)} {sport} entities to {sport_path}")

    return output


def main():
    parser = argparse.ArgumentParser(description="Export profiles for frontend autocomplete")
    parser.add_argument(
        "--output", "-o",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help=f"Output directory (default: {DEFAULT_OUTPUT_DIR})"
    )
    args = parser.parse_args()

    result = export_entities_minimal(args.output)
    print(f"\nExport complete! Total entities: {result['counts']['total']}")
    print(f"Output: {args.output / 'entities_minimal.json'}")


if __name__ == "__main__":
    main()
