"""
Shared helpers for autofill entity building.

Used by both:
- CLI export-profiles command (sync, writes JSON files)
- API /autofill_databases endpoint (async, serves live data)

Contains pure functions only — no DB access, no I/O.
"""

from __future__ import annotations

import unicodedata
from typing import Any


# =============================================================================
# Position group mappings
# =============================================================================

NBA_POS_GROUP: dict[str, str] = {
    "G": "Guard", "PG": "Guard", "SG": "Guard",
    "F": "Forward", "SF": "Forward", "PF": "Forward",
    "C": "Center",
    "G-F": "Guard-Forward", "F-G": "Guard-Forward",
    "F-C": "Forward-Center", "C-F": "Forward-Center",
}

NFL_POS_GROUP: dict[str, str] = {
    "QB": "Offense", "RB": "Offense - Skill", "FB": "Offense",
    "WR": "Offense - Skill", "TE": "Offense - Skill",
    "OT": "Offense - Line", "OG": "Offense - Line", "C": "Offense - Line",
    "OL": "Offense - Line", "T": "Offense - Line", "G": "Offense - Line",
    "DE": "Defense - Line", "DT": "Defense - Line", "DL": "Defense - Line",
    "NT": "Defense - Line",
    "LB": "Defense - Linebacker", "OLB": "Defense - Linebacker",
    "ILB": "Defense - Linebacker", "MLB": "Defense - Linebacker",
    "CB": "Defense - Secondary", "S": "Defense - Secondary",
    "SS": "Defense - Secondary", "FS": "Defense - Secondary",
    "DB": "Defense - Secondary",
    "K": "Special Teams", "P": "Special Teams", "LS": "Special Teams",
    "KR": "Special Teams", "PR": "Special Teams",
}


# =============================================================================
# Text normalization
# =============================================================================

def normalize_text(text: str) -> str:
    """Lowercase + strip accents for fuzzy search."""
    nfkd = unicodedata.normalize("NFKD", text)
    return "".join(c for c in nfkd if not unicodedata.combining(c)).lower()


def tokenize(name: str) -> list[str]:
    """Split name into search tokens, sorted alphabetically."""
    return sorted(name.lower().split())


# =============================================================================
# Entity builders — convert DB rows to v2.0 bootstrap format
# =============================================================================

def build_player_entity(row: dict[str, Any], sport: str) -> dict[str, Any] | None:
    """Build a player entity dict from a DB row.

    Args:
        row: Dict from a players query. Expected keys depend on sport:
            All:      id, name, position
            NBA/NFL:  team_abbr
            FOOTBALL: team_name, team_abbr, league_id, league_name
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Entity dict in v2.0 format, or None if row has no name.
    """
    if not row.get("name"):
        return None

    entity: dict[str, Any] = {
        "id": f"{sport.lower()}_player_{row['id']}",
        "entity_id": row["id"],
        "type": "player",
        "sport": sport,
        "name": row["name"],
        "normalized": normalize_text(row["name"]),
        "tokens": tokenize(row["name"]),
    }

    meta: dict[str, Any] = {}
    position = row.get("position")
    if position:
        meta["position"] = position

    if sport == "NBA" and position:
        pg = NBA_POS_GROUP.get(position, "")
        if pg:
            meta["position_group"] = pg
    elif sport == "NFL" and position:
        pg = NFL_POS_GROUP.get(position, "")
        if pg:
            meta["position_group"] = pg

    team_abbr = row.get("team_abbr")
    if team_abbr:
        meta["team"] = team_abbr
    elif sport == "FOOTBALL" and row.get("team_name"):
        meta["team"] = row["team_name"]

    if sport == "FOOTBALL":
        league_id = row.get("league_id")
        if league_id:
            entity["league_id"] = league_id
        league_name = row.get("league_name")
        if league_name:
            meta["league"] = league_name

    entity["meta"] = meta
    return entity


def build_team_entity(row: dict[str, Any], sport: str) -> dict[str, Any] | None:
    """Build a team entity dict from a DB row.

    Args:
        row: Dict from a teams query. Expected keys depend on sport:
            All:      id, name, short_code
            NBA/NFL:  conference, division
            FOOTBALL: country, league_id, league_name
        sport: Sport identifier (NBA, NFL, FOOTBALL).

    Returns:
        Entity dict in v2.0 format, or None if row has no name.
    """
    if not row.get("name"):
        return None

    entity: dict[str, Any] = {
        "id": f"{sport.lower()}_team_{row['id']}",
        "entity_id": row["id"],
        "type": "team",
        "sport": sport,
        "name": row["name"],
        "normalized": normalize_text(row["name"]),
        "tokens": tokenize(row["name"]),
    }

    meta: dict[str, Any] = {}
    if row.get("short_code"):
        meta["abbreviation"] = row["short_code"]

    if sport in ("NBA", "NFL"):
        if row.get("conference"):
            meta["conference"] = row["conference"]
        if row.get("division"):
            meta["division"] = row["division"]
    elif sport == "FOOTBALL":
        if row.get("country"):
            meta["country"] = row["country"]
        league_id = row.get("league_id")
        if league_id:
            entity["league_id"] = league_id
        league_name = row.get("league_name")
        if league_name:
            meta["league"] = league_name

    entity["meta"] = meta
    return entity
