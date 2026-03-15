"""Tests for canonical data models."""

from scoracle_seed.models import Player, SeedResult, Team


def test_seed_result_summary():
    r = SeedResult(teams_upserted=3, players_upserted=10, player_stats_upserted=10)
    assert "teams=3" in r.summary()
    assert "players=10" in r.summary()
    assert "errors=0" in r.summary()


def test_seed_result_add():
    a = SeedResult(teams_upserted=2, players_upserted=5)
    b = SeedResult(teams_upserted=1, player_stats_upserted=3, errors=["fail"])
    a.add(b)
    assert a.teams_upserted == 3
    assert a.players_upserted == 5
    assert a.player_stats_upserted == 3
    assert len(a.errors) == 1


def test_team_defaults():
    t = Team(id=1, name="Lakers")
    assert t.short_code is None
    assert t.meta == {}


def test_player_defaults():
    p = Player(id=1, name="LeBron James")
    assert p.team_id is None
    assert p.position is None
    assert p.meta == {}
