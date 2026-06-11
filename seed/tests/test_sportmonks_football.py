"""Tests for the SportMonks football event handler's raw-payload parsing."""

from services.event.handlers.sportmonks_football import _parse_player


def test_parse_player_strips_trailing_nbsp():
    # SportMonks ships some display_names with a trailing non-breaking space
    # (U+00A0). It must not survive into players.name (renders as a stray space).
    p = _parse_player({"id": 997, "display_name": "Harry Kane "})
    assert p.name == "Harry Kane"


def test_parse_player_strips_surrounding_whitespace():
    p = _parse_player({"id": 1, "display_name": "  Kevin De Bruyne  "})
    assert p.name == "Kevin De Bruyne"


def test_parse_player_falls_back_to_first_last_when_no_display_name():
    p = _parse_player({"id": 2, "firstname": "Jude", "lastname": "Bellingham"})
    assert p.name == "Jude Bellingham"


def test_parse_player_falls_back_to_placeholder_when_nameless():
    p = _parse_player({"id": 42})
    assert p.name == "Player 42"
