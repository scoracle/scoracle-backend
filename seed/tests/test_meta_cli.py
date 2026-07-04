from __future__ import annotations

import pytest

import services.meta.cli as meta_cli


class _Rows:
    def __init__(self, rows):
        self._rows = rows

    def fetchall(self):
        return self._rows


class _RosterConn:
    def __init__(self, rows):
        self.rows = rows
        self.calls = []

    def execute(self, sql, params=None):
        self.calls.append((sql, params))
        return _Rows(self.rows)


def test_load_roster_player_ids_uses_team_rosters_universe():
    conn = _RosterConn([{"player_id": 7}, {"player_id": 9}])

    ids = meta_cli._load_roster_player_ids(
        conn,
        "NBA",
        2025,
        team_ids=[1, 2],
        max_players=25,
    )

    sql, params = conn.calls[0]
    assert ids == [7, 9]
    assert "FROM team_rosters" in sql
    assert "is_active" in sql
    assert "team_id = ANY" in sql
    assert params == ["NBA", 2025, [1, 2], 25]


class _DummyNBAHandler:
    def __init__(self, api_key):
        self.api_key = api_key
        self.closed = False

    def get_teams(self):
        from shared.models import Team

        return [Team(id=1, name="Team One")]

    def get_all_players(self, *args, **kwargs):
        raise AssertionError("meta seed must not use BDL historical player list")

    def get_player(self, player_id):
        return {
            "id": player_id,
            "first_name": "Roster",
            "last_name": "Player",
            "team": {"id": 1},
        }

    def close(self):
        self.closed = True


class _MetaConn:
    def __init__(self, roster_rows):
        self.roster_rows = roster_rows

    def execute(self, sql, params=None):
        if "FROM team_rosters" in sql:
            return _Rows(self.roster_rows)
        return _Rows([])


def test_seed_nba_metadata_enriches_roster_scope_not_bdl_historical(monkeypatch):
    monkeypatch.setattr(meta_cli, "NBAHandler", _DummyNBAHandler)
    monkeypatch.setattr(meta_cli, "upsert_team", lambda *args, **kwargs: None)
    monkeypatch.setattr(meta_cli, "upsert_player", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        meta_cli, "upsert_provider_entity_map", lambda *args, **kwargs: None
    )

    teams, players, failed, purged = meta_cli._seed_nba_metadata(
        _MetaConn([{"player_id": 7}]),
        "key",
        2025,
        None,
        None,
    )

    assert (teams, players, failed, purged) == (1, 1, 0, 0)


def test_seed_nba_metadata_requires_roster_seed_first(monkeypatch):
    monkeypatch.setattr(meta_cli, "NBAHandler", _DummyNBAHandler)
    monkeypatch.setattr(meta_cli, "upsert_team", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        meta_cli, "upsert_provider_entity_map", lambda *args, **kwargs: None
    )

    with pytest.raises(RuntimeError, match="roster seed nba"):
        meta_cli._seed_nba_metadata(_MetaConn([]), "key", 2025, None, None)


class _DummyFootballHandler:
    def __init__(self, api_token):
        self.api_token = api_token
        self.closed = False

    def get_teams(self, season_id):
        from shared.models import Team

        return [Team(id=36, name="Football Team")]

    def get_team_squad(self, *args, **kwargs):
        raise AssertionError("meta seed must not rediscover football squads")

    def get_player_profile(self, player_id):
        return {"id": player_id, "display_name": "Roster Player"}

    def close(self):
        self.closed = True


def test_seed_football_metadata_enriches_team_rosters_not_squads(monkeypatch):
    monkeypatch.setattr(meta_cli, "FootballHandler", _DummyFootballHandler)
    monkeypatch.setattr(meta_cli, "resolve_provider_season_id", lambda *args: 25659)
    monkeypatch.setattr(meta_cli, "upsert_team", lambda *args, **kwargs: None)
    monkeypatch.setattr(meta_cli, "upsert_player", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        meta_cli, "upsert_provider_entity_map", lambda *args, **kwargs: None
    )

    conn = _MetaConn(
        [{"player_id": 190919, "team_id": 36, "jersey_number": "9"}]
    )

    teams, players, failed, paused = meta_cli._seed_football_metadata(
        conn,
        "token",
        2025,
        8,
        None,
        None,
    )

    assert (teams, players, failed, paused) == (1, 1, 0, False)
