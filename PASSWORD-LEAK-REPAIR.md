# PASSWORD-LEAK-REPAIR.md — F-046 credential-leak repair runbook

Standalone, step-by-step repair for the F-046 credential leak. The **autonomous half is done**
(working tree scrubbed, leak vector stopped); the steps below are **gated on Scott** and are the
*only* real fix. Canonical findings: `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` F-046. Discovery
narrative: `progress_docs/2026-06-24_F-046-credential-leak-remediation.md`. Summary in `RUNBOOK.md` §12.

## Status

| Phase | State |
|---|---|
| Working-tree scrub (all 4 secrets) | ✅ DONE (commit `fa9aeba`) |
| Leak vector stopped — `.claude/settings.local.json` untracked + gitignored | ✅ DONE (`fa9aeba`) |
| Full cross-repo / cross-secret scope mapped | ✅ DONE |
| **Step 1 — Rotate / revoke the 4 credentials** | ⏳ **GATED ON SCOTT (do FIRST)** |
| **Step 2 — Purge git history + force-push (3 repos)** | ⏳ GATED ON SCOTT |
| **Step 3 — Post-repair verification** | ⏳ |

## What leaked (scope)

Four **distinct** secrets — NOT the single password first recorded:

| # | Secret | Where (history) | Risk |
|---|---|---|---|
| 1 | **Neon CLOUD Postgres pw** (`neondb_owner@…neon.tech`; endpoints `ep-morning-waterfall`, `ep-divine-term`, `ep-plain-bonus`) | `.claude/settings.local.json` + historical `.env.local` | **Highest — internet-reachable** |
| 2 | **Local archbox `scoracle` Postgres pw** (current prod DB) | `.claude/settings.local.json` + `planning_docs/SELF_HOSTING_OPS.md` | Medium (localhost) |
| 3 | **`API_SPORTS_KEY`** (api-sports.io) | historical `.env.local` | Medium (may be active — CLAUDE.md lists it as seeder "third key") |
| 4 | **`TWITTER_BEARER_TOKEN`** | historical `.env.local` | Low (X decommissioned O15) — still revoke |

**Affected repos:** `scoracle-backend` (all 4, 3 paths incl. a historically-tracked `.env.local`),
`dotfiles` (Neon pw ×1 commit), `/home/sheneveld/Scoracle` (capital-S legacy clone — `API_SPORTS_KEY`
×7 commits). **CLEAN:** `scoracle-frontend`, `scoracle-ios`, `scoracle-api-client`,
`scoracle-mobile-ui`, `scoracleWiki`, the `scoracle` wrapper.

## ⚠️ Critical handling rule

**Never print, write, or commit a literal secret.** The auto-mode safety classifier blocks any push
that reproduces one. Derive each into a redacted shell var; only print file names / counts.

```bash
cd /home/sheneveld/scoracle/scoracle-backend
A=$(git show 0ab6496:.claude/settings.local.json | grep -oiP "PGPASSWORD=\K[^ \"'@]+" | head -1)                    # Neon cloud pw
B=$(git show 0ab6496:.claude/settings.local.json | grep -oP "PGPASSWORD=\K[^ ]+(?= psql -h localhost)" | head -1)  # local archbox pw
K1=$(git show 3cd5a8a:.env.local | grep -m1 '^API_SPORTS_KEY=' | cut -d= -f2- | tr -d "\"' \r")                    # api-sports key
K2=$(git show 3cd5a8a:.env.local | grep -m1 '^TWITTER_BEARER_TOKEN=' | cut -d= -f2- | tr -d "\"' \r")              # twitter bearer
echo "${#A} ${#B} ${#K1} ${#K2}"   # expect: 16 9 32 118
```

---

## STEP 1 — Rotate / revoke (the real fix; do FIRST)

A scrub can't un-leak history. Treat all four as compromised. Risk order:

