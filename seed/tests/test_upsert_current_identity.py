from __future__ import annotations

from shared.models import Player
from shared.upsert import upsert_player


class _RecordingConn:
    def __init__(self):
        self.calls: list[tuple[str, object]] = []

    def execute(self, sql, params=None):
        self.calls.append((sql, params))


def test_upsert_player_conflict_preserves_current_team_identity():
    conn = _RecordingConn()

    upsert_player(
        conn,
        "FOOTBALL",
        Player(id=1, name="Cole Palmer", first_name="Cole", last_name="Palmer", team_id=9),
    )

    sql, params = conn.calls[0]
    assert "team_id = players.team_id" in sql
    assert "team_id = COALESCE(EXCLUDED.team_id, players.team_id)" not in sql
    assert params[10] == 9  # new inserts may carry provider team; conflicts may not clobber it
