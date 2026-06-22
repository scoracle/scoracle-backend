"""Tests for event seeding CLI helpers."""

import pytest

from services.event.cli import _fixture_seed_delay_hours


@pytest.mark.parametrize(
    ("sport", "expected"),
    [("NBA", 4), ("NFL", 6), ("FOOTBALL", 3)],
)
def test_fixture_seed_delay_hours(sport: str, expected: int):
    assert _fixture_seed_delay_hours(sport) == expected