1. **Neon (highest — cloud-reachable).** Project migrated off Neon to local Postgres → cleanest is
   to **delete the abandoned Neon projects** (`ep-morning-waterfall`, `ep-divine-term`, `ep-plain-bonus`)
   in the Neon console (also stops billing). If any is still wanted, reset `neondb_owner` there instead.
2. **Local archbox `scoracle` pw** (current prod). On archbox:
   ```bash
   sudo -u postgres psql -c "ALTER ROLE scoracle PASSWORD '<new-strong-pw>';"
   ```
   Then update `.env.local` (`DATABASE_PRIVATE_URL` / `PGPASSWORD`) on **archbox AND archx220**, and verify:
   ```bash
   systemctl --user restart scoracle-api.service && curl -s localhost:8000/ | grep commit   # API authenticates
   scripts/hosting/restore-drill.sh <latest-dump>                                            # restore-drill authenticates
   # plus a no-op seeder command against the DB
   ```
3. **`API_SPORTS_KEY`** — if the seeder still uses it, rotate at the api-sports.io dashboard + update
   `.env.local`; else revoke.
4. **`TWITTER_BEARER_TOKEN`** — revoke/regenerate at the X developer portal (integration is gone).

> Run these via `! <cmd>` in the Claude session so new creds land in-session for verification.

---

## STEP 2 — Purge git history + force-push (hygiene; DISRUPTIVE)

**Prereqs:** `git filter-repo` is NOT installed — `pip install git-filter-repo` (or `paru -S
git-filter-repo`). Ensure the parallel **Rust session has no unpushed work**, and have **archbox +
archx220** ready to re-clone/hard-reset after.

### scoracle-backend (main rewrite)

```bash
# 0. Reversible backup FIRST (rollback = restore this mirror)
git clone --mirror /home/sheneveld/scoracle/scoracle-backend /home/sheneveld/scoracle-backend-PREPURGE.git

# 1. Local-only secret-replacement file (NEVER commit). Uses the vars from the handling-rule block.
cat > /tmp/f046-replace.txt <<EOF
$A==>REDACTED
$B==>REDACTED
$K1==>REDACTED
$K2==>REDACTED
EOF

# 2. Path-delete the gitignore-class files everywhere + scrub any literal left in kept files.
git filter-repo \
  --invert-paths --path .env.local --path .claude/settings.local.json \
  --replace-text /tmp/f046-replace.txt

# 3. filter-repo drops the remote — re-add + force-push all refs + tags
git remote add origin <origin-url>
git push origin --force --all
git push origin --force --tags

# 4. Verify (expect 0 each)
for V in "$A" "$B" "$K1" "$K2"; do echo "commits w/ literal: $(git log --all --oneline -S"$V" | wc -l)"; done
rm -f /tmp/f046-replace.txt   # keep the PREPURGE mirror until verified everywhere + creds rotated

# 5. archbox + archx220: re-clone or `git fetch && git reset --hard origin/main`.
#    Back up each box's local .claude/settings.local.json first (a stale tracked copy on archx220
#    is deleted on the first pull of the untrack commit fa9aeba).
```

### dotfiles (Neon pw ×1)

```bash
cd /home/sheneveld/dotfiles
git clone --mirror . ../dotfiles-PREPURGE.git
printf '%s==>REDACTED\n' "$A" > /tmp/f046-dotfiles.txt
git filter-repo --replace-text /tmp/f046-dotfiles.txt
# re-add remote + force-push as above; rm the replace file
```

### /home/sheneveld/Scoracle (capital-S legacy clone, API_SPORTS_KEY ×7)

Likely an abandoned old checkout. **Easiest: confirm dead and `rm -rf` it.** If kept, rewrite with
`--replace-text` on `$K1`.

---

## STEP 3 — Post-repair verification

- [ ] All 4 credentials rotated/revoked (Step 1).
- [ ] `for V in A B K1 K2; do git log --all -S"$V" | wc -l; done` → **0** in every affected repo.
- [ ] API + seeder + restore-drill authenticate with the new archbox pw.
- [ ] archbox + archx220 re-cloned/reset onto the rewritten history; their local
      `.claude/settings.local.json` restored as ignored untracked files.
