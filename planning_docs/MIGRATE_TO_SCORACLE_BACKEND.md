# Migrate scoracle backend: `albapepper/scoracle-data` → `scoracle/scoracle-backend`

## Context

The Scoracle platform consolidated under the `scoracle` GitHub org earlier this year. The flagship frontend (`scoracle-frontend`) and design tokens (`scoracle-tokens`) already moved; the API service backend is the last big repo still living under the personal `albapepper` namespace. The vault flagged this as "unblocked post-cutover" after the 2026-05-03 DNS cutover — that's been live for two weeks, so it's time.

The repo serves `api.scoracle.com` from archbox (self-hosted box, systemd-managed Go binary on `localhost:8000` fronted by a cloudflared tunnel). Goal: move the GitHub remote with full history, repoint the live deployment on archbox to the new remote, and rename the on-disk directory to match. No code changes, no functional changes, no CI added.

## Decisions locked (per user)

1. **Git history**: mirror-push full history. Old repo stays as archive.
2. **Archbox cutover**: rename in place + update remote (preserves `.env.local`, build artifacts, postgres state).
3. **Open issues** (8): leave on old repo. Don't transfer.
4. **CI/CD**: out of scope for this migration. Follow-up task.

## Phase 1 — Create the new repo content (GitHub-side, zero deployment impact)

From any machine with `gh` auth (your laptop is fine):

```bash
cd /tmp
git clone --mirror git@github.com:albapepper/scoracle-data.git
cd scoracle-data.git
git remote set-url --push origin git@github.com:scoracle/scoracle-backend.git
git push --mirror
```

This pushes all branches, tags, and history. After this completes, `scoracle/scoracle-backend` is a full clone of `albapepper/scoracle-data` at the moment of the mirror; the old repo continues to be the live origin until Phase 2.

**Update the description** on `scoracle/scoracle-backend` to match the old repo ("Dedicated data seeding and statistics database management for Scoracle") — currently it's just "API host".

Don't touch settings (default branch, branch protection, secrets) yet — there are none on the source repo worth porting, but verify before moving on.

## Phase 2 — Cut over archbox

SSH to archbox as `sheneveld`. Before touching anything, capture the current state of path references so nothing is missed:

```bash
sudo grep -rn scoracle-data /etc/systemd/system/ /etc/logrotate.d/ /etc/cron.d/ 2>/dev/null
sudo crontab -l 2>/dev/null | grep scoracle-data
crontab -l 2>/dev/null | grep scoracle-data
grep -rn scoracle-data ~/.bashrc ~/.profile ~/.zshrc 2>/dev/null
```

Save the output. Then execute the cutover (estimated downtime: ~30 seconds):

```bash
# 1. Stop the restart-watcher FIRST so it doesn't fire during the rename
sudo systemctl stop scoracle-api-restart.path scoracle-api-restart.service
sudo systemctl stop scoracle-api.service

# 2. Rename the working tree
mv ~/scoracle-data ~/scoracle-backend

# 3. Update the git remote
git -C ~/scoracle-backend remote set-url origin git@github.com:scoracle/scoracle-backend.git
git -C ~/scoracle-backend remote -v   # verify

# 4. Update INSTALLED systemd units in /etc/systemd/system/ (these are copies, not symlinks)
sudo sed -i.bak 's|scoracle-data|scoracle-backend|g' \
  /etc/systemd/system/scoracle-api.service \
  /etc/systemd/system/scoracle-api-restart.service \
  /etc/systemd/system/scoracle-api-restart.path

# 5. Update logrotate + any cron.d files surfaced by the grep above
#    (hand-edit based on what the grep found; use sed if straightforward)

# 6. Update user crontab (cron-scoseed.sh + cron-vibe.sh paths)
crontab -e   # change scoracle-data → scoracle-backend in any matched lines

# 7. Reload systemd and restart
sudo systemctl daemon-reload
sudo systemctl start scoracle-api.service
sudo systemctl start scoracle-api-restart.path
```

**Also update the in-repo systemd templates** so a future `install.sh` re-runs cleanly:

```bash
cd ~/scoracle-backend
sed -i 's|scoracle-data|scoracle-backend|g' \
  scripts/systemd/scoracle-api.service \
  scripts/systemd/scoracle-api-restart.service \
  scripts/systemd/scoracle-api-restart.path \
  scripts/hosting/cron-scoseed.sh \
  scripts/hosting/cron-vibe.sh \
  scripts/install.sh \
  scripts/logrotate.conf  # if it references the path
# Inspect the diff, commit, push (now goes to scoracle/scoracle-backend):
git add -A && git status
git commit -m "Rename working tree path from scoracle-data to scoracle-backend"
git push origin main
```

