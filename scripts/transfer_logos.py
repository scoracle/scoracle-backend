"""One-shot: copy NBA+NFL team logo_url from old Neon DB into local DB.

Neon's `sport_id` is unreliable (NBA teams duplicated under sport_id='NFL';
the NFL-labeled duplicates are the rows that actually carry logo_url).
Strategy: drive from the local side. For each local team, find a Neon row
with the same normalized name that has a logo_url, ignoring Neon's sport_id.
Full team names don't collide across NBA/NFL, so this disambiguates cleanly.

Dry-run by default; pass --apply to write.
"""
from __future__ import annotations

import argparse
import os
import re
import sys

import psycopg

NEON_URL = os.environ["NEON_URL"]
LOCAL_URL = os.environ.get("DATABASE_PRIVATE_URL") or os.environ["DATABASE_URL"]


def norm(s: str | None) -> str:
    if not s:
        return ""
    return re.sub(r"[^a-z0-9]", "", s.lower())


NEON_SQL = """
SELECT id, sport_id AS sport, name, abbreviation AS short_code, logo_url
FROM teams
WHERE sport_id = ANY(%s)
"""
LOCAL_SQL = """
SELECT id, sport, name, short_code, city, logo_url
FROM teams
WHERE sport = ANY(%s)
"""


def fetch_teams(url: str, sports: tuple[str, ...], sql: str) -> list[dict]:
    with psycopg.connect(url) as conn, conn.cursor() as cur:
        cur.execute(sql, (list(sports),))
        cols = [d.name for d in cur.description]
        return [dict(zip(cols, r)) for r in cur.fetchall()]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true", help="actually write updates")
    args = ap.parse_args()

    sports = ("NBA", "NFL")
    neon = [t for t in fetch_teams(NEON_URL, sports, NEON_SQL) if t["logo_url"]]
    local = fetch_teams(LOCAL_URL, sports, LOCAL_SQL)

    print(f"Neon rows with logo_url (NBA+NFL bucket): {len(neon)}")
    print(f"Local teams total:                       {len(local)}")

    # Local stores nickname-only ("Hawks") while Neon stores full names
    # ("Atlanta Hawks"). Match by city + nickname against Neon's name.
    neon_by_name: dict[str, dict] = {}
    for n in neon:
        neon_by_name[norm(n["name"])] = n

    matches: list[tuple[dict, dict]] = []
    unmatched_local: list[dict] = []
    for l in local:
        full = norm((l["city"] or "") + (l["name"] or ""))
        hit = neon_by_name.get(full) or neon_by_name.get(norm(l["name"]))
        if hit:
            matches.append((l, hit))
        else:
            unmatched_local.append(l)

    print(f"\nMatched: {len(matches)}   Local without logo source: {len(unmatched_local)}\n")

    print(f"{'sport':<4}  {'local_name':<30} <- {'neon_name':<30}")
    print("-" * 80)
    for l, n in matches:
        print(f"{l['sport']:<4}  {l['name']:<30} <- {n['name']:<30}")

    if unmatched_local:
        print("\nLOCAL TEAMS WITH NO MATCHING NEON LOGO:")
        for l in unmatched_local:
            print(f"  {l['sport']}  {l['name']} (short={l['short_code']})")

    already_set = [l for l, _ in matches if l["logo_url"]]
    if already_set:
        print(f"\nNOTE: {len(already_set)} matched local rows ALREADY have logo_url set — will overwrite.")

    if not args.apply:
        print("\n(dry-run — rerun with --apply to write)")
        return 0

    with psycopg.connect(LOCAL_URL) as conn, conn.cursor() as cur:
        for l, n in matches:
            cur.execute(
                "UPDATE teams SET logo_url = %s WHERE id = %s",
                (n["logo_url"], l["id"]),
            )
        conn.commit()
    print(f"\nWrote logo_url for {len(matches)} teams.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
