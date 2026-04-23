"""One-shot: copy NBA+NFL team logo_url from old Neon DB into local DB.

Match strategy: short_code first, then normalized name. IDs differ between DBs.
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
SELECT id, sport, name, short_code, logo_url
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

    print(f"Neon teams with logo_url: {len(neon)}")
    print(f"Local teams total:       {len(local)}")

    # Build lookup indices on local side, per sport.
    by_short: dict[tuple[str, str], dict] = {}
    by_name: dict[tuple[str, str], dict] = {}
    for t in local:
        by_short[(t["sport"], (t["short_code"] or "").upper())] = t
        by_name[(t["sport"], norm(t["name"]))] = t

    matches: list[tuple[dict, dict, str]] = []
    unmatched: list[dict] = []
    for n in neon:
        sport = n["sport"]
        hit = None
        how = ""
        if n["short_code"]:
            hit = by_short.get((sport, n["short_code"].upper()))
            if hit:
                how = "short_code"
        if not hit:
            hit = by_name.get((sport, norm(n["name"])))
            if hit:
                how = "name"
        if hit:
            matches.append((n, hit, how))
        else:
            unmatched.append(n)

    print(f"\nMatched: {len(matches)}   Unmatched: {len(unmatched)}\n")

    print(f"{'sport':<4}  {'neon_name':<30} -> {'local_name':<30}  via")
    print("-" * 90)
    for n, l, how in matches:
        print(f"{n['sport']:<4}  {n['name']:<30} -> {l['name']:<30}  {how}")

    if unmatched:
        print("\nUNMATCHED (no write for these):")
        for n in unmatched:
            print(f"  {n['sport']}  {n['name']} (short={n['short_code']})")

    # Check which local rows already have a logo_url (should be 0 per user)
    already_set = [l for _, l, _ in matches if l["logo_url"]]
    if already_set:
        print(f"\nNOTE: {len(already_set)} matched local rows ALREADY have logo_url set — will overwrite.")

    if not args.apply:
        print("\n(dry-run — rerun with --apply to write)")
        return 0

    with psycopg.connect(LOCAL_URL) as conn, conn.cursor() as cur:
        for n, l, _ in matches:
            cur.execute(
                "UPDATE teams SET logo_url = %s WHERE id = %s",
                (n["logo_url"], l["id"]),
            )
        conn.commit()
    print(f"\nWrote logo_url for {len(matches)} teams.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
