"""One-off: re-fetch team rows for a single football league/season from
SportMonks and overwrite logo_url where the upsert provides a non-null
value. Avoids the heavy squad/player path of the full meta seed.

Use after correcting cross-sport logo corruption (see
scripts/ops/refresh_football_team_logos.md if added later).

Usage:
    python scripts/ops/refresh_football_team_logos.py --league 8 --season 2025
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Make the seed package importable when running as a script.
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "seed"))

from shared import config as config_mod  # noqa: E402
from shared.db import (  # noqa: E402
    check_connectivity,
    create_pool,
    get_conn,
    resolve_provider_season_id,
)
from shared.upsert import upsert_provider_entity_map, upsert_team  # noqa: E402
from services.event.handlers.sportmonks_football import FootballHandler  # noqa: E402


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--league", type=int, required=True, help="DB league_id")
    p.add_argument("--season", type=int, required=True, help="Season year")
    args = p.parse_args()

    cfg = config_mod.load()
    if not cfg.sportmonks_api_token:
        print("SPORTMONKS_API_TOKEN required", file=sys.stderr)
        return 1

    pool = create_pool(cfg)
    try:
        if not check_connectivity(pool):
            print("DB connectivity check failed", file=sys.stderr)
            return 1

        with get_conn(pool) as conn:
            sm_season_id = resolve_provider_season_id(conn, args.league, args.season)
            if not sm_season_id:
                print(
                    f"No SportMonks season mapping for league={args.league} "
                    f"season={args.season}",
                    file=sys.stderr,
                )
                return 1

            handler = FootballHandler(cfg.sportmonks_api_token)
            try:
                teams = handler.get_teams(sm_season_id)
                print(
                    f"Fetched {len(teams)} teams from SportMonks "
                    f"(league={args.league} sm_season={sm_season_id})"
                )
                refreshed = 0
                for team in teams:
                    team.league_id = args.league
                    upsert_team(conn, "FOOTBALL", team)
                    upsert_provider_entity_map(
                        conn, "sportmonks", "FOOTBALL", "team",
                        str(team.id), team.id,
                    )
                    if team.logo_url:
                        refreshed += 1
                print(
                    f"Upserted {len(teams)} teams; {refreshed} had non-null logos."
                )
            finally:
                handler.close()
    finally:
        pool.close()

    return 0


if __name__ == "__main__":
    sys.exit(main())
