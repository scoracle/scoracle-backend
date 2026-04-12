"""Search alias generation for teams and players.

Generates alternate name forms so the news service can find articles that use
different spellings, abbreviations, or transliterations of an entity name.
"""

from __future__ import annotations

import re
import unicodedata

# Common team name prefixes (football/soccer clubs).
# Matched case-insensitively against the first word of the team name.
_TEAM_PREFIXES = {p.upper() for p in [
    "FC", "CF", "SC", "AC", "AS", "SS", "SK", "FK", "BSC", "TSG",
    "VfB", "VfL", "VfR", "SV", "RC", "CD", "US", "OGC",
]}

# Character transliterations for common diacritics beyond what NFKD handles.
_TRANSLITERATIONS = {
    "ü": "u",
    "ö": "o",
    "ä": "a",
    "ß": "ss",
    "ø": "o",
    "æ": "ae",
    "ð": "d",
    "þ": "th",
    "ł": "l",
    "ñ": "n",
    "ç": "c",
    "š": "s",
    "č": "c",
    "ž": "z",
    "đ": "d",
    "ğ": "g",
    "ı": "i",
    "ş": "s",
}

# Manual overrides for teams where algorithmic aliases aren't enough.
# Map (name, sport) -> list of additional aliases.
TEAM_OVERRIDES: dict[tuple[str, str], list[str]] = {
    ("FC Bayern München", "FOOTBALL"): ["Bayern Munich", "Bayern Munchen", "FC Bayern"],
    ("FC Bayern Munchen", "FOOTBALL"): ["Bayern Munich", "Bayern München", "FC Bayern"],
    ("Borussia Mönchengladbach", "FOOTBALL"): ["Borussia Monchengladbach", "Gladbach", "BMG"],
    ("Borussia Monchengladbach", "FOOTBALL"): ["Borussia Mönchengladbach", "Gladbach", "BMG"],
    ("Atlético Madrid", "FOOTBALL"): ["Atletico Madrid", "Atleti"],
    ("Atletico Madrid", "FOOTBALL"): ["Atlético Madrid", "Atleti"],
    ("1. FC Köln", "FOOTBALL"): ["FC Koln", "Cologne", "Koln"],
    ("1. FC Koln", "FOOTBALL"): ["FC Köln", "Cologne", "Köln"],
    ("1. FC Nürnberg", "FOOTBALL"): ["FC Nurnberg", "Nuremberg", "Nurnberg"],
    ("1. FC Nurnberg", "FOOTBALL"): ["FC Nürnberg", "Nuremberg", "Nürnberg"],
}

PLAYER_OVERRIDES: dict[tuple[str, str], list[str]] = {}


def transliterate(text: str) -> str:
    """Remove diacritics and apply common transliterations."""
    # First apply explicit transliterations.
    result = text
    for src, dst in _TRANSLITERATIONS.items():
        result = result.replace(src, dst)

    # Then strip any remaining combining characters via NFKD.
    nfkd = unicodedata.normalize("NFKD", result)
    return "".join(c for c in nfkd if not unicodedata.combining(c))


def generate_team_aliases(
    name: str,
    sport: str,
    short_code: str | None = None,
    meta: dict | None = None,
) -> list[str]:
    """Generate search aliases for a team name.

    Returns a deduplicated list of alternate names, excluding the primary name.
    """
    aliases: list[str] = []

    # Manual overrides first.
    overrides = TEAM_OVERRIDES.get((name, sport.upper()), [])
    aliases.extend(overrides)

    # Strip prefix to get bare name (e.g., "FC Bayern Munchen" -> "Bayern Munchen").
    words = name.split()
    if len(words) >= 2 and words[0].upper() in _TEAM_PREFIXES:
        bare = " ".join(words[1:])
        aliases.append(bare)

    # Transliterated form (e.g., "Bayern München" -> "Bayern Munchen" or vice versa).
    transliterated = transliterate(name)
    if transliterated != name:
        aliases.append(transliterated)

    # Transliterated bare name too.
    if len(words) >= 2 and words[0].upper() in _TEAM_PREFIXES:
        bare = " ".join(words[1:])
        bare_trans = transliterate(bare)
        if bare_trans != bare:
            aliases.append(bare_trans)

    # Short code (e.g., "BAY", "FCB") — only if 3+ chars.
    if short_code and len(short_code) >= 3:
        aliases.append(short_code)

    # full_name from meta if different.
    if meta:
        full_name = meta.get("full_name")
        if full_name and full_name != name:
            aliases.append(full_name)

    # Deduplicate, preserve order, exclude primary name.
    name_lower = name.lower()
    seen: set[str] = {name_lower}
    unique: list[str] = []
    for alias in aliases:
        key = alias.lower().strip()
        if key and key not in seen:
            seen.add(key)
            unique.append(alias.strip())

    return unique


def generate_player_aliases(
    name: str,
    sport: str,
    first_name: str | None = None,
    last_name: str | None = None,
    meta: dict | None = None,
) -> list[str]:
    """Generate search aliases for a player name.

    Returns a deduplicated list of alternate names, excluding the primary name.
    """
    aliases: list[str] = []

    # Manual overrides.
    overrides = PLAYER_OVERRIDES.get((name, sport.upper()), [])
    aliases.extend(overrides)

    # Transliterated form.
    transliterated = transliterate(name)
    if transliterated != name:
        aliases.append(transliterated)

    # Short form: first + last for multi-part names (e.g., Brazilian players).
    parts = name.split()
    if len(parts) >= 3 and first_name and last_name:
        short = f"{first_name} {last_name}"
        if short != name:
            aliases.append(short)

    # common_name from meta if different.
    if meta:
        common_name = meta.get("common_name") or meta.get("display_name")
        if common_name and common_name != name:
            aliases.append(common_name)

    # Deduplicate, preserve order, exclude primary name.
    name_lower = name.lower()
    seen: set[str] = {name_lower}
    unique: list[str] = []
    for alias in aliases:
        key = alias.lower().strip()
        if key and key not in seen:
            seen.add(key)
            unique.append(alias.strip())

    return unique
