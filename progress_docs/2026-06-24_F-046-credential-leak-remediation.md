# 2026-06-24 — F-046 credential-leak remediation (Session 18)

Follow-up to the now-complete FIRST-GPT-AUDIT (S1–S17). This session reassessed F-046, found the
leak is **much wider than documented**, did all the autonomous remediation, and prepared the gated
steps for Scott. See `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` F-046 for the canonical writeup.

## Goal

Drive F-046 (leaked credentials in git) to closed: stop the bleed, map the true scope, and hand
Scott the disruptive/credential-bearing steps with exact commands.

## What S18 discovered (the documentation was wrong)

S17 recorded "one local Postgres password, 3 commits, 2 paths." Reality:

- **FOUR distinct secrets** (all confirmed distinct values, lengths 16 / 9 / 32 / 118):
  1. **Neon cloud Postgres password** (`neondb_owner@…neon.tech`, legacy Python era, ≥3 endpoints:
     `ep-morning-waterfall`, `ep-divine-term`, `ep-plain-bonus`) — internet-reachable, highest risk.
  2. **Local archbox `scoracle` Postgres password** — the current prod DB password (S17's find).
  3. **`API_SPORTS_KEY`** — api-sports.io provider key (CLAUDE.md still lists it as the seeder third key).
  4. **`TWITTER_BEARER_TOKEN`** — X API token (integration decommissioned O15; still revoke).
- **`.env.local` was tracked in the legacy era** (deleted in `205c173` "credential exposure") and
  survives in history with secrets 1/3/4. Gitignored now; history is the leak.
- **Three affected repos** (redacted scans of every `.git` under `/home/sheneveld`):
  - `scoracle-backend` — all 4, in 3 paths (`.env.local`, `.claude/settings.local.json`,
    `planning_docs/SELF_HOSTING_OPS.md`).
  - `dotfiles` — Neon pw, 1 history commit.
  - `/home/sheneveld/Scoracle` (capital-S legacy clone) — `API_SPORTS_KEY`, 7 history commits.
  - CLEAN: `scoracle-frontend`, `scoracle-ios`, `scoracle-api-client`, `scoracle-mobile-ui`,
    `scoracleWiki`, `scoracle` wrapper.
- **S17's working-tree scrub was incomplete** — it missed all 10 Neon-pw occurrences in
  `.claude/settings.local.json` (it only handled the local-pw `PGPASSWORD=` form).

## Done this session (autonomous, committed)

1. **Completed the working-tree scrub** — `.claude/settings.local.json` Neon pw 10 → `REDACTED_ROTATE_ME`
   (now 21 placeholders; JSON valid; tracked tree clean of all 4 literals).
2. **Stopped the leak vector** — added `.claude/settings.local.json` to `.gitignore` + `git rm --cached`.
   The shared `.claude/settings.json` + `.claude/hooks/` stay tracked.
3. Updated F-046 in the findings ledger + RUNBOOK §12 + this progress doc.

## ⚠️ Re-deriving the literals (for the gated steps — never print/commit them)

The classifier blocks any push that reproduces a literal. Derive into shell vars, redact all output:

```bash
cd /home/sheneveld/scoracle/scoracle-backend
A=$(git show 0ab6496:.claude/settings.local.json | grep -oiP "PGPASSWORD=\K[^ \"'@]+" | head -1)                    # Neon cloud pw
B=$(git show 0ab6496:.claude/settings.local.json | grep -oP "PGPASSWORD=\K[^ ]+(?= psql -h localhost)" | head -1)  # local archbox pw
K1=$(git show 3cd5a8a:.env.local | grep -m1 '^API_SPORTS_KEY=' | cut -d= -f2- | tr -d "\"' \r")                    # api-sports key
K2=$(git show 3cd5a8a:.env.local | grep -m1 '^TWITTER_BEARER_TOKEN=' | cut -d= -f2- | tr -d "\"' \r")              # twitter bearer
# verify: echo "${#A} ${#B} ${#K1} ${#K2}"  -> 16 9 32 118
```

---

## GATED ON SCOTT — Step 1: ROTATE / REVOKE (the real fix; do FIRST)

A scrub cannot un-leak history. Treat all four as compromised and rotate in risk order:

1. **Neon (highest risk — cloud-reachable).** The project migrated off Neon to local Postgres, so the
   cleanest fix is to **delete the abandoned Neon projects** (`ep-morning-waterfall`, `ep-divine-term`,
   `ep-plain-bonus`) in the Neon console — that also stops any billing. If any is still wanted, reset the
   `neondb_owner` password there instead.
2. **Local archbox `scoracle` Postgres password** (current prod). Run on archbox:
   ```bash
   sudo -u postgres psql -c "ALTER ROLE scoracle PASSWORD '<new-strong-pw>';"
   ```
   Then update `.env.local` (`DATABASE_PRIVATE_URL` / `PGPASSWORD`) on **archbox AND archx220**, and verify:
   - API authenticates: `systemctl --user restart scoracle-api.service && curl -s localhost:8000/ | grep commit`
   - seeder authenticates: a no-op seeder command against the DB
   - restore-drill still works: `scripts/hosting/restore-drill.sh <latest-dump>`
3. **`API_SPORTS_KEY`** — if still used by the seeder, rotate at the api-sports.io dashboard + update
   `.env.local`; if unused, just revoke.
4. **`TWITTER_BEARER_TOKEN`** — revoke/regenerate at the X developer portal (integration is gone).

> Suggest Scott run these himself via `! <cmd>` so new creds land in this session for verification.

---

## GATED ON SCOTT — Step 2: PURGE HISTORY + force-push (hygiene; DISRUPTIVE)

**Prerequisites:** `git filter-repo` is NOT installed — `pip install git-filter-repo` (or
`paru -S git-filter-repo`). Coordinate so the parallel **Rust session has no unpushed work**, and
have **archbox + archx220** ready to re-clone/hard-reset afterward.

### scoracle-backend (the main rewrite)

```bash
# 0. Full reversible backup FIRST (rollback = restore this mirror)
git clone --mirror /home/sheneveld/scoracle/scoracle-backend /home/sheneveld/scoracle-backend-PREPURGE.git

# 1. Author the secret-replacement file (LOCAL ONLY — never commit it). Uses the vars above.
cat > /tmp/f046-replace.txt <<EOF
$A==>REDACTED
$B==>REDACTED
$K1==>REDACTED
$K2==>REDACTED
EOF

# 2. Rewrite: delete the gitignore-class files everywhere + scrub any remaining literals in kept files.
#    (.env.local + settings.local.json should never have been tracked; path-deleting them removes
#     secrets 1/3/4 and the local pw wholesale. --replace-text scrubs the local pw left in
#     SELF_HOSTING_OPS.md history, a doc we keep.)
git filter-repo \
  --invert-paths --path .env.local --path .claude/settings.local.json \
  --replace-text /tmp/f046-replace.txt

# 3. filter-repo drops the remote — re-add and force-push all refs + tags
git remote add origin <origin-url>
git push origin --force --all
git push origin --force --tags

# 4. Verify the literals are gone from rewritten history (expect 0 for each)
for V in "$A" "$B" "$K1" "$K2"; do echo "commits w/ literal: $(git log --all --oneline -S"$V" | wc -l)"; done
rm -f /tmp/f046-replace.txt /home/sheneveld/scoracle-backend-PREPURGE.git -r  # only after verified + creds rotated

# 5. archbox + archx220: re-clone (or `git fetch && git reset --hard origin/main`). Back up each box's
#    local .claude/settings.local.json first (it's untracked now, but a stale tracked copy on archx220
#    gets deleted on the first pull of the untrack commit).
```

### dotfiles (Neon pw, 1 commit)

```bash
cd /home/sheneveld/dotfiles
git clone --mirror . ../dotfiles-PREPURGE.git
printf '%s==>REDACTED\n' "$A" > /tmp/f046-dotfiles.txt   # $A re-derived from scoracle-backend
git filter-repo --replace-text /tmp/f046-dotfiles.txt
# re-add remote + force-push as above; rm the replace file
```

### /home/sheneveld/Scoracle (capital-S legacy clone, API_SPORTS_KEY ×7)

Likely an abandoned old checkout. **Easiest: confirm it's dead and `rm -rf` it.** If it must be kept,
rewrite with `--replace-text` on `$K1` like the others.

## Rollback note

Each rewrite is preceded by a `--mirror` backup (`*-PREPURGE.git`). To roll back: `git remote set-url
origin <PREPURGE.git>` (or re-clone from it) and force-push back. Keep the PREPURGE mirrors until the
force-push is verified everywhere AND credentials are rotated — after rotation the old history is inert
even if it lingers somewhere.

## Quick reference — F-046 status

| Item | State |
|---|---|
| Working-tree scrub (all 4 secrets) | ✅ DONE (S17 partial + S18 completed Neon) |
| Leak vector stopped (untrack + gitignore `settings.local.json`) | ✅ DONE (S18) |
| Cross-repo + cross-secret scope mapped | ✅ DONE (S18) |
| Rotate/revoke the 4 credentials | ⏳ GATED ON SCOTT |
| History purge + force-push (3 repos) | ⏳ GATED ON SCOTT (filter-repo not installed) |
| archx220 back up local settings before pulling untrack commit | ⏳ coordinate |
