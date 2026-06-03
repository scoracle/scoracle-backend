# 2026-06-03 — API auto-restart hardening (systemd --user)

## Goal

The self-hosted Go API is the single point of failure (it 502'd twice today). Make it
self-heal so a crash ≠ outage.

## Root cause found

A `scoracle-api.service` systemd **--user** unit already existed and was `enabled`, but
it had been `failed (start-limit-hit)` since 2026-06-01: its `WorkingDirectory`/
`ExecStart` still pointed at the **pre-consolidation** path `/home/sheneveld/scoracle-backend`
(repos moved under `~/scoracle/` on 2026-05-19). It exited `203/EXEC` (binary not found)
5× in 60s, hit the `StartLimitBurst=5`/`StartLimitIntervalSec=60` cap, and systemd
stopped retrying. So the API had been running as a **manual detached process**, with no
auto-restart — which is why it stayed down.

## Fix

Rewrote `~/.config/systemd/user/scoracle-api.service`:
- Corrected paths to `/home/sheneveld/scoracle/scoracle-backend` (WorkingDirectory +
  ExecStart `go/bin/scoracle-api` + the two `EnvironmentFile=-` lines).
- `Restart=always`, `RestartSec=3`.
- **`StartLimitIntervalSec=0`** — disables the start-rate limiter so the service never
  permanently gives up (the 5-in-60s cap is exactly what left it down).
- Logs to journal (`journalctl --user -u scoracle-api -f`).

Then: `systemctl --user daemon-reload`, `reset-failed`, stopped the manual process,
`systemctl --user start scoracle-api`. Linger was already on (`Linger=yes`) so it runs
without a login session and survives reboot.

## Verification

- `is-active: active`, serving (local :8000 + `api.scoracle.com` → 200).
- **Self-heal proven**: `kill -9` the MainPID → API back in ~3s with a new MainPID,
  `NRestarts: 1`, 200 again.

## Note

The unit lives outside the repo (`~/.config/systemd/user/`). For new machines, recreate
it from the content above (consider templating it into `~/scoracleWiki/bootstrap.sh`).