**Do NOT restart cloudflared.** It maps `api.scoracle.com → localhost:8000`; the rename doesn't change the port. Restarting it would cause a 5-15s 502 window for no benefit — let it transparently reconnect to localhost when the Go service comes back up.

## Phase 3 — Verify

```bash
# Service is up
sudo systemctl status scoracle-api.service scoracle-api-restart.path

# Health endpoint via tunnel
curl -fsS https://api.scoracle.com/api/v1/nba/health
curl -fsS https://api.scoracle.com/api/v1/nfl/health

# Recent journal — look for clean startup, no errors
sudo journalctl -u scoracle-api -n 100 --no-pager

# Cron jobs ran successfully on their next scheduled tick (check the next morning):
sudo journalctl -u cron --since "1 hour ago" | grep -i scoseed
```

From your laptop, hit a real profile page on `scoracle.com` and confirm data loads (the frontend talks to `api.scoracle.com` directly).

## Phase 4 — Wiki + cleanup

Update the vault to reflect the new state:

- `~/scoracleWiki/CLAUDE.md` line 12: change the api row from "albapepper/scoracle-data today; rename to scoracle/scoracle-backend unblocked post-cutover" → "scoracle/scoracle-backend" with local path `~/scoracle-backend`. Mark the migration as done.
- `~/scoracleWiki/wiki/Changelog.md`: add an entry for 2026-05-18 noting the repo migration.

**Wait 24 hours of green operation**, then archive the old GH repo:

```bash
gh repo archive albapepper/scoracle-data --yes
```

Archiving (not deleting) keeps the old issues/history reachable and preserves the rollback path until you're fully confident.

## Rollback (if Phase 2 verification fails)

```bash
sudo systemctl stop scoracle-api.service scoracle-api-restart.path
mv ~/scoracle-backend ~/scoracle-data
git -C ~/scoracle-data remote set-url origin git@github.com:albapepper/scoracle-data.git
# Restore systemd unit backups
sudo mv /etc/systemd/system/scoracle-api.service.bak /etc/systemd/system/scoracle-api.service
sudo mv /etc/systemd/system/scoracle-api-restart.service.bak /etc/systemd/system/scoracle-api-restart.service
sudo mv /etc/systemd/system/scoracle-api-restart.path.bak /etc/systemd/system/scoracle-api-restart.path
sudo systemctl daemon-reload
sudo systemctl start scoracle-api.service scoracle-api-restart.path
```

Old repo is still pushable until archived in Phase 4, so any in-flight commits won't be lost.

## Critical files

**In-repo (will be edited on archbox + pushed):**
- `scripts/systemd/scoracle-api.service`
- `scripts/systemd/scoracle-api-restart.service`
- `scripts/systemd/scoracle-api-restart.path`
- `scripts/systemd/cloudflared.service` (grep first — may not need changes)
- `scripts/hosting/cron-scoseed.sh`
- `scripts/hosting/cron-vibe.sh`
- `scripts/install.sh`
- `scripts/logrotate.conf` (if present and path-referencing)

**System-level on archbox (hand-edited, not in any repo):**
- `/etc/systemd/system/scoracle-api.service`
- `/etc/systemd/system/scoracle-api-restart.service`
- `/etc/systemd/system/scoracle-api-restart.path`
- `/etc/logrotate.d/scoracle-*` (if any)
- User crontab for `sheneveld`

**Untouched:**
- Postgres data dir (lives outside the repo)
- `cloudflared` tunnel config in `/etc/cloudflared/` (port-based, path-agnostic)
- `.env.local` (moves with `mv`, contains no absolute paths to the repo dir — verify by `grep scoracle-data ~/scoracle-backend/.env.local` after rename)

## Out of scope (deferred follow-ups)

- **GitHub Actions / CI** — none exist today; not adding any in this pass.
- **`railway.toml`** — left as-is in the repo. Archbox is the live host; Railway config is dormant but harmless. Revisit later if you want a single source of truth.
- **Issue transfer** — 8 issues remain on `albapepper/scoracle-data` until archived; consult them by URL.
- **`scoracle/scoracle-wiki` Changelog cross-post** — the vault entry is enough; only "milestone" updates land in the public wiki repo per vault conventions.

## Verification summary

End-to-end OK when all of these are green:

1. `scoracle/scoracle-backend` on GitHub has all branches + tags from the old repo (`git ls-remote` parity).
2. `systemctl status scoracle-api.service` is `active (running)` on archbox.
3. `curl https://api.scoracle.com/api/v1/nba/health` returns 200.
4. A live profile load on `scoracle.com` populates data without errors.
5. The next scheduled `cron-scoseed.sh` / `cron-vibe.sh` run completes successfully (next-day check).
6. After 24h green, old repo archived.
