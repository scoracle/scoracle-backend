# OpenCode Tunnel Runbook

This folder documents the optional `opencode.scoracle.com` Cloudflare Tunnel.
It is intentionally separate from the main Scoracle API tunnel so the OpenCode
surface can stay off unless it is needed.

## Current State

As of July 3, 2026, the OpenCode tunnel is stopped and disabled:

```bash
systemctl --user status cloudflared.service
```

Expected state while OpenCode is off:

```text
Loaded: loaded (.../cloudflared.service; disabled)
Active: inactive (dead)
```

The production API tunnel is separate and should remain running as a system
service:

```bash
systemctl status cloudflared.service
```

That system service reads:

```text
/etc/cloudflared/config.yml
```

The optional OpenCode/user tunnel reads:

```text
/home/sheneveld/.cloudflared/config.yml
```

Do not stop the system-level `cloudflared.service` when only turning off
OpenCode. The system-level service carries `api.scoracle.com`.

## What The OpenCode Tunnel Does

The user-level Cloudflared config currently includes this hostname:

```yaml
- hostname: opencode.scoracle.com
  service: http://localhost:8000
```

If OpenCode later runs on a different local port, edit
`/home/sheneveld/.cloudflared/config.yml` and change only the
`opencode.scoracle.com` service target.

Keep `api.scoracle.com` on the system-level config unless deliberately changing
the production API tunnel topology.

## Start The OpenCode Tunnel

Start and re-enable the user-level tunnel:

```bash
systemctl --user enable --now cloudflared.service
```

Verify it is running:

```bash
systemctl --user status cloudflared.service
pgrep -af 'cloudflared|opencode'
```

You should see a user-level process similar to:

```text
/usr/bin/cloudflared --no-autoupdate tunnel run
```

Check logs:

```bash
journalctl --user -u cloudflared.service -f
```

Probe the hostname:

```bash
curl --max-time 10 -sS -D - https://opencode.scoracle.com/ -o /tmp/opencode-smoke.body
head -c 300 /tmp/opencode-smoke.body
```

If the hostname connects but returns the wrong application, check the local
`service:` target in `/home/sheneveld/.cloudflared/config.yml`.

## Stop The OpenCode Tunnel

Stop it for the current boot and keep it off across future logins:

```bash
systemctl --user disable --now cloudflared.service
```

Verify only the production API tunnel remains:

```bash
pgrep -af 'cloudflared|opencode'
systemctl --user status cloudflared.service
systemctl status cloudflared.service
```

Expected process list after stopping OpenCode:

```text
/usr/bin/cloudflared --no-autoupdate --config /etc/cloudflared/config.yml tunnel run
```

Re-smoke the production API after stopping OpenCode:

```bash
scripts/hosting/tunnel-smoke.sh https://api.scoracle.com https://scoracle.com
```

Expected summary:

```text
Summary: 19 passed, 0 failed, 0 warned
```

## Quick Commands

```bash
# Start OpenCode tunnel
systemctl --user enable --now cloudflared.service

# Stop OpenCode tunnel
systemctl --user disable --now cloudflared.service

# Follow OpenCode tunnel logs
journalctl --user -u cloudflared.service -f

# Confirm the production API tunnel is still running
systemctl status cloudflared.service

# Confirm only one Cloudflared process remains after shutdown
pgrep -af 'cloudflared|opencode'
```

## Safety Notes

- `systemctl --user ... cloudflared.service` controls the optional OpenCode
  tunnel.
- `systemctl ... cloudflared.service` controls the production system tunnel.
- The production API smoke test should pass before and after OpenCode changes.
- If both user and system Cloudflared services are running, two tunnel processes
  are expected. If OpenCode is supposed to be off, only the system process should
  remain.
