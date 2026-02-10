"""
Tests for small dataset seeding.

These tests are integration-style and require DATABASE_URL/NEON_DATABASE_URL to be set
(similar to other postgres tests in the repo). They verify that:
- Teams and players from the fixture are inserted into `teams` and `players` tables
- A `meta` key `small_dataset_endpoints` is created containing the endpoints mapping
"""

import json


def test_seed_small_dataset_inserts_entities(neon_url):
    from scoracle_data.pg_connection import PostgresDB
    from scoracle_data.seeders.small_dataset_seeder import seed_small_dataset

    db = PostgresDB(connection_string=neon_url)

    # Ensure no leftover meta key
    db.execute("DELETE FROM meta WHERE key = %s", ("small_dataset_endpoints",))

    result = seed_small_dataset(db=db)

    assert result["summary"]["teams"] >= 1
    assert result["summary"]["players"] >= 1

    # Verify teams exist in DB
    teams = db.fetchall("SELECT id, sport, name FROM teams WHERE id IN (10, 29, 49)")
    ids = {t["id"] for t in teams}
    assert {10, 29, 49}.intersection(ids)

    # Verify players exist in DB
    players = db.fetchall(
        "SELECT id, sport, name, team_id FROM players WHERE id IN (2801, 2076, 152982)"
    )
    pids = {p["id"] for p in players}
    assert {2801, 2076, 152982}.intersection(pids)

    # Football player should have first/last name populated from fixture
    football_player = db.fetchone(
        "SELECT first_name, last_name, name FROM players WHERE id = 152982 AND sport = %s",
        ("FOOTBALL",),
    )
    assert football_player is not None
    assert football_player["first_name"] == "Cole Jermaine"
    assert football_player["last_name"] == "Palmer"

    # Verify meta entry exists and contains expected structure
    meta_row = db.fetchone(
        "SELECT value FROM meta WHERE key = %s", ("small_dataset_endpoints",)
    )
    assert meta_row is not None
    endpoints = json.loads(meta_row["value"])
    assert "NBA" in endpoints and "NFL" in endpoints and "FOOTBALL" in endpoints

    db.close()
