"""Per-sport fixture completeness contract (FIRST-GPT-AUDIT Session 4).

A fixture is finalized (``status='seeded'``) only when the provider payload is
*complete enough to serve*, not merely "the provider returned something". The
acceptance predicate is:

    NOT provider_known_not_final
    AND expected_home_team_present
    AND expected_away_team_present
    AND home_score_present AND away_score_present
    AND player_rows_meet_sport_expectation

Finality is treated tri-state: a *recognised* not-final provider status (an
abandoned/postponed/in-progress game) is rejected outright; a recognised final
status confirms completeness; an unknown/blank status defers to the structural
checks, which are themselves strong (an unfinished game cannot satisfy both
final scores + a full player count). This keeps a provider label we have not
enumerated from silently stalling the live pipeline while still tightening
finality where the provider gives us an authoritative signal.

The gate lives in the seeder (ingestion-acceptance policy), not in SQL — it
decides whether to accept a provider payload, which is distinct from the
derived-stat/percentile logic that CLAUDE.md keeps in Postgres.
"""

from __future__ import annotations

from dataclasses import dataclass

from shared.models import BoxScoreResult


class IncompleteFixtureError(Exception):
    """Raised when a provider payload fails the completeness contract.

    Distinct from a transport failure: the fixture is left pending/retryable
    and the reason is recorded separately (fixtures.last_incomplete_reason),
    never marked seeded.
    """


# SportMonks fixture state codes that mean the match is finished and carries a
# servable box score.
_FOOTBALL_FINAL_STATES = {"FT", "AET", "FT_PEN"}

# SportMonks states that positively mean NOT (yet) a complete, servable match.
# Recognised here so an abandoned/postponed fixture with a partial lineup is
# rejected rather than half-seeded.
_FOOTBALL_NONFINAL_STATES = {
    "NS", "INPLAY_1ST_HALF", "HT", "INPLAY_2ND_HALF", "INPLAY_ET",
    "BREAK", "INPLAY_PENALTIES", "PEN_BREAK", "EXTRA_TIME_BREAK", "ET_BREAK",
    "POSTPONED", "CANCELLED", "ABANDONED", "SUSPENDED", "DELETED",
    "AWARDED", "WALKOVER", "INTERRUPTED", "TBA", "AWAITING_UPDATES",
    "PENDING",
}

# Minimum player box-score rows for a fixture to count as "complete enough".
# Conservative anti-partial floors, well below any real complete game
# (NBA ~26, NFL ~50, football ~28 lineup rows) yet clearly above a team-only
# (0) or one-player partial. Not a full-completeness assertion — the team and
# score checks carry that weight.
_MIN_PLAYER_ROWS = {"NBA": 10, "NFL": 10, "FOOTBALL": 10}


@dataclass
class CompletenessVerdict:
    """Outcome of evaluating a provider payload against the contract."""

    accepted: bool
    reason: str | None = None


def is_final(sport: str, status: str | None) -> bool | None:
    """Tri-state provider finality from the raw status label.

    True  — provider reports the game final/complete.
    False — provider reports a recognised not-final state (reject).
    None  — status missing/unrecognised (defer to the structural checks).
    """
    if status is None:
        return None
    s = status.strip()
    if not s:
        return None

    if sport in ("NBA", "NFL"):
        # BallDontLie reports "Final" (sometimes "Final/OT") for completed
        # games; scheduled games carry an ISO datetime and in-progress games a
        # period label. We only positively confirm finality on "Final*" and
        # otherwise defer — we never hard-reject NBA/NFL on the status string
        # alone, leaning on the structural checks instead.
        if s.lower().startswith("final"):
            return True
        return None

    if sport == "FOOTBALL":
        su = s.upper()
        if su in _FOOTBALL_FINAL_STATES:
            return True
        if su in _FOOTBALL_NONFINAL_STATES:
            return False
        return None

    return None


def evaluate_completeness(
    sport: str,
    home_team_id: int,
    away_team_id: int,
    result: BoxScoreResult,
) -> CompletenessVerdict:
    """Decide whether a provider payload is complete enough to finalize.

    Pure function — no I/O. The caller runs this *before* mutating
    event_box_scores/event_team_stats so a rejected payload leaves existing
    rows intact and the fixture pending/retryable.
    """
    # Reject recognised not-final games up front.
    if is_final(sport, result.provider_status) is False:
        return CompletenessVerdict(
            False, f"provider status not final: {result.provider_status!r}"
        )

    # Both expected teams must be present in the team rows.
    team_rows = {t.team_id: t for t in result.teams}
    if home_team_id not in team_rows:
        return CompletenessVerdict(
            False, f"expected home team {home_team_id} missing from event_team_stats"
        )
    if away_team_id not in team_rows:
        return CompletenessVerdict(
            False, f"expected away team {away_team_id} missing from event_team_stats"
        )

    # Both final scores must be present (None means absent — a legitimate 0 is
    # fine, e.g. a 0-0 football draw).
    if team_rows[home_team_id].score is None:
        return CompletenessVerdict(
            False, f"home team {home_team_id} has no final score"
        )
    if team_rows[away_team_id].score is None:
        return CompletenessVerdict(
            False, f"away team {away_team_id} has no final score"
        )

    # A meaningful number of player rows must be present.
    min_players = _MIN_PLAYER_ROWS.get(sport, 1)
    if len(result.players) < min_players:
        return CompletenessVerdict(
            False,
            f"only {len(result.players)} player rows (expected >= {min_players})",
        )

    return CompletenessVerdict(True)