- [ ] PREPURGE mirrors deleted only after all the above.
- [ ] Findings F-046 status → ✅ RESOLVED; `RUNBOOK.md` §12 + memory updated.

## Coordination & rollback

- **archx220:** still tracks `.claude/settings.local.json` until it pulls `fa9aeba` — back up the local
  copy there first, or pulling deletes it.
- **Parallel Rust session:** shares the archbox tree. No history rewrite while it has unpushed work;
  `git fetch` before any commit; stage only your own files (never `git add -A`); migration `099` +
  `rust/*` are its artifacts — leave untracked.
- **Rollback:** each rewrite is preceded by a `--mirror` backup (`*-PREPURGE.git`). To undo, re-clone
  from it (or `git remote set-url origin <PREPURGE.git>`) and force-push back. After rotation the old
  history is inert even if a copy lingers.

---

## Appendix — Next-session handoff prompt

Paste this to start the session that executes Steps 1–3 with Scott:

> **F-046 credential-leak repair — EXECUTION session (the gated half).** The autonomous half is done
> and pushed (`fa9aeba` on `origin/main`): working tree scrubbed of all 4 secrets,
> `.claude/settings.local.json` untracked + gitignored, full scope mapped. This session drives the
> repair to closed **with Scott** (rotation is interactive — guide, don't do silently).
>
> **Read first (in order):** `PASSWORD-LEAK-REPAIR.md` (the runbook — Steps 1–3, exact commands,
> redacted re-derivation), then `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` F-046 and
> `progress_docs/2026-06-24_F-046-credential-leak-remediation.md` for context, and `RUNBOOK.md` §12 +
> §3. The repo IS `scoracle-backend` (its own `.git`) — `cd scoracle-backend` for git ops.
>
> **First step per CLAUDE.md:** `cd scoracle-backend && git fetch && git status`, confirm synced with
> `origin/main` (a parallel Rust session shares this tree and may have pushed), then read the runbook.
>
> **Do, in order:**
> 1. **Rotate/revoke all 4** (the only real fix; risk order in the runbook): delete the abandoned Neon
>    projects (or reset `neondb_owner`); `ALTER ROLE scoracle PASSWORD …` on archbox + update
>    `.env.local` on **both** boxes; rotate/revoke `API_SPORTS_KEY` (confirm if seeder still uses it)
>    and `TWITTER_BEARER_TOKEN`. Verify API + seeder + restore-drill authenticate. Have Scott run
>    creds-bearing commands via `! <cmd>` so they land in-session.
> 2. **Purge history** across `scoracle-backend` + `dotfiles` (+ delete/rewrite the capital-`Scoracle`
>    clone): `pip install git-filter-repo` first, take `--mirror` backups, run the runbook's filter-repo
>    recipe, force-push, then have archbox + archx220 re-clone/hard-reset (back up archx220's local
>    `settings.local.json` first). **Coordinate with the Rust session — no rewrite while it has unpushed
>    work.**
> 3. **Verify** (runbook Step 3): `git log --all -S<literal>` → 0 in every repo; PREPURGE mirrors
>    removed last.
>
> **Handling rule:** never print/commit a literal secret (the classifier blocks the push) — use the
> runbook's redacted-var re-derivation; print only file names / counts.
>
> **Landmines:** never pattern-kill backend procs (F-001 — use `systemctl --user restart
> scoracle-api.service`); migrate-before-restart except drop-column = binary-first (F-022); next free
> migration = 107; `git fetch` before committing, stage only your own files; `099` + `rust/*` are the
> parallel session's — leave untracked.
>
> **At session end:** set F-046 Status → ✅ RESOLVED (if closed) in the findings ledger; update
> `RUNBOOK.md` §12 + this runbook's Status table; write a `progress_docs/` entry; commit + push your own
> files (`git fetch` first); update the `[[first-gpt-audit-execution]]` memory + the MEMORY.md F-046 line.
