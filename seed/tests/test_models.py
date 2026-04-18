"""Tests for canonical data models."""

from shared.models import Player, Team


def test_team_defaults():
    t = Team(id=1, name="Lakers")
    assert t.short_code is None
    assert t.meta == {}


def test_player_defaults():
    p = Player(id=1, name="LeBron James")
    assert p.team_id is None
    assert p.position is None
    assert p.meta == {}
