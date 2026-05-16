# Vibe corpus 2-week checkin

## Status: BLOCKED — api.scoracle.com unreachable from remote execution environment

This checkin was attempted on 2026-05-16 by a remote Claude Code session.
All API calls returned HTTP 403 before reaching the origin server.

## What was attempted

| Endpoint | Result |
|---|---|
| `GET https://api.scoracle.com/api/v1/nba/vibe/hottest` | 403 Forbidden |
| `GET https://api.scoracle.com/api/v1/nfl/vibe/hottest` | 403 Forbidden |
| `GET https://api.scoracle.com/api/v1/football/vibe/hottest` | 403 Forbidden |
| `GET https://api.scoracle.com/health` | 403 Forbidden |
| `GET https://api.scoracle.com/` | 403 Forbidden |

## Root cause

TLS inspection confirmed the 403 originates from the **sandbox network policy**,
not the API itself. The TLS handshake completes, but the TLS certificate served
is:

```
issuer: O=Anthropic; CN=sandbox-egress-production TLS Inspection CA
```

Anthropic's managed remote execution environment intercepts the connection and
returns 403 before the request reaches `api.scoracle.com`. The API host is DNS-
resolvable (172.67.176.173 / 104.21.56.39) and TLS negotiates successfully, but
the egress policy blocks the request.

## Repo-side findings (completed)

Progress docs read. Vibe-related entries after 2026-05-02:

- **2026-05-03 `vibe-quality-pass`** — tweet 48h posted_at filter; v3 prompt
  (1-100 direct, eliminates multiples-of-10 bias); corpus candidate filter
  aligned with the 72h news lookback so corpus mode never writes null markers.
- **2026-05-12 `per-rate-and-scoped-percentiles`** — not vibe-pipeline work;
  per-36/per-90 stat expansion and scoped percentiles. Vibe pipeline unchanged.

No duplicate work to avoid. The 2026-05-03 quality pass is the only post-corpus
vibe change.

## Action required

Run the checkin from a machine with direct access to `api.scoracle.com`, or
whitelist egress to `api.scoracle.com` in the remote session's network policy.

Checkin template (tasks 1–4 from the original brief) is ready to execute once
the API is reachable. The local-only follow-up queries from the brief are
reproduced below for convenience.

## Local-only follow-ups

For the user to run before deciding to retire `-mode batch`:

a. **Coverage breakdown:**
   ```sql
   SELECT sport, entity_type, trigger_type,
          COUNT(*) FILTER (WHERE sentiment IS NOT NULL) scored,
          COUNT(*) FILTER (WHERE sentiment IS NULL) null_sent
   FROM vibe_scores
   WHERE generated_at > NOW() - INTERVAL '14 days'
   GROUP BY 1,2,3
   ORDER BY 1,2,3;
   ```
   Expect: NFL and FOOTBALL show steady scored rows daily; null_sent is a
   small fraction of scored.

b. **Cron installed?**
   ```bash
   crontab -l | grep cron-vibe
   ```
   Expect: `0 0,12 * * * /home/sheneveld/scoracle-data/scripts/hosting/cron-vibe.sh -mode corpus`
   If missing, the API stale readings are explained by a missing cron install.

c. **Unique batch catches:**
   Any (entity_id, sport, day) pair scored ONLY via fixture-driven batch (not
   corpus) in the last 14 days? Run a manual SQL comparison or re-run
   `-mode batch -sport all` on a fresh day and diff against the corpus run.
   If <5 unique catches over the period, safe to retire batch.

## Recommendation

Cannot determine from API-side signal alone.
Run the checkin from a machine with access to the live API, then re-evaluate.
