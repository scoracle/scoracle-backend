# 2026-05-18 — Repo migration: `albapepper/scoracle-data` → `scoracle/scoracle-backend`

## Goal

Consolidate the backend repo under the `scoracle` GitHub org alongside `scoracle-frontend` and `scoracle-tokens`. Rename the archbox working tree from `~/scoracle-data` to `~/scoracle-backend` in lockstep. No code or feature changes — pure repo+path rename. Plan: [`planning_docs/MIGRATE_TO_SCORACLE_BACKEND.md`](../planning_docs/MIGRATE_TO_SCORACLE_BACKEND.md).

## Decisions (carried over from the plan)

1. Mirror-push full history; old repo stays as archive.
2. Rename archbox dir in place (preserves `.env.local`, build artifacts, postgres state).
3. Open issues on `albapepper/scoracle-data` are not transferred.
4. CI/CD is out of scope; revisit later.

## What actually happened

### Phase 1 — GitHub side

`git clone --mirror` of the source repo in `/tmp`, push-url repointed to `scoracle/scoracle-backend`, `git push --mirror`. All 9 branches + HEAD landed on the new repo. GitHub rejects pushes to `refs/pull/*` (these are managed per-repo and don't transfer) — expected, ignore.

### Phase 2 — archbox cutover

Plan-vs-reality divergences worth noting:

- **User-mode systemd, not system-mode.** The plan referenced `/etc/systemd/system/` + `sudo systemctl`; reality is `~/.config/systemd/user/` + `systemctl --user`. No sudo needed for the service operations.
- **Unit name was `scoracle-api.path`**, not `scoracle-api-restart.path` as the plan stated. `scoracle-api-restart.service` exists as the path-trigger target but has no path refs in it.
- **6 user crontab entries** to update (cron-scoseed.sh ×3, backup-postgres.sh, cron-vibe.sh, recompute-tiers.sh); updated via `crontab -l | sed | crontab -`. crontab auto-backed up at `~/.cache/crontab/crontab.bak`.
- **No `gh` CLI on archbox** — description update + repo archive deferred to manual follow-up via the GitHub web UI.

Sequence: stash WIP (vibe listen-notify work was in flight) → `systemctl --user stop scoracle-api.path scoracle-api.service` → `mv ~/scoracle-data ~/scoracle-backend` → `git remote set-url` → sed installed user systemd units (with `.bak` backups) → sed user crontab → `daemon-reload` + start → verify → sed in-repo path refs → commit + push.

Total observed downtime: ~42s (stop at 21:41:23, start at 21:42:05).

### Phase 3 — verify

- `systemctl --user is-active scoracle-api.service scoracle-api.path` → both active
- `curl http://localhost:8000/api/v1/{nba,nfl}/health` → 200 healthy
- `curl https://api.scoracle.com/api/v1/nba/health` (via cloudflared) → 200 healthy
- Journal showed clean startup: DB connected, cache enabled, news-volume + percentile listeners attached
- cloudflared was deliberately *not* restarted (port-mapped to localhost:8000; reconnected transparently when the API came back up)

### Phase 4 — wiki + follow-ups

- Updated `~/scoracleWiki/CLAUDE.md` backend row to reflect the new repo + path
- Added Changelog.md entry under 2026 Q2
- This progress doc

## Quick reference

| Thing | Before | After |
|---|---|---|
| GitHub repo | `albapepper/scoracle-data` | `scoracle/scoracle-backend` |
| Local path | `~/scoracle-data` | `~/scoracle-backend` |
| Go module path | `github.com/albapepper/scoracle-data` | unchanged (separate migration) |
| Service runtime | user-mode systemd | user-mode systemd (unchanged) |
| Cloudflared mapping | localhost:8000 | localhost:8000 (unchanged) |

The Go module path rename is a separate, larger change (it touches every `import` line + go.mod + go.sum + any downstream consumers) and was intentionally not folded into this cutover.

## Files touched in this commit

- `scripts/systemd/scoracle-api.service`, `scripts/systemd/scoracle-api.path` — path templates
- `scripts/hosting/{cron-scoseed,cron-vibe,backup-postgres,recompute-tiers,restore-drill}.sh`
- `scripts/hosting/{crontab.example,logrotate.conf}`
- `README.md` — repository-layout tree

Out-of-band changes on archbox (not in any repo): `~/.config/systemd/user/scoracle-api.{service,path}`, user crontab.

## Outstanding manual follow-ups

1. Update the `scoracle/scoracle-backend` repo description on github.com — currently "API host"; should match the old repo's "Dedicated data seeding and statistics database management for Scoracle".
2. After 24h of green operation: `gh repo archive albapepper/scoracle-data --yes` (or via web UI — Settings → Archive). Archiving preserves issues + history + rollback path.
3. Stash from this session is retained as a safety net (`git stash list` shows it as `stash@{0}: ... vibe-listen-notify WIP — pre-migration stash`). It was popped and the vibe WIP is back in the working tree; drop with `git stash drop stash@{0}` once you've verified nothing was lost.
4. Backup files left behind:
   - `~/.config/systemd/user/scoracle-api.service.bak`
   - `~/.config/systemd/user/scoracle-api.path.bak`
   - `/tmp/crontab-pre-migration.bak` (will clear on reboot)
   - `~/.cache/crontab/crontab.bak` (managed by crontab(1))

## Rollback (if needed)

```bash
systemctl --user stop scoracle-api.path scoracle-api.service
mv ~/scoracle-backend ~/scoracle-data
git -C ~/scoracle-data remote set-url origin git@github.com:albapepper/scoracle-data.git
mv ~/.config/systemd/user/scoracle-api.service.bak ~/.config/systemd/user/scoracle-api.service
mv ~/.config/systemd/user/scoracle-api.path.bak ~/.config/systemd/user/scoracle-api.path
crontab /tmp/crontab-pre-migration.bak  # or ~/.cache/crontab/crontab.bak
systemctl --user daemon-reload
systemctl --user start scoracle-api.service scoracle-api.path
```

The old repo is still pushable until step 2 above; in-flight commits aren't lost.
